//! Conformance with the Poli Page API contract
//! (<https://github.com/poli-page/api-contract>).
//!
//! CI copies the pinned `contract.json` with the `poli-page/api-contract`
//! action and sets `POLI_CONTRACT` to its path. Every `kind: "sdk"` case runs
//! as its own test, named by the case id, against a local HTTP/1.1 server on
//! `127.0.0.1:0` that answers the case's `responses` in order (`drop` closes
//! the connection, `hang` never answers) and records each request. The test
//! then compares the recorded requests, the `on_request` / `on_retry` events
//! and the normalized result with the case's `expect`. Timing is checked as
//! lower bounds, measured on the server, in integer milliseconds.
//!
//! `tests/contract-skip.json` maps the case ids allowed to fail to the
//! reason. A listed case that passes, a listed id the contract does not have,
//! or a listed case that does not run here fails the run.
//!
//! Locally: `POLI_CONTRACT=/path/to/contract.json cargo test --test contract`.
//! Without it the suite is skipped, except when `CI` is set, where a missing
//! contract is an error.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures_util::StreamExt;
use libtest_mimic::{Arguments, Failed, Trial};
use poli_page::{
    DocumentDescriptor, Error, Orientation, PageFormat, PoliPage, RenderInput, RenderMetadata,
    Thumbnail, ThumbnailFormat, ThumbnailOptions,
};
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;

// ---------------------------------------------------------------------------
// Contract format 1 (README of poli-page/api-contract)
// ---------------------------------------------------------------------------

const KNOWN_FORMAT_VERSION: u64 = 1;

/// This SDK's name in `policy.cancellationSdks`.
const SDK_NAME: &str = "rust";

/// The client's key when a case sets no `apiKey`.
const TEST_API_KEY: &str = "pp_sandbox_00000000000000000000000000000000";

/// No case comes near this; it only turns a stuck case into a failure.
const CASE_TIMEOUT: Duration = Duration::from_secs(60);

/// The normalization ids this file applies; each must be listed in the
/// contract's policy.
const NORMALIZATIONS_USED: [&str; 8] = [
    "N-STATUS-NULL",
    "N-REQID-NULL",
    "N-BYTES",
    "N-DELETE-NULL",
    "N-THUMBS-ARRAY",
    "N-PREVIEW",
    "N-DESCRIPTOR",
    "N-DURATION-MS",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Contract {
    policy: Policy,
    cases: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Policy {
    cancellation_sdks: Vec<String>,
    normalizations: Vec<Normalization>,
}

#[derive(Deserialize)]
struct Normalization {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SdkCase {
    requires: Option<String>,
    api_key: Option<String>,
    options: ClientOptions,
    call: SdkCall,
    responses: Vec<Value>,
    expect: Expect,
    stop_after_retry_events: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientOptions {
    max_retries: u32,
    retry_delay_ms: u64,
    timeout_ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SdkCall {
    method: String,
    args: Vec<Value>,
    cancel_after_ms: Option<u64>,
}

#[derive(Deserialize)]
struct Expect {
    requests: Vec<Value>,
    events: Vec<Value>,
    result: Value,
}

fn load_contract(path: &str) -> Result<Contract, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let raw: Value = serde_json::from_str(&text).map_err(|e| format!("{path} is not JSON: {e}"))?;
    let version = raw
        .get("formatVersion")
        .ok_or_else(|| format!("{path} is not a contract.json: it has no formatVersion"))?;
    if version.as_u64() != Some(KNOWN_FORMAT_VERSION) {
        return Err(format!(
            "contract.json formatVersion {version} is not supported (this test reads format \
             {KNOWN_FORMAT_VERSION}): update tests/contract.rs"
        ));
    }
    serde_json::from_value(raw).map_err(|e| format!("{path}: unexpected contract shape: {e}"))
}

fn load_skip_list() -> Result<BTreeMap<String, String>, String> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/contract-skip.json");
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let raw: Map<String, Value> = serde_json::from_str(&text).map_err(|e| {
        format!("tests/contract-skip.json must be an object of case id to reason: {e}")
    })?;
    let mut skip = BTreeMap::new();
    for (id, reason) in raw {
        match reason.as_str() {
            Some(reason) if !reason.trim().is_empty() => {
                skip.insert(id, reason.to_owned());
            }
            _ => return Err(format!("tests/contract-skip.json: \"{id}\" needs a reason")),
        }
    }
    Ok(skip)
}

// ---------------------------------------------------------------------------
// Placeholders: {{mock}}, {{httpDate+N}}
// ---------------------------------------------------------------------------

fn placeholder_regex() -> Regex {
    Regex::new(r"\{\{([^}]*)\}\}").expect("placeholder regex")
}

fn substitute_str(text: &str, mock: &str) -> Result<String, String> {
    let date = Regex::new(r"^httpDate\+(\d+)$").expect("httpDate regex");
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for caps in placeholder_regex().captures_iter(text) {
        let whole = caps.get(0).expect("match");
        out.push_str(&text[last..whole.start()]);
        let name = &caps[1];
        if name == "mock" {
            out.push_str(mock);
        } else if let Some(d) = date.captures(name) {
            let seconds: u64 = d[1].parse().map_err(|e| format!("{{{{{name}}}}}: {e}"))?;
            // An HTTP-date has whole seconds: round to the nearest one.
            let at = SystemTime::now() + Duration::from_secs(seconds);
            let millis = at
                .duration_since(UNIX_EPOCH)
                .expect("after epoch")
                .as_millis();
            let seconds = u64::try_from((millis + 500) / 1000).expect("seconds fit in u64");
            let rounded = UNIX_EPOCH + Duration::from_secs(seconds);
            out.push_str(&httpdate::fmt_http_date(rounded));
        } else {
            return Err(format!("unknown contract placeholder {{{{{name}}}}}"));
        }
        last = whole.end();
    }
    out.push_str(&text[last..]);
    Ok(out)
}

fn substitute(value: &Value, mock: &str) -> Result<Value, String> {
    Ok(match value {
        Value::String(s) => Value::String(substitute_str(s, mock)?),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| substitute(item, mock))
                .collect::<Result<_, _>>()?,
        ),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| Ok((k.clone(), substitute(v, mock)?)))
                .collect::<Result<_, String>>()?,
        ),
        other => other.clone(),
    })
}

