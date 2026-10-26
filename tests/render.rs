//! End-to-end wiremock coverage for the `render` namespace.
//!
//! Ports the renderPdf / renderDocument / renderPdfStream describe blocks
//! from `sdk-node/tests/render.test.ts`. Uses `wiremock` (Rust's
//! async-native HTTP mock) so the tests exercise the real `reqwest`
//! transport, headers, body serialization, response parsing, and the
//! two-hop second fetch against the "presigned" URL — only the network is
//! faked. Transport, header, hook and retry tests run against
//! `POST /v1/render` through `render.document`.

use std::time::Duration;

use futures_util::StreamExt;
use poli_page::{DocumentDescriptor, Orientation, PageFormat, PoliPage, RenderInput};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// Presigned URL for tests that never perform the second hop.
const UNUSED_PDF_URL: &str = "https://nowhere.example/x.pdf";

// `async` kept so every call site reads `build_client(...).await` symmetrically
// with the namespace methods being tested — no real work happens inside.
#[allow(clippy::unused_async)]
async fn build_client(uri: String) -> PoliPage {
    PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(uri)
        // Keep retries fast so error-path tests don't dawdle.
        .max_retries(0)
        .retry_delay(Duration::from_millis(1))
        .timeout(Duration::from_secs(2))
        .build()
        .expect("builder")
}

#[tokio::test]
async fn render_posts_to_v1_render_with_auth_and_json_headers() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .and(header("authorization", "Bearer pp_sandbox_x"))
        .and(header("content-type", "application/json"))
        .and(header("accept", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(UNUSED_PDF_URL)))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let result = client
        .render
        .document(RenderInput {
            project: "billing".into(),
            template: "invoice".into(),
            data: json!({ "x": 1 }),
            ..Default::default()
        })
        .await
        .expect("document");

    assert_eq!(result.document_id, "doc_abc123");
    assert_eq!(result.environment, "live");
}

#[tokio::test]
async fn render_body_holds_only_the_seven_documented_fields() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(UNUSED_PDF_URL)))
        .mount(&server)
        .await;

    let mut metadata = poli_page::RenderMetadata::new();
    metadata.insert("customerId".into(), "cust_1".into());
    let client = build_client(server.uri()).await;
    client
        .render
        .document(RenderInput {
            project: "billing".into(),
            template: "invoice".into(),
            data: json!({ "amount": 1280 }),
            metadata: Some(metadata),
            format: Some(PageFormat::A4),
            orientation: Some(Orientation::Landscape),
            locale: Some("fr-FR".into()),
            idempotency_key: Some("idem-1".into()),
            timeout: Some(Duration::from_secs(5)),
        })
        .await
        .expect("document");

    let req = &server.received_requests().await.expect("requests")[0];
    let body: serde_json::Value = req.body_json().expect("json body");
    assert_eq!(body["project"], "billing");
    assert_eq!(body["template"], "invoice");
    assert_eq!(body["data"]["amount"], 1280);
    assert_eq!(body["format"], "A4");
    assert_eq!(body["orientation"], "landscape");
    assert_eq!(body["locale"], "fr-FR");
    assert_eq!(body["metadata"]["customerId"], "cust_1");
    let mut keys: Vec<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "data",
            "format",
            "locale",
            "metadata",
            "orientation",
            "project",
            "template"
        ],
    );
}

#[tokio::test]
async fn render_body_omits_unset_optional_fields() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(UNUSED_PDF_URL)))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    client
        .render
        .document(project_input())
        .await
        .expect("document");

    let req = &server.received_requests().await.expect("requests")[0];
    let body: serde_json::Value = req.body_json().expect("json body");
    let mut keys: Vec<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["data", "project", "template"]);
}

