//! Transport behaviour the contract cases cannot pin down: no hidden resend
//! on a reused connection, the per-attempt timeout as an upper bound, and
//! answers whose body breaks while it is read.
//!
//! Each test runs a small scripted HTTP/1.1 server on `127.0.0.1:0` that
//! answers the n-th request with the n-th action and records which TCP
//! connection every request came on.

use std::error::Error as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use poli_page::{Error, PoliPage, RenderInput};
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Far above every timeout used here: a call still running then was not
/// bounded by the SDK.
const UPPER_BOUND: Duration = Duration::from_secs(3);

#[derive(Clone)]
enum Act {
    /// A complete answer, the connection kept alive.
    Answer {
        status: u16,
        headers: Vec<(&'static str, String)>,
        body: Vec<u8>,
    },
    /// Close the connection once the request is read, with no answer.
    Drop,
    /// Never answer.
    Hang,
    /// The head with `content-length: declared`, the first bytes of the
    /// body, then a close (`drip: false`) or one more byte every 50 ms,
    /// never reaching `declared` (`drip: true`).
    Partial {
        status: u16,
        headers: Vec<(&'static str, String)>,
        declared: usize,
        sent: Vec<u8>,
        drip: bool,
    },
}

#[derive(Debug, Clone)]
struct Seen {
    connection: usize,
    method: String,
    path: String,
}

struct Server {
    origin: String,
    seen: Arc<Mutex<Vec<Seen>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Server {
    async fn start(acts: Vec<Act>) -> Self {
        Self::start_with(|_| acts).await
    }

    /// `acts` gets the server's origin, for answers that point back at it.
    async fn start_with(acts: impl FnOnce(&str) -> Vec<Act>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let origin = format!("http://{}", listener.local_addr().expect("addr"));
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
        let acts = Arc::new(acts(&origin));
        let task = {
            let seen = Arc::clone(&seen);
            tokio::spawn(async move {
                // Dropping the set (when this task is aborted) closes every
                // connection, hung ones included.
                let mut connections = tokio::task::JoinSet::new();
                let mut next_id = 0;
                while let Ok((stream, _)) = listener.accept().await {
                    next_id += 1;
                    connections.spawn(serve(stream, next_id, Arc::clone(&acts), Arc::clone(&seen)));
                }
            })
        };
        Self { origin, seen, task }
    }

    fn seen(&self) -> Vec<Seen> {
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve(
    mut stream: TcpStream,
    connection: usize,
    acts: Arc<Vec<Act>>,
    seen: Arc<Mutex<Vec<Seen>>>,
) {
    let mut buf = Vec::new();
    loop {
        let Some((method, path)) = read_request(&mut stream, &mut buf).await else {
            return;
        };
        let index = {
            let mut seen = seen.lock().unwrap_or_else(PoisonError::into_inner);
            seen.push(Seen {
                connection,
                method,
                path,
            });
            seen.len() - 1
        };
        let act = acts.get(index).cloned().unwrap_or(Act::Answer {
            status: 599,
            headers: vec![],
            body: b"{\"error\":\"MOCK_EXHAUSTED\"}".to_vec(),
        });
        match act {
            Act::Answer {
                status,
                headers,
                body,
            } => {
                let head = head(status, &headers, body.len());
                if stream.write_all(head.as_bytes()).await.is_err()
                    || stream.write_all(&body).await.is_err()
                {
                    return;
                }
            }
            Act::Drop => return,
            Act::Hang => std::future::pending().await,
            Act::Partial {
                status,
                headers,
                declared,
                sent,
                drip,
            } => {
                let head = head(status, &headers, declared);
                if stream.write_all(head.as_bytes()).await.is_err()
                    || stream.write_all(&sent).await.is_err()
                {
                    return;
                }
                if !drip {
                    return;
                }
                for _ in sent.len()..declared - 1 {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    if stream.write_all(b" ").await.is_err() {
                        return;
                    }
                }
                std::future::pending::<()>().await;
            }
        }
    }
}

fn head(status: u16, headers: &[(&str, String)], length: usize) -> String {
    let mut out = format!("HTTP/1.1 {status} X\r\n");
    for (name, value) in headers {
        out.push_str(&format!("{name}: {value}\r\n"));
    }
    out.push_str(&format!("content-length: {length}\r\n\r\n"));
    out
}

async fn read_request(stream: &mut TcpStream, buf: &mut Vec<u8>) -> Option<(String, String)> {
    let end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        let mut chunk = [0u8; 4096];
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..end]).into_owned();
    buf.drain(..end + 4);
    let mut lines = head.split("\r\n");
    let mut first = lines.next()?.split(' ');
    let (method, path) = (first.next()?.to_owned(), first.next()?.to_owned());
    let length = lines
        .filter_map(|l| l.split_once(':'))
        .find(|(n, _)| n.trim().eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse::<usize>().ok())
        .unwrap_or(0);
    while buf.len() < length {
        let mut chunk = [0u8; 4096];
        let n = stream.read(&mut chunk).await.ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    buf.drain(..length);
    Some((method, path))
}

fn descriptor(pdf_url: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "documentId": "doc_abc123",
        "organizationId": "org_xyz",
        "projectId": null,
        "projectSlug": "billing",
        "templateId": null,
        "templateSlug": "invoice",
        "environment": "sandbox",
        "apiKeyId": null,
        "format": "A4",
        "orientation": "portrait",
        "locale": null,
        "pageCount": 1,
        "sizeBytes": 10,
        "createdAt": "2026-09-29T09:45:22.000Z",
        "metadata": {},
        "presignedPdfUrl": pdf_url,
        "expiresAt": "2026-09-29T10:00:22.000Z",
    }))
    .expect("descriptor json")
}

fn ok_descriptor(pdf_url: &str) -> Act {
    Act::Answer {
        status: 200,
        headers: vec![("content-type", "application/json".into())],
        body: descriptor(pdf_url),
    }
}

fn input() -> RenderInput {
    RenderInput {
        project: "billing".into(),
        template: "invoice".into(),
        data: json!({ "n": 1 }),
        ..Default::default()
    }
}

/// A client at `origin` that counts its `on_request` events.
fn client(origin: &str, max_retries: u32, timeout: Duration) -> (PoliPage, Arc<AtomicUsize>) {
    client_with(origin, max_retries, timeout, None)
}

fn client_with(
    origin: &str,
    max_retries: u32,
    timeout: Duration,
    http: Option<reqwest::Client>,
) -> (PoliPage, Arc<AtomicUsize>) {
    let attempts = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&attempts);
    let mut builder = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(origin)
        .max_retries(max_retries)
        .retry_delay(Duration::from_millis(1))
        .timeout(timeout)
        .on_request(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        });
    if let Some(http) = http {
        builder = builder.http_client(http);
    }
    (builder.build().expect("client"), attempts)
}