// ---------------------------------------------------------------------------
// Mock server
// ---------------------------------------------------------------------------

enum Body {
    Empty,
    Json(Value),
    Text(String),
    Bytes(Vec<u8>),
}

enum Step {
    Answer {
        status: u16,
        headers: Vec<(String, String)>,
        body: Body,
    },
    Drop,
    Hang,
}

fn parse_step(raw: &Value) -> Result<Step, String> {
    let obj = raw
        .as_object()
        .ok_or_else(|| format!("response step is not an object: {raw}"))?;
    if let Some(fault) = obj.get("fault") {
        return match fault.as_str() {
            Some("drop") => Ok(Step::Drop),
            Some("hang") => Ok(Step::Hang),
            _ => Err(format!("unknown fault {fault}")),
        };
    }
    let status = obj
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|s| u16::try_from(s).ok())
        .ok_or_else(|| format!("response step has no status: {raw}"))?;
    let mut headers = Vec::new();
    if let Some(h) = obj.get("headers") {
        let h = h.as_object().ok_or("response headers must be an object")?;
        for (name, value) in h {
            let value = value
                .as_str()
                .ok_or_else(|| format!("header {name} must be a string"))?;
            headers.push((name.clone(), value.to_owned()));
        }
    }
    let bodies = ["json", "text", "base64"]
        .iter()
        .filter(|k| obj.contains_key(**k))
        .count();
    if bodies > 1 {
        return Err(format!("response step has more than one body: {raw}"));
    }
    let body = if let Some(v) = obj.get("json") {
        Body::Json(v.clone())
    } else if let Some(v) = obj.get("text") {
        Body::Text(v.as_str().ok_or("text must be a string")?.to_owned())
    } else if let Some(v) = obj.get("base64") {
        Body::Bytes(decode_base64(v.as_str().ok_or("base64 must be a string")?)?)
    } else {
        Body::Empty
    };
    Ok(Step::Answer {
        status,
        headers,
        body,
    })
}

fn decode_base64(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            _ => return Err(format!("invalid base64 byte {c:#x}")),
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from(acc >> bits).expect("one byte"));
            acc &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