#[tokio::test]
async fn render_forwards_metadata_in_request_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(UNUSED_PDF_URL)))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let mut metadata = poli_page::RenderMetadata::new();
    metadata.insert("customerId".into(), "cust_1".into());
    metadata.insert("amount".into(), 1280i64.into());
    client
        .render
        .document(RenderInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            metadata: Some(metadata),
            ..Default::default()
        })
        .await
        .expect("document");

    let req = &server.received_requests().await.expect("requests")[0];
    let body: serde_json::Value = req.body_json().expect("json body");
    assert_eq!(body["metadata"]["customerId"], "cust_1");
    assert_eq!(body["metadata"]["amount"], 1280);
}

#[tokio::test]
async fn render_sends_auto_generated_idempotency_key_for_post() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(UNUSED_PDF_URL)))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    client
        .render
        .document(RenderInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect("document");

    let req = &server.received_requests().await.expect("requests")[0];
    let idem = req
        .headers
        .get("idempotency-key")
        .expect("idempotency-key header");
    // UUID v4 is 36 chars.
    assert_eq!(idem.to_str().unwrap().len(), 36);
}

#[tokio::test]
async fn render_honors_caller_supplied_idempotency_key() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(UNUSED_PDF_URL)))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    client
        .render
        .document(RenderInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            idempotency_key: Some("idem-caller-supplied".into()),
            ..Default::default()
        })
        .await
        .expect("document");

    let req = &server.received_requests().await.expect("requests")[0];
    let idem = req.headers.get("idempotency-key").unwrap();
    assert_eq!(idem.to_str().unwrap(), "idem-caller-supplied");
}

#[tokio::test]
async fn render_surfaces_validation_error_from_400_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(
            ResponseTemplate::new(400).set_body_json(
                json!({ "code": "VALIDATION_ERROR", "message": "data is required" }),
            ),
        )
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let err = client
        .render
        .document(RenderInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect_err("expected validation error");

    assert!(err.is_validation_error(), "wrong variant: {err:?}");
    assert_eq!(err.code(), "VALIDATION_ERROR");
    assert_eq!(err.status(), Some(400));
}

#[tokio::test]
async fn render_retries_on_503_then_succeeds() {
    let server = MockServer::start().await;
    // First call: 503; second call: 200. Mock.up_to_n_times scopes the response.
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(503).set_body_string("transient"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(UNUSED_PDF_URL)))
        .mount(&server)
        .await;

    // Use a client that DOES retry — overriding the helper's max_retries=0.
    let client = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .max_retries(1)
        .retry_delay(Duration::from_millis(1))
        .timeout(Duration::from_secs(2))
        .build()
        .expect("builder");

    let result = client
        .render
        .document(RenderInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect("retry should have produced success");
    assert_eq!(result.document_id, "doc_abc123");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn render_does_not_retry_401_auth_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({ "code": "INVALID_API_KEY", "message": "nope" })),
        )
        .mount(&server)
        .await;

    let client = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .max_retries(3) // would retry if 401 were retryable
        .retry_delay(Duration::from_millis(1))
        .timeout(Duration::from_secs(2))
        .build()
        .expect("builder");

    let err = client
        .render
        .document(RenderInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect_err("auth error");
    assert!(err.is_auth_error());
    // Exactly one request — 401 is not retryable.
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn render_honors_retry_after_header_on_429() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "0")
                .set_body_json(json!({ "code": "RATE_LIMITED", "message": "slow down" })),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(UNUSED_PDF_URL)))
        .mount(&server)
        .await;

    let retry_count = Arc::new(AtomicU32::new(0));
    let rc = Arc::clone(&retry_count);
    let client = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .max_retries(1)
        .retry_delay(Duration::from_millis(500)) // would dominate wall-time if Retry-After ignored
        .timeout(Duration::from_secs(2))
        .on_retry(move |_evt| {
            rc.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .expect("builder");

    let start = std::time::Instant::now();
    client
        .render
        .document(RenderInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect("retry should have succeeded");
    let elapsed = start.elapsed();
    assert_eq!(
        retry_count.load(Ordering::SeqCst),
        1,
        "on_retry should fire once"
    );
    // Retry-After: 0 means almost no sleep — should finish much faster than
    // retry_delay (500ms) would suggest. 200ms gives slack for jitter on
    // CI hosts running the suite alongside other workloads.
    assert!(elapsed < Duration::from_millis(200), "took {elapsed:?}");
}

#[tokio::test]
async fn render_fires_on_error_when_retries_exhausted() {
    use std::sync::Arc;
    use std::sync::Mutex;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let last_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let captured = Arc::clone(&last_error);
    let client = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .max_retries(2)
        .retry_delay(Duration::from_millis(1))
        .timeout(Duration::from_secs(2))
        .on_error(move |err| {
            *captured.lock().unwrap() = Some(err.code().to_string());
        })
        .build()
        .expect("builder");

    let _ = client
        .render
        .document(RenderInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect_err("exhausted retries");
    // 3 attempts (initial + 2 retries) all 503.
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
    assert_eq!(
        last_error.lock().unwrap().as_deref(),
        Some("INTERNAL_ERROR"), // 503 with no parseable body → fallback code
    );
}

/// Helper: assert custom hooks never panic the call site even when they themselves panic.
#[tokio::test]
async fn render_hook_panic_does_not_break_the_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(UNUSED_PDF_URL)))
        .mount(&server)
        .await;

    let client = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .max_retries(0)
        .retry_delay(Duration::from_millis(1))
        .timeout(Duration::from_secs(2))
        .on_error(|_| panic!("hook went boom"))
        .build()
        .expect("builder");

    // Hook isn't fired on success — but compile-time we want this to typecheck.
    let _ = client
        .render
        .document(RenderInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect("ok");

    // Now make on_error fire by triggering a non-retryable error.
    let server2 = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server2)
        .await;
    let client2 = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server2.uri())
        .max_retries(0)
        .timeout(Duration::from_secs(2))
        .on_error(|_| panic!("hook went boom on error path"))
        .build()
        .expect("builder");

    let err = client2
        .render
        .document(RenderInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect_err("hook panic must not change error path");
    assert!(err.is_auth_error());
}