async fn bounded<T>(what: &str, call: impl std::future::Future<Output = T>) -> (T, Duration) {
    let start = Instant::now();
    let out = tokio::time::timeout(UPPER_BOUND, call)
        .await
        .unwrap_or_else(|_| panic!("{what}: still running after {UPPER_BOUND:?}"));
    (out, start.elapsed())
}

// ---------------------------------------------------------------------------
// No hidden resend
// ---------------------------------------------------------------------------

/// A request lost on a reused keep-alive connection is never sent again
/// behind the SDK's back: the server sees exactly the attempts `on_request`
/// reports, and with `max_retries(0)` the caller gets `network_error`.
async fn lost_on_a_reused_connection(post: bool) {
    let server = Server::start(vec![
        ok_descriptor("http://unused.invalid/x.pdf"),
        // The second request arrives on the same connection, is read in
        // full, and the connection closes without an answer.
        Act::Drop,
    ])
    .await;
    let (client, attempts) = client(&server.origin, 0, Duration::from_secs(2));
    let call = || async {
        if post {
            client.render.document(input()).await.map(|_| ())
        } else {
            client.documents.get("doc_abc123").await.map(|_| ())
        }
    };
    call().await.expect("first call");
    let err = call().await.expect_err("second call");

    let seen = server.seen();
    assert_eq!(seen.len(), 2, "requests the server saw: {seen:?}");
    // Control: the lost request really went out on the reused connection.
    assert_eq!(seen[0].connection, seen[1].connection, "{seen:?}");
    assert_eq!(seen[1].method, if post { "POST" } else { "GET" });
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(err.code(), "network_error", "{err:?}");
    assert_eq!(err.message(), "Network request failed");
    assert!(err.source().is_some(), "the transport error is the source");
}

#[tokio::test]
async fn post_lost_on_a_reused_connection_is_not_resent() {
    lost_on_a_reused_connection(true).await;
}