struct Recorded {
    method: String,
    /// The request target: path and query.
    path: String,
    /// Lowercase names; repeated headers joined with `, `.
    headers: HashMap<String, String>,
    body: Vec<u8>,
    /// When the request head reached the server.
    at: Instant,
}

type Requests = Arc<Mutex<Vec<Recorded>>>;

struct MockServer {
    origin: String,
    requests: Requests,
    accept: tokio::task::JoinHandle<()>,
}

impl MockServer {
    async fn start(steps: Vec<Step>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock");
        let origin = format!("http://{}", listener.local_addr().expect("mock addr"));
        let requests: Requests = Arc::default();
        let steps = Arc::new(steps);
        let accept = {
            let (requests, origin) = (Arc::clone(&requests), origin.clone());
            tokio::spawn(async move {
                // Dropping the set (when this task is aborted) aborts every
                // connection, hung ones included.
                let mut connections = tokio::task::JoinSet::new();
                while let Ok((stream, _)) = listener.accept().await {
                    connections.spawn(serve_connection(
                        stream,
                        Arc::clone(&steps),
                        Arc::clone(&requests),
                        origin.clone(),
                    ));
                }
            })
        };
        Self {
            origin,
            requests,
            accept,
        }
    }

    fn close(self) -> Vec<Recorded> {
        self.accept.abort();
        let mut requests = self.requests.lock().unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut *requests)
    }
}

async fn serve_connection(
    mut stream: TcpStream,
    steps: Arc<Vec<Step>>,
    requests: Requests,
    origin: String,
) {
    let mut buf = Vec::new();
    loop {
        let Some(request) = read_request(&mut stream, &mut buf).await else {
            return;
        };
        let index = {
            let mut recorded = requests.lock().unwrap_or_else(PoisonError::into_inner);
            recorded.push(request);
            recorded.len() - 1
        };
        let written = match steps.get(index) {
            // More requests than the case has answers: make it visible in the diff.
            None => {
                let body = Body::Json(json!({ "error": "MOCK_EXHAUSTED" }));
                write_answer(&mut stream, 599, &[], &body, &origin).await
            }
            // Dropping the stream closes the connection without an answer.
            Some(Step::Drop) => return,
            Some(Step::Hang) => std::future::pending().await,
            Some(Step::Answer {
                status,
                headers,
                body,
            }) => write_answer(&mut stream, *status, headers, body, &origin).await,
        };
        if written.is_err() {
            return;
        }
    }
}