// Touch the `Request` import so it's clearly part of the test crate's API
// surface — wiremock exposes `Request` via `Mock::respond_with` closures in
// some patterns we may add later.
#[allow(dead_code)]
fn _ensure_request_import_used(_: &Request) {}

// =============================================================================
// Phase 3 — render.pdf / render.pdf_stream / render.document
// =============================================================================

/// Canonical descriptor wire shape, mirroring the Node sample at
/// `tests/render.test.ts:99-117`. Callers override `presignedPdfUrl` to
/// point at the local mock server.
fn descriptor_wire(presigned_url: &str) -> serde_json::Value {
    json!({
        "documentId": "doc_abc123",
        "organizationId": "org_xyz",
        "projectId": "proj_42",
        "projectSlug": "billing",
        "templateId": "tpl_invoice_v1",
        "templateSlug": "invoice",
        "environment": "live",
        "apiKeyId": "key_live_abc",
        "format": "A4",
        "orientation": "portrait",
        "locale": "en-US",
        "pageCount": 2,
        "sizeBytes": 38421,
        "createdAt": "2026-04-30T19:45:22Z",
        "metadata": {},
        "presignedPdfUrl": presigned_url,
        "expiresAt": "2026-04-30T20:00:22Z"
    })
}

const PDF_STUB: &[u8] = b"%PDF-1.4 stub";