#[tokio::test]
async fn get_lost_on_a_reused_connection_is_not_resent() {
    lost_on_a_reused_connection(false).await;
}

#[tokio::test]
async fn post_lost_on_a_reused_connection_is_resent_only_as_a_counted_retry() {
    let server = Server::start(vec![
        ok_descriptor("http://unused.invalid/x.pdf"),
        Act::Drop,
        ok_descriptor("http://unused.invalid/x.pdf"),
    ])
    .await;
    let (client, attempts) = client(&server.origin, 1, Duration::from_secs(2));
    client.render.document(input()).await.expect("first call");
    client.render.document(input()).await.expect("retried call");

    let seen = server.seen();
    assert_eq!(seen.len(), 3, "{seen:?}");
    assert_eq!(seen[0].connection, seen[1].connection, "{seen:?}");
    assert_ne!(seen[1].connection, seen[2].connection, "{seen:?}");
    // 1 for the first call, 2 for the second: every request is an attempt.
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

// ---------------------------------------------------------------------------
// The per-attempt timeout is an upper bound
// ---------------------------------------------------------------------------

#[tokio::test]
async fn timeout_bounds_an_attempt_that_never_answers() {
    let server = Server::start(vec![Act::Hang]).await;
    let (client, _) = client(&server.origin, 0, Duration::from_millis(200));
    let (result, elapsed) = bounded("hang", client.documents.get("doc_abc123")).await;
    let err = result.expect_err("timeout");
    assert_eq!(err.code(), "timeout", "{err:?}");
    assert_eq!(err.message(), "Request timed out after 200ms");
    assert!(elapsed >= Duration::from_millis(200), "{elapsed:?}");
}

#[tokio::test]
async fn timeout_bounds_an_attempt_with_an_injected_client() {
    let server = Server::start(vec![Act::Hang]).await;
    let http = reqwest::Client::builder().build().expect("reqwest client");
    let (client, _) = client_with(&server.origin, 0, Duration::from_millis(200), Some(http));
    let (result, _) = bounded("hang", client.documents.get("doc_abc123")).await;
    let err = result.expect_err("timeout");
    assert_eq!(err.message(), "Request timed out after 200ms");
}

#[tokio::test]
async fn timeout_bounds_a_success_body_that_drips() {
    let server = Server::start(vec![Act::Partial {
        status: 200,
        headers: vec![("content-type", "application/json".into())],
        declared: 10_000,
        sent: b"{".to_vec(),
        drip: true,
    }])
    .await;
    let (client, _) = client(&server.origin, 0, Duration::from_millis(200));
    let (result, _) = bounded("drip", client.documents.get("doc_abc123")).await;
    let err = result.expect_err("timeout");
    assert_eq!(err.code(), "timeout", "{err:?}");
    assert_eq!(err.message(), "Request timed out after 200ms");
    assert!(err.is_retryable());
}

// ---------------------------------------------------------------------------
// Answers whose body breaks while it is read
// ---------------------------------------------------------------------------

#[tokio::test]
async fn success_body_cut_off_is_a_retried_network_error() {
    let cut = Act::Partial {
        status: 200,
        headers: vec![("content-type", "application/json".into())],
        declared: 1000,
        sent: b"{\"documentId\":".to_vec(),
        drip: false,
    };
    let server = Server::start(vec![cut.clone(), cut]).await;
    let responses = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&responses);
    let client = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(&server.origin)
        .max_retries(1)
        .retry_delay(Duration::from_millis(1))
        .on_response(move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .expect("client");
    let err = client
        .render
        .document(input())
        .await
        .expect_err("network error");
    assert_eq!(err.code(), "network_error", "{err:?}");
    assert_eq!(err.message(), "Network request failed");
    assert_eq!(server.seen().len(), 2, "retried once");
    // on_response fires only once a body is read in full.
    assert_eq!(responses.load(Ordering::SeqCst), 0);
}

fn partial_error(status: u16, drip: bool) -> Act {
    Act::Partial {
        status,
        headers: vec![
            ("content-type", "application/json".into()),
            ("x-request-id", "req_partial".into()),
        ],
        declared: 1000,
        // Bytes that start like a parseable error body: never read as one.
        sent: b"{\"error\":\"VALIDATION_ERROR\"}".to_vec(),
        drip,
    }
}

async fn assert_malformed_error_body(status: u16, drip: bool, retried: bool) {
    let act = partial_error(status, drip);
    let server = Server::start(vec![act.clone(), act.clone(), act]).await;
    let (client, attempts) = client(&server.origin, 2, Duration::from_millis(200));
    let (result, _) = bounded("error body", client.documents.get("doc_abc123")).await;
    let err = result.expect_err("error");
    assert_eq!(err.code(), "INTERNAL_ERROR", "{err:?}");
    assert_eq!(err.message(), format!("HTTP {status}"));
    assert_eq!(err.status(), Some(status));
    assert_eq!(err.request_id(), Some("req_partial"));
    assert!(err.source().is_some(), "the read error is the source");
    assert_eq!(err.is_retryable(), retried);
    let expected = if retried { 3 } else { 1 };
    assert_eq!(server.seen().len(), expected);
    assert_eq!(attempts.load(Ordering::SeqCst), expected);
}

#[tokio::test]
async fn error_body_cut_off_is_internal_error_and_a_400_is_not_retried() {
    assert_malformed_error_body(400, false, false).await;
}

#[tokio::test]
async fn error_body_cut_off_is_internal_error_and_a_503_is_retried() {
    assert_malformed_error_body(503, false, true).await;
}

#[tokio::test]
async fn error_body_past_the_timeout_is_internal_error_retried_by_status() {
    assert_malformed_error_body(503, true, true).await;
}

// ---------------------------------------------------------------------------
// The presigned-URL download
// ---------------------------------------------------------------------------

fn storage_url(origin: &str) -> String {
    format!("{origin}/storage/doc_abc123.pdf")
}

#[tokio::test]
async fn download_failed_with_a_storage_503_is_not_retried() {
    // DOWNLOAD_FAILED is never retried, whatever the storage status.
    let server = Server::start_with(|origin| {
        vec![
            ok_descriptor(&storage_url(origin)),
            Act::Answer {
                status: 503,
                headers: vec![("content-type", "application/xml".into())],
                body: b"<Error><Code>SlowDown</Code></Error>".to_vec(),
            },
        ]
    })
    .await;
    let (client, attempts) = client(&server.origin, 2, Duration::from_secs(2));
    let err = client
        .render
        .pdf(input())
        .await
        .expect_err("download failed");
    assert_eq!(err.code(), "DOWNLOAD_FAILED", "{err:?}");
    assert_eq!(err.message(), "Failed to download PDF: HTTP 503");
    assert_eq!(err.status(), Some(503));
    assert!(!err.is_retryable());
    let seen = server.seen();
    assert_eq!(seen.len(), 2, "one render, one download: {seen:?}");
    assert_eq!(seen[1].path, "/storage/doc_abc123.pdf");
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

fn cut_pdf() -> Act {
    Act::Partial {
        status: 200,
        headers: vec![("content-type", "application/pdf".into())],
        declared: 1000,
        sent: b"%PDF-1.4".to_vec(),
        drip: false,
    }
}

#[tokio::test]
async fn download_body_cut_off_is_download_failed_network_error() {
    let server =
        Server::start_with(|origin| vec![ok_descriptor(&storage_url(origin)), cut_pdf()]).await;
    let (client, _) = client(&server.origin, 2, Duration::from_secs(2));
    let err = client
        .render
        .pdf(input())
        .await
        .expect_err("download failed");
    assert_eq!(err.code(), "DOWNLOAD_FAILED", "{err:?}");
    assert_eq!(err.message(), "Failed to download PDF: network error");
    assert_eq!(err.status(), None);
    assert!(err.source().is_some(), "the read error is the source");
    assert!(!err.is_retryable());
    assert_eq!(server.seen().len(), 2);
}

#[tokio::test]
async fn pdf_stream_body_cut_off_is_download_failed_network_error() {
    let server =
        Server::start_with(|origin| vec![ok_descriptor(&storage_url(origin)), cut_pdf()]).await;
    let (client, _) = client(&server.origin, 2, Duration::from_secs(2));
    let mut stream = client.render.pdf_stream(input()).await.expect("stream");
    let mut failure: Option<Error> = None;
    while let Some(chunk) = stream.next().await {
        if let Err(e) = chunk {
            failure = Some(e);
            break;
        }
    }
    let err = failure.expect("a chunk error");
    assert_eq!(err.code(), "DOWNLOAD_FAILED", "{err:?}");
    assert_eq!(err.message(), "Failed to download PDF: network error");
    assert_eq!(err.status(), None);
    assert!(err.source().is_some());
}