/// Reads one HTTP/1.1 request; `None` once the client closes the connection.
async fn read_request(stream: &mut TcpStream, buf: &mut Vec<u8>) -> Option<Recorded> {
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        let mut chunk = [0u8; 8192];
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let at = Instant::now();
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    buf.drain(..head_end + 4);
    let mut lines = head.split("\r\n");
    let mut request_line = lines.next()?.split(' ');
    let method = request_line.next()?.to_owned();
    let path = request_line.next()?.to_owned();
    let mut headers: HashMap<String, String> = HashMap::new();
    for line in lines {
        let (name, value) = line.split_once(':')?;
        headers
            .entry(name.trim().to_ascii_lowercase())
            .and_modify(|v| {
                v.push_str(", ");
                v.push_str(value.trim());
            })
            .or_insert_with(|| value.trim().to_owned());
    }
    // reqwest sends a Content-Length for every body the SDK builds.
    let length: usize = headers
        .get("content-length")
        .map_or(Some(0), |v| v.parse().ok())?;
    while buf.len() < length {
        let mut chunk = [0u8; 8192];
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = buf.drain(..length).collect();
    Some(Recorded {
        method,
        path,
        headers,
        body,
        at,
    })
}

async fn write_answer(
    stream: &mut TcpStream,
    status: u16,
    headers: &[(String, String)],
    body: &Body,
    origin: &str,
) -> std::io::Result<()> {
    let mut head_lines = Vec::new();
    for (name, value) in headers {
        let value = substitute_str(value, origin).map_err(std::io::Error::other)?;
        head_lines.push((name.clone(), value));
    }
    let has = |name: &str| head_lines.iter().any(|(n, _)| n.eq_ignore_ascii_case(name));
    let bytes: Vec<u8> = match body {
        Body::Empty => Vec::new(),
        Body::Json(v) => {
            let v = substitute(v, origin).map_err(std::io::Error::other)?;
            if !has("content-type") {
                head_lines.push(("content-type".into(), "application/json".into()));
            }
            serde_json::to_vec(&v).map_err(std::io::Error::other)?
        }
        Body::Text(t) => substitute_str(t, origin)
            .map_err(std::io::Error::other)?
            .into_bytes(),
        Body::Bytes(b) => b.clone(),
    };
    let reason = http::StatusCode::from_u16(status)
        .ok()
        .and_then(|s| s.canonical_reason())
        .unwrap_or("Unknown");
    let mut out = format!("HTTP/1.1 {status} {reason}\r\n");
    for (name, value) in &head_lines {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str(&format!("content-length: {}\r\n\r\n", bytes.len()));
    stream.write_all(out.as_bytes()).await?;
    stream.write_all(&bytes).await?;
    stream.flush().await
}

// ---------------------------------------------------------------------------
// Normalizations (policy.normalizations): the only rewrites applied to a result
// ---------------------------------------------------------------------------

/// N-DURATION-MS.
fn millis(d: Duration) -> u64 {
    // Rounded, so a 999.9 ms gap is the 1000 ms it was asked to be.
    let ms = (d.as_secs_f64() * 1000.0).round();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ms = ms as u64;
    ms
}

/// N-BYTES.
fn bytes_value(data: &[u8]) -> Value {
    let digest = ring::digest::digest(&ring::digest::SHA256, data);
    let hex: String = digest.as_ref().iter().map(|b| format!("{b:02x}")).collect();
    json!({ "sha256": hex, "length": data.len() })
}

/// N-DESCRIPTOR: the fields of sdk-specification.md §7.1.
fn descriptor_value(d: &DocumentDescriptor) -> Value {
    json!({
        "documentId": d.document_id,
        "organizationId": d.organization_id,
        "projectId": d.project_id,
        "projectSlug": d.project_slug,
        "templateId": d.template_id,
        "templateSlug": d.template_slug,
        "environment": d.environment,
        "apiKeyId": d.api_key_id,
        "format": d.format,
        "orientation": d.orientation,
        "locale": d.locale,
        "pageCount": d.page_count,
        "sizeBytes": d.size_bytes,
        "createdAt": d.created_at,
        "metadata": d.metadata,
        "presignedPdfUrl": d.presigned_pdf_url,
        "expiresAt": d.expires_at,
    })
}

/// N-THUMBS-ARRAY: the SDK already returns the unwrapped array.
fn thumbnail_value(t: &Thumbnail) -> Value {
    json!({
        "page": t.page,
        "width": t.width,
        "height": t.height,
        "contentType": t.content_type,
        "data": t.data,
    })
}

/// N-STATUS-NULL, N-REQID-NULL.
fn error_value(err: &Error) -> Value {
    json!({
        "code": err.code(),
        "status": err.status(),
        "message": err.message(),
        "requestId": err.request_id(),
        "retryable": err.is_retryable(),
        "isAuthError": err.is_auth_error(),
        "isRateLimitError": err.is_rate_limit_error(),
        "isValidationError": err.is_validation_error(),
        "isNetworkError": err.is_network_error(),
    })
}

// ---------------------------------------------------------------------------
// Calling the SDK
// ---------------------------------------------------------------------------

fn string_field(value: &Value, what: &str) -> Result<String, String> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{what} must be a string, got {value}"))
}

fn typed<T: serde::de::DeserializeOwned>(value: &Value, what: &str) -> Result<T, String> {
    serde_json::from_value(value.clone()).map_err(|e| format!("{what}: {e}"))
}

fn render_input(value: Option<&Value>) -> Result<RenderInput, String> {
    let obj = value
        .and_then(Value::as_object)
        .ok_or("call.args[0] must be a render input object")?;
    let mut input = RenderInput::default();
    for (key, v) in obj {
        match key.as_str() {
            "project" => input.project = string_field(v, "project")?,
            "template" => input.template = string_field(v, "template")?,
            "data" => input.data = v.clone(),
            "format" => input.format = Some(typed::<PageFormat>(v, "format")?),
            "orientation" => input.orientation = Some(typed::<Orientation>(v, "orientation")?),
            "locale" => input.locale = Some(string_field(v, "locale")?),
            "metadata" => input.metadata = Some(typed::<RenderMetadata>(v, "metadata")?),
            other => return Err(format!("{other} is not a RenderInput field")),
        }
    }
    Ok(input)
}

fn thumbnail_options(value: Option<&Value>) -> Result<Option<ThumbnailOptions>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    let obj = value
        .as_object()
        .ok_or("call.args[1] must be a thumbnail options object")?;
    let mut options = ThumbnailOptions::new();
    for (key, v) in obj {
        match key.as_str() {
            "width" => options.width = Some(typed(v, "width")?),
            "format" => options.format = Some(typed::<ThumbnailFormat>(v, "format")?),
            "quality" => options.quality = Some(typed(v, "quality")?),
            "pages" => options.pages = Some(typed(v, "pages")?),
            other => return Err(format!("{other} is not a ThumbnailOptions field")),
        }
    }
    Ok(Some(options))
}