#[tokio::test]
async fn render_document_posts_to_v1_render_and_returns_parsed_descriptor() {
    let server = MockServer::start().await;
    let presigned = format!("{}/presigned/x.pdf", server.uri());
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let doc = client
        .render
        .document(RenderInput {
            project: "billing".into(),
            template: "invoice".into(),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect("document");

    assert_eq!(doc.document_id, "doc_abc123");
    assert_eq!(doc.template_slug.as_deref(), Some("invoice"));
    assert_eq!(doc.page_count, 2);
    assert_eq!(doc.format, PageFormat::A4);
    assert_eq!(doc.environment, "live");
    assert_eq!(doc.presigned_pdf_url, presigned);

    // The request landed on /v1/render with the project body.
    let req = &server.received_requests().await.unwrap()[0];
    assert_eq!(req.url.path(), "/v1/render");
    let body: serde_json::Value = req.body_json().expect("json body");
    assert_eq!(body["project"], "billing");
    assert_eq!(body["template"], "invoice");
}

#[tokio::test]
async fn render_document_attached_client_back_ref_lets_download_pdf_succeed() {
    let server = MockServer::start().await;
    let presigned = format!("{}/presigned/x.pdf", server.uri());
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/presigned/x.pdf"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(PDF_STUB))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let doc = client
        .render
        .document(project_input())
        .await
        .expect("document");
    let bytes = doc.download_pdf().await.expect("download_pdf");
    assert_eq!(&bytes[..4], b"%PDF");
}

#[tokio::test]
async fn render_document_returns_empty_metadata_when_server_returns_empty() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(descriptor_wire("https://nowhere.example/x.pdf")),
        )
        .mount(&server)
        .await;
    let client = build_client(server.uri()).await;
    let doc = client
        .render
        .document(project_input())
        .await
        .expect("document");
    assert!(doc.metadata.is_empty());
}

#[tokio::test]
async fn render_document_download_pdf_surfaces_download_failed_on_403() {
    let server = MockServer::start().await;
    let presigned = format!("{}/presigned/expired.pdf", server.uri());
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/presigned/expired.pdf"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let doc = client
        .render
        .document(project_input())
        .await
        .expect("document");
    let err = doc.download_pdf().await.expect_err("download must fail");
    assert!(matches!(err, poli_page::Error::Download { .. }));
    assert_eq!(err.code(), "DOWNLOAD_FAILED");
    assert_eq!(err.status(), Some(403));
}

#[tokio::test]
async fn render_pdf_two_hop_returns_bytes() {
    let server = MockServer::start().await;
    let presigned = format!("{}/presigned/x.pdf", server.uri());
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/presigned/x.pdf"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(PDF_STUB))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let bytes = client
        .render
        .pdf(project_input())
        .await
        .expect("pdf two-hop");
    assert_eq!(&bytes[..4], b"%PDF");

    // Exactly two requests: POST /v1/render, GET /presigned/x.pdf.
    let reqs = server.received_requests().await.unwrap();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[0].url.path(), "/v1/render");
    assert_eq!(reqs[0].method.as_str(), "POST");
    assert_eq!(reqs[1].url.path(), "/presigned/x.pdf");
    assert_eq!(reqs[1].method.as_str(), "GET");
}

#[tokio::test]
async fn render_pdf_strips_idempotency_and_timeout_from_render_body() {
    let server = MockServer::start().await;
    let presigned = format!("{}/presigned/x.pdf", server.uri());
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/presigned/x.pdf"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(PDF_STUB))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    client
        .render
        .pdf(RenderInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            idempotency_key: Some("idem-caller-supplied".into()),
            timeout: Some(Duration::from_secs(5)),
            ..Default::default()
        })
        .await
        .expect("pdf");

    let reqs = server.received_requests().await.unwrap();
    let render_req = reqs
        .iter()
        .find(|r| r.url.path() == "/v1/render")
        .expect("render request");
    let body: serde_json::Value = render_req.body_json().expect("json body");
    let obj = body.as_object().unwrap();
    assert!(!obj.contains_key("idempotencyKey"));
    assert!(!obj.contains_key("timeout"));
    // Caller's idempotency key still made it into the header.
    let idem = render_req.headers.get("idempotency-key").unwrap();
    assert_eq!(idem.to_str().unwrap(), "idem-caller-supplied");
}

#[tokio::test]
async fn render_pdf_surfaces_download_failed_on_presigned_403() {
    let server = MockServer::start().await;
    let presigned = format!("{}/presigned/expired.pdf", server.uri());
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/presigned/expired.pdf"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let err = client
        .render
        .pdf(project_input())
        .await
        .expect_err("download must fail");
    assert!(matches!(err, poli_page::Error::Download { .. }));
    assert_eq!(err.code(), "DOWNLOAD_FAILED");
    assert_eq!(err.status(), Some(403));
}