fn id_arg(args: &[Value]) -> Result<String, String> {
    string_field(args.first().unwrap_or(&Value::Null), "call.args[0]")
}

/// Calls `call.method`. The outer error is the harness's (a case this file
/// cannot express); the inner result is the SDK's, normalized.
async fn invoke(client: &PoliPage, call: &SdkCall) -> Result<Result<Value, Error>, String> {
    let args = &call.args;
    Ok(match call.method.as_str() {
        "render.pdf" => {
            let input = render_input(args.first())?;
            client.render.pdf(input).await.map(|b| bytes_value(&b))
        }
        "render.pdfStream" => {
            let input = render_input(args.first())?;
            match client.render.pdf_stream(input).await {
                Err(e) => Err(e),
                Ok(mut stream) => {
                    let mut data = Vec::new();
                    let mut failure = None;
                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Ok(bytes) => data.extend_from_slice(&bytes),
                            Err(e) => {
                                failure = Some(e);
                                break;
                            }
                        }
                    }
                    failure.map_or_else(|| Ok(bytes_value(&data)), Err)
                }
            }
        }
        "render.document" => {
            let input = render_input(args.first())?;
            client
                .render
                .document(input)
                .await
                .map(|d| descriptor_value(&d))
        }
        "documents.get" => client
            .documents
            .get(&id_arg(args)?)
            .await
            .map(|d| descriptor_value(&d)),
        // N-PREVIEW
        "documents.preview" => client
            .documents
            .preview(&id_arg(args)?)
            .await
            .map(|p| json!({ "html": p.html, "pageCount": p.page_count })),
        "documents.thumbnails" => {
            let options = thumbnail_options(args.get(1))?;
            client
                .documents
                .thumbnails(&id_arg(args)?, options)
                .await
                .map(|t| Value::Array(t.iter().map(thumbnail_value).collect()))
        }
        // N-DELETE-NULL
        "documents.delete" => client
            .documents
            .delete(&id_arg(args)?)
            .await
            .map(|()| Value::Null),
        other => return Err(format!("unknown SDK method in the contract: {other}")),
    })
}

// ---------------------------------------------------------------------------
// Running and comparing one case
// ---------------------------------------------------------------------------

fn uuid_v4() -> Regex {
    Regex::new(r"^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
        .expect("uuid regex")
}

/// The recorded requests in the shape of `expect.requests`. A field that
/// breaks the expectation keeps the observed value, so the diff shows it.
fn describe_requests(recorded: &[Recorded], expected: &[Value], api_key: &str) -> Value {
    let uuid = uuid_v4();
    let mut first_key: Option<&str> = None;
    let mut out = Vec::new();
    for (i, req) in recorded.iter().enumerate() {
        let want = expected.get(i);
        let auth = match req.headers.get("authorization") {
            None => "none".to_owned(),
            Some(v) if *v == format!("Bearer {api_key}") => "bearer".to_owned(),
            Some(v) => format!("unexpected Authorization: {v}"),
        };
        let idempotency_key = match req.headers.get("idempotency-key") {
            None => "absent".to_owned(),
            Some(key) if !uuid.is_match(key) => format!("not a lowercase UUID v4: {key}"),
            Some(key) => {
                let first = *first_key.get_or_insert(key.as_str());
                if key == first {
                    "stable".to_owned()
                } else {
                    format!("changed: {first} then {key}")
                }
            }
        };
        let mut entry = Map::new();
        entry.insert("method".into(), json!(req.method));
        entry.insert("path".into(), json!(req.path));
        entry.insert("auth".into(), json!(auth));
        entry.insert("idempotencyKey".into(), json!(idempotency_key));
        if want.is_some_and(|w| w.get("json").is_some()) {
            let body = serde_json::from_slice::<Value>(&req.body).unwrap_or_else(|_| {
                json!(format!(
                    "body is not JSON: {}",
                    String::from_utf8_lossy(&req.body)
                ))
            });
            entry.insert("json".into(), body);
        }
        if let Some(min) = want.and_then(|w| w.get("minGapMsBefore")) {
            let gap = i
                .checked_sub(1)
                .map_or(0, |p| millis(req.at.duration_since(recorded[p].at)));
            let ok = min.as_u64().is_some_and(|m| gap >= m);
            entry.insert(
                "minGapMsBefore".into(),
                if ok {
                    min.clone()
                } else {
                    json!(format!("only {gap} ms"))
                },
            );
        }
        out.push(Value::Object(entry));
    }
    Value::Array(out)
}

/// The observed events, with a delay inside an expected range shown as that
/// range (`max` is exclusive).
fn describe_events(actual: &[Value], expected: &[Value]) -> Value {
    let out = actual
        .iter()
        .enumerate()
        .map(|(i, event)| {
            let range = expected
                .get(i)
                .filter(|w| w["type"] == "retry" && event["type"] == "retry")
                .and_then(|w| w.get("delayMs"))
                .filter(|d| d.is_object());
            match (range, event["delayMs"].as_u64()) {
                (Some(range), Some(delay))
                    if range["min"].as_u64().is_some_and(|min| delay >= min)
                        && range["max"].as_u64().is_some_and(|max| delay < max) =>
                {
                    let mut shown = event.clone();
                    shown["delayMs"] = range.clone();
                    shown
                }
                _ => event.clone(),
            }
        })
        .collect();
    Value::Array(out)
}

struct Prefix<'a> {
    events: &'a [Value],
    requests: &'a [Value],
    check_result: bool,
}

/// Expectations cut at the `stopAfterRetryEvents`-th retry event, when the
/// case sets one.
fn expected_prefix(c: &SdkCase) -> Result<Prefix<'_>, String> {
    let Some(stop) = c.stop_after_retry_events else {
        return Ok(Prefix {
            events: &c.expect.events,
            requests: &c.expect.requests,
            check_result: true,
        });
    };
    let mut retries = 0;
    let end = c
        .expect
        .events
        .iter()
        .position(|e| {
            if e["type"] == "retry" {
                retries += 1;
            }
            e["type"] == "retry" && retries == stop
        })
        .ok_or_else(|| format!("stopAfterRetryEvents {stop}: the case has fewer retries"))?;
    let events = &c.expect.events[..=end];
    let attempts = events.iter().filter(|e| e["type"] == "request").count();
    Ok(Prefix {
        events,
        requests: &c.expect.requests[..attempts.min(c.expect.requests.len())],
        check_result: false,
    })
}

fn compare(what: &str, actual: &Value, expected: &Value, failures: &mut Vec<String>) {
    if actual != expected {
        failures.push(format!(
            "{what} differ\n--- expected\n{}\n--- actual\n{}",
            serde_json::to_string_pretty(expected).unwrap_or_default(),
            serde_json::to_string_pretty(actual).unwrap_or_default(),
        ));
    }
}