#[tokio::test]
async fn render_pdf_does_not_retry_second_hop_failure() {
    // The first hop's retry policy must NOT extend to the presigned URL —
    // per spec §5.5 the S3 fetch is single-attempt regardless of
    // max_retries.
    let server = MockServer::start().await;
    let presigned = format!("{}/presigned/expired.pdf", server.uri());
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/presigned/expired.pdf"))
        .respond_with(ResponseTemplate::new(503)) // would normally be retryable
        .mount(&server)
        .await;

    let client = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .max_retries(5) // would retry ~6x for a 5xx if the policy applied
        .retry_delay(Duration::from_millis(1))
        .timeout(Duration::from_secs(2))
        .build()
        .expect("builder");

    let _ = client
        .render
        .pdf(project_input())
        .await
        .expect_err("expected Download error");
    // Exactly 2 requests: 1 POST /v1/render + 1 GET /presigned/expired.pdf.
    // (Not 1 + 6.)
    let reqs = server.received_requests().await.unwrap();
    let gets: Vec<_> = reqs
        .iter()
        .filter(|r| r.url.path() == "/presigned/expired.pdf")
        .collect();
    assert_eq!(gets.len(), 1, "second-hop must not be retried");
}

#[tokio::test]
async fn render_pdf_stream_yields_pdf_bytes() {
    let server = MockServer::start().await;
    let presigned = format!("{}/presigned/x.pdf", server.uri());
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/presigned/x.pdf"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(PDF_STUB))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let mut stream = client
        .render
        .pdf_stream(project_input())
        .await
        .expect("pdf_stream");

    // Drain the stream into a buffer.
    let mut collected = Vec::new();
    while let Some(chunk) = stream.next().await {
        collected.extend_from_slice(&chunk.expect("chunk"));
    }
    assert_eq!(&collected[..4], b"%PDF");
}

#[tokio::test]
async fn render_pdf_stream_surfaces_download_failed_on_presigned_410() {
    let server = MockServer::start().await;
    let presigned = format!("{}/presigned/gone.pdf", server.uri());
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/presigned/gone.pdf"))
        .respond_with(ResponseTemplate::new(410))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    // The header-arrival error surfaces synchronously from pdf_stream itself,
    // not as a stream Err item.
    let err = client
        .render
        .pdf_stream(project_input())
        .await
        .expect_err("stream must fail at header time");
    assert!(matches!(err, poli_page::Error::Download { .. }));
    assert_eq!(err.code(), "DOWNLOAD_FAILED");
    assert_eq!(err.status(), Some(410));
}

/// Proves `PoliPageBuilder::http_client` is actually used for requests:
/// the injected client carries a default header the SDK never sets; the
/// mock only matches when that header is present on the wire.
#[tokio::test]
async fn render_uses_injected_http_client() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .and(header("x-sdk-test-marker", "injected-client"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(UNUSED_PDF_URL)))
        .mount(&server)
        .await;

    let mut default_headers = reqwest::header::HeaderMap::new();
    default_headers.insert(
        "x-sdk-test-marker",
        reqwest::header::HeaderValue::from_static("injected-client"),
    );
    let injected = reqwest::Client::builder()
        .default_headers(default_headers)
        .build()
        .expect("custom client builds");

    let client = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .max_retries(0)
        .http_client(injected)
        .build()
        .expect("builder");

    client
        .render
        .document(project_input())
        .await
        .expect("document");
}

fn project_input() -> RenderInput {
    RenderInput {
        project: "p".into(),
        template: "t".into(),
        data: json!({}),
        ..Default::default()
    }
}

// Surface the import so unused-warning doesn't bite.
#[allow(dead_code)]
fn _ensure_document_descriptor_import_used(_: &DocumentDescriptor) {}

// =============================================================================
// Hook lifecycle tests (Tasks 3-6)
// =============================================================================

#[tokio::test]
async fn on_request_setter_compiles_on_async_builder() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    let count = Arc::new(AtomicU32::new(0));
    let c = Arc::clone(&count);
    let _client = poli_page::PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url("https://api.poli.page")
        .on_request(move |_evt: &poli_page::RequestEvent| {
            c.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .expect("build");
    // We only assert the builder accepted the setter and `build()` succeeded
    // — actual firing is covered in a later test.
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn on_request_fires_once_per_attempt_with_resolved_url() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    let server = MockServer::start().await;
    // First two attempts 503, third 200 — gives us 3 total dispatches.
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(UNUSED_PDF_URL)))
        .mount(&server)
        .await;

    let events: Arc<Mutex<Vec<(String, String, u32)>>> = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    let count = Arc::new(AtomicU32::new(0));
    let c = Arc::clone(&count);
    let client = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .max_retries(2)
        .retry_delay(Duration::from_millis(1))
        .timeout(Duration::from_secs(2))
        .on_request(move |evt| {
            c.fetch_add(1, Ordering::SeqCst);
            captured
                .lock()
                .unwrap()
                .push((evt.method.clone(), evt.url.clone(), evt.attempt));
        })
        .build()
        .expect("builder");

    client
        .render
        .document(RenderInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect("eventual success");

    assert_eq!(count.load(Ordering::SeqCst), 3, "fired once per attempt");
    let evts = events.lock().unwrap();
    assert_eq!(evts[0].0, "POST");
    assert!(evts[0].1.ends_with("/v1/render"), "url was {}", evts[0].1);
    assert_eq!(evts[0].2, 1, "1-based attempt");
    assert_eq!(evts[1].2, 2);
    assert_eq!(evts[2].2, 3);
}

#[tokio::test]
async fn on_response_fires_only_on_2xx_with_status_and_request_id() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    let server = MockServer::start().await;
    // First call returns 503 (no on_response), second returns 200 with
    // x-request-id (on_response should fire exactly once).
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-request-id", "req_xyz")
                .set_body_json(descriptor_wire(UNUSED_PDF_URL)),
        )
        .mount(&server)
        .await;

    let calls = Arc::new(AtomicU32::new(0));
    let captured_status: Arc<Mutex<Option<u16>>> = Arc::new(Mutex::new(None));
    let captured_rid: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let captured_dur: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));
    let cc = Arc::clone(&calls);
    let cs = Arc::clone(&captured_status);
    let cr = Arc::clone(&captured_rid);
    let cd = Arc::clone(&captured_dur);

    let client = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .max_retries(1)
        .retry_delay(Duration::from_millis(1))
        .timeout(Duration::from_secs(2))
        .on_response(move |evt| {
            cc.fetch_add(1, Ordering::SeqCst);
            *cs.lock().unwrap() = Some(evt.status);
            *cr.lock().unwrap() = evt.request_id.clone();
            *cd.lock().unwrap() = Some(evt.duration_ms);
        })
        .build()
        .expect("builder");

    client
        .render
        .document(RenderInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect("ok");

    assert_eq!(calls.load(Ordering::SeqCst), 1, "fires once on success");
    assert_eq!(*captured_status.lock().unwrap(), Some(200));
    assert_eq!(captured_rid.lock().unwrap().as_deref(), Some("req_xyz"));
    assert!(
        captured_dur.lock().unwrap().is_some(),
        "duration_ms should be populated"
    );
}

#[tokio::test]
async fn on_response_does_not_fire_on_terminal_error() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let calls = Arc::new(AtomicU32::new(0));
    let c = Arc::clone(&calls);
    let client = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .max_retries(0)
        .timeout(Duration::from_secs(2))
        .on_response(move |_evt| {
            c.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .expect("builder");

    let _ = client
        .render
        .document(RenderInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect_err("401");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "must not fire on error");
}

#[tokio::test]
async fn on_request_and_on_response_panics_do_not_break_the_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(UNUSED_PDF_URL)))
        .mount(&server)
        .await;

    let client = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .max_retries(0)
        .timeout(Duration::from_secs(2))
        .on_request(|_evt| panic!("request hook boom"))
        .on_response(|_evt| panic!("response hook boom"))
        .build()
        .expect("builder");

    // Both hooks panic, but the request must still succeed.
    let result = client
        .render
        .document(RenderInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect("hook panics must be swallowed");
    assert_eq!(result.document_id, "doc_abc123");
}