async fn run_case(c: &SdkCase) -> Result<(), String> {
    match c.requires.as_deref() {
        None | Some("cancellation") => {}
        Some(other) => return Err(format!("unknown case requirement: {other}")),
    }
    if c.call.cancel_after_ms.is_some() {
        return Err("call.cancelAfterMs needs cancellation, which this SDK does not run".into());
    }
    let steps = c
        .responses
        .iter()
        .map(parse_step)
        .collect::<Result<Vec<_>, _>>()?;
    let want = expected_prefix(c)?;
    let server = MockServer::start(steps).await;
    let api_key = c.api_key.clone().unwrap_or_else(|| TEST_API_KEY.to_owned());

    let events: Arc<Mutex<Vec<Value>>> = Arc::default();
    let stop = Arc::new(Notify::new());
    let retries = Arc::new(AtomicUsize::new(0));
    let built = {
        let (on_request, on_retry) = (Arc::clone(&events), Arc::clone(&events));
        let (stop, stop_after) = (Arc::clone(&stop), c.stop_after_retry_events);
        PoliPage::builder()
            .api_key(api_key.clone())
            .base_url(server.origin.clone())
            .max_retries(c.options.max_retries)
            .retry_delay(Duration::from_millis(c.options.retry_delay_ms))
            .timeout(Duration::from_millis(c.options.timeout_ms))
            .on_request(move |e| {
                let mut events = on_request.lock().unwrap_or_else(PoisonError::into_inner);
                events.push(json!({ "type": "request", "attempt": e.attempt }));
            })
            .on_retry(move |e| {
                let mut events = on_retry.lock().unwrap_or_else(PoisonError::into_inner);
                events.push(json!({
                    "type": "retry",
                    "attempt": e.attempt,
                    "delayMs": millis(e.delay),
                }));
                if Some(retries.fetch_add(1, Ordering::SeqCst) + 1) == stop_after {
                    stop.notify_one();
                }
            })
            .build()
    };
    let result = match built {
        Err(e) => Err(e),
        // A stopped call is abandoned: dropping its future cancels it. The
        // stop branch is polled first, and a retry always yields once (even
        // with no delay), so no attempt starts after the stop.
        Ok(client) => {
            let call = SdkCall {
                method: c.call.method.clone(),
                args: c
                    .call
                    .args
                    .iter()
                    .map(|a| substitute(a, &server.origin))
                    .collect::<Result<_, _>>()?,
                cancel_after_ms: None,
            };
            tokio::select! {
                biased;
                () = stop.notified() => Ok(Value::Null),
                r = invoke(&client, &call) => r?,
            }
        }
    };
    let mock = server.origin.clone();
    let recorded = server.close();
    let result = match result {
        Ok(value) => json!({ "ok": true, "value": value }),
        Err(err) => json!({ "ok": false, "error": error_value(&err) }),
    };

    let mut failures = Vec::new();
    let expected_requests = substitute(&Value::Array(want.requests.to_vec()), &mock)?;
    compare(
        "requests",
        &describe_requests(&recorded, want.requests, &api_key),
        &expected_requests,
        &mut failures,
    );
    let actual_events = events
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    compare(
        "on_request / on_retry events",
        &describe_events(&actual_events, want.events),
        &Value::Array(want.events.to_vec()),
        &mut failures,
    );
    if want.check_result {
        compare(
            "results",
            &result,
            &substitute(&c.expect.result, &mock)?,
            &mut failures,
        );
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n\n"))
    }
}

fn run_case_blocking(c: &SdkCase) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| format!("tokio runtime: {e}"))?;
    runtime.block_on(async {
        tokio::time::timeout(CASE_TIMEOUT, run_case(c))
            .await
            .unwrap_or_else(|_| Err(format!("the case did not finish in {CASE_TIMEOUT:?}")))
    })
}

// ---------------------------------------------------------------------------
// Suite
// ---------------------------------------------------------------------------

fn failing(name: &str, message: String) -> Trial {
    Trial::test(name, move || Err(Failed::from(message)))
}

fn trials() -> Vec<Trial> {
    let Some(path) = std::env::var("POLI_CONTRACT")
        .ok()
        .filter(|p| !p.is_empty())
    else {
        if std::env::var("CI").is_ok_and(|v| !v.is_empty()) {
            return vec![failing(
                "api_contract::poli_contract_is_set",
                "POLI_CONTRACT is not set: in CI the \"API contract\" step must provide \
                 contract.json (poli-page/api-contract action, output `path`)"
                    .into(),
            )];
        }
        return vec![Trial::test(
            "api_contract (skipped: set POLI_CONTRACT to a contract.json path)",
            || Ok(()),
        )
        .with_ignored_flag(true)];
    };
    let contract = match load_contract(&path) {
        Ok(c) => c,
        Err(e) => return vec![failing("api_contract::load", e)],
    };
    let skip = match load_skip_list() {
        Ok(s) => s,
        Err(e) => return vec![failing("api_contract::skip_list", e)],
    };
    let runs_cancellation = contract
        .policy
        .cancellation_sdks
        .iter()
        .any(|s| s == SDK_NAME);

    let mut trials = Vec::new();
    let mut runnable: Vec<String> = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    let mut unknown_kinds = Vec::new();
    for raw in &contract.cases {
        let kind = raw["kind"].as_str().unwrap_or("<none>");
        let id = raw["id"].as_str().unwrap_or("<no id>").to_owned();
        match kind {
            "sdk" => {}
            "impl" => continue,
            other => {
                unknown_kinds.push(format!("{id}: {other}"));
                continue;
            }
        }
        ids.push(id.clone());
        let case: SdkCase = match serde_json::from_value(raw.clone()) {
            Ok(c) => c,
            Err(e) => {
                trials.push(failing(&id, format!("cannot read the case: {e}")));
                continue;
            }
        };
        // Any placeholder anywhere in the case, answers never served
        // included, must be one the contract defines.
        if let Err(e) = substitute(raw, "http://127.0.0.1:1") {
            trials.push(failing(&id, e));
            continue;
        }
        if case.requires.as_deref() == Some("cancellation") && !runs_cancellation {
            trials.push(
                Trial::test(&id, || Ok(()))
                    .with_kind("needs cancellation, rust is not in policy.cancellationSdks")
                    .with_ignored_flag(true),
            );
            continue;
        }
        runnable.push(id.clone());
        match skip.get(&id).cloned() {
            None => trials.push(Trial::test(&id, move || {
                run_case_blocking(&case).map_err(Failed::from)
            })),
            Some(reason) => {
                let id2 = id.clone();
                trials.push(
                    Trial::test(&id, move || match run_case_blocking(&case) {
                        Ok(()) => Err(Failed::from(format!(
                            "{id2} passes now: remove it from tests/contract-skip.json"
                        ))),
                        Err(_) => Ok(()),
                    })
                    .with_kind(format!("expected to fail: {reason}")),
                );
            }
        }
    }

    let sdk_count = ids.len();
    trials.push(Trial::test(
        "api_contract::has_sdk_cases_and_known_kinds",
        move || {
            if sdk_count == 0 {
                return Err("the contract has no kind \"sdk\" case".into());
            }
            if !unknown_kinds.is_empty() {
                return Err(format!(
                    "case kinds other than \"sdk\" and \"impl\": {unknown_kinds:?}"
                )
                .into());
            }
            Ok(())
        },
    ));
    let allowed: Vec<String> = contract
        .policy
        .normalizations
        .into_iter()
        .map(|n| n.id)
        .collect();
    trials.push(Trial::test(
        "api_contract::normalizations_are_allowed",
        move || {
            let disallowed: Vec<&str> = NORMALIZATIONS_USED
                .iter()
                .copied()
                .filter(|id| !allowed.iter().any(|a| a == id))
                .collect();
            if disallowed.is_empty() {
                Ok(())
            } else {
                Err(format!("normalizations the contract no longer allows: {disallowed:?}").into())
            }
        },
    ));
    trials.push(Trial::test("api_contract::skip_list_ids_run_here", move || {
        let stale: Vec<&String> = skip.keys().filter(|id| !ids.contains(id)).collect();
        let idle: Vec<&String> = skip
            .keys()
            .filter(|id| ids.contains(id) && !runnable.contains(id))
            .collect();
        let mut problems = Vec::new();
        if !stale.is_empty() {
            problems.push(format!(
                "ids the contract does not have, remove them from tests/contract-skip.json: {stale:?}"
            ));
        }
        if !idle.is_empty() {
            problems.push(format!(
                "cases that do not run in this SDK, remove them from tests/contract-skip.json: {idle:?}"
            ));
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems.join("\n").into())
        }
    }));
    trials
}

fn main() {
    let args = Arguments::from_args();
    libtest_mimic::run(&args, trials()).exit();
}