// =============================================================================
// Environments contract: error codes, publish lock, open environment names
// =============================================================================

#[tokio::test]
async fn render_surfaces_template_not_found_code_and_detail_from_404() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "error": "TEMPLATE_NOT_FOUND",
            "detail": "Template 'invoice' is not published in this environment.",
            "requestId": "req_404"
        })))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let err = client
        .render
        .document(project_input())
        .await
        .expect_err("404");

    assert!(matches!(err, poli_page::Error::NotFound { .. }), "{err:?}");
    assert_eq!(err.code(), poli_page::error_codes::TEMPLATE_NOT_FOUND);
    assert_eq!(err.status(), Some(404));
    assert_eq!(
        err.to_payload().message,
        "Template 'invoice' is not published in this environment.",
    );
}

#[tokio::test]
async fn render_passes_an_unknown_error_code_through_verbatim() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({
            "error": "SOME_FUTURE_CODE",
            "detail": "something new"
        })))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let err = client
        .render
        .document(project_input())
        .await
        .expect_err("409");

    assert_eq!(err.code(), "SOME_FUTURE_CODE");
    assert_eq!(err.status(), Some(409));
    assert_eq!(err.to_payload().message, "something new");
}

fn publish_in_progress() -> ResponseTemplate {
    ResponseTemplate::new(503)
        .insert_header("retry-after", "1")
        .set_body_json(json!({
            "error": "PUBLISH_IN_PROGRESS",
            "detail": "A publish is in progress for this project."
        }))
}

#[tokio::test]
async fn render_retries_publish_in_progress_after_retry_after_delay() {
    use std::sync::{Arc, Mutex};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(publish_in_progress())
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(UNUSED_PDF_URL)))
        .mount(&server)
        .await;

    let retries: Arc<Mutex<Vec<(Duration, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&retries);
    let client = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .retry_delay(Duration::from_millis(1))
        .timeout(Duration::from_secs(5))
        .on_retry(move |evt| {
            captured
                .lock()
                .unwrap()
                .push((evt.delay, evt.reason.code().to_string()));
        })
        .build()
        .expect("builder");

    let start = std::time::Instant::now();
    let doc = client
        .render
        .document(project_input())
        .await
        .expect("retry should have succeeded");
    let elapsed = start.elapsed();

    assert_eq!(doc.document_id, "doc_abc123");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
    assert_eq!(
        *retries.lock().unwrap(),
        vec![(Duration::from_secs(1), "PUBLISH_IN_PROGRESS".to_string())],
    );
    assert!(elapsed >= Duration::from_secs(1), "took {elapsed:?}");
}

#[tokio::test]
async fn render_surfaces_publish_in_progress_once_retries_are_exhausted() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(publish_in_progress())
        .mount(&server)
        .await;

    let client = PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .retry_delay(Duration::from_millis(1))
        .timeout(Duration::from_secs(5))
        .build()
        .expect("builder");

    let start = std::time::Instant::now();
    let err = client
        .render
        .document(project_input())
        .await
        .expect_err("exhausted retries");
    let elapsed = start.elapsed();

    assert_eq!(err.code(), poli_page::error_codes::PUBLISH_IN_PROGRESS);
    assert_eq!(err.status(), Some(503));
    assert!(err.is_retryable());
    // Default policy: initial attempt + 2 retries, each after Retry-After: 1.
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
    assert!(elapsed >= Duration::from_secs(2), "took {elapsed:?}");
}

#[tokio::test]
async fn render_document_passes_any_environment_name_through() {
    let server = MockServer::start().await;
    let mut wire = descriptor_wire(UNUSED_PDF_URL);
    wire["environment"] = json!("staging");
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(wire))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let doc = client
        .render
        .document(project_input())
        .await
        .expect("document");
    assert_eq!(doc.environment, "staging");
}
