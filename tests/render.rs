//! End-to-end wiremock coverage for the `render` namespace.
//!
//! Ports the renderPreview / renderPdf / renderDocument / renderPdfStream
//! describe blocks from `sdk-node/tests/render.test.ts`. Uses `wiremock`
//! (Rust's async-native HTTP mock) so the tests exercise the real `reqwest`
//! transport, headers, body serialization, response parsing, and the
//! two-hop second fetch against the "presigned" URL — only the network is
//! faked.

use std::time::Duration;

use futures_util::StreamExt;
use poli_page::{
    DocumentDescriptor, Environment, InlineModeInput, PageFormat, PoliPage, ProjectModeInput,
};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

// `async` kept so every call site reads `build_client(...).await` symmetrically
// with the namespace methods being tested — no real work happens inside.
#[allow(clippy::unused_async)]
async fn build_client(uri: String) -> PoliPage {
    PoliPage::builder()
        .api_key("pp_test_x")
        .base_url(uri)
        // Keep retries fast so error-path tests don't dawdle.
        .max_retries(0)
        .retry_delay(Duration::from_millis(1))
        .timeout(Duration::from_secs(2))
        .build()
        .expect("builder")
}

#[tokio::test]
async fn preview_posts_to_v1_render_preview_and_parses_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render/preview"))
        .and(header("authorization", "Bearer pp_test_x"))
        .and(header("content-type", "application/json"))
        .and(header("accept", "application/json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "html": "<p>preview</p>",
            "totalPages": 3,
            "environment": "sandbox"
        })))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let result = client
        .render
        .preview(ProjectModeInput {
            project: "billing".into(),
            template: "invoice".into(),
            version: Some("1.0.0".into()),
            data: json!({ "x": 1 }),
            ..Default::default()
        })
        .await
        .expect("preview");

    assert_eq!(result.html, "<p>preview</p>");
    assert_eq!(result.total_pages, 3);
    assert_eq!(result.environment, Environment::Sandbox);
}

#[tokio::test]
async fn preview_serializes_project_body_with_camel_case_keys() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render/preview"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "html": "<p>x</p>", "totalPages": 1, "environment": "sandbox"
        })))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    client
        .render
        .preview(ProjectModeInput {
            project: "billing".into(),
            template: "invoice".into(),
            version: Some("1.0.0".into()),
            data: json!({ "amount": 1280 }),
            ..Default::default()
        })
        .await
        .expect("preview");

    let req = &server.received_requests().await.expect("requests")[0];
    let body: serde_json::Value = req.body_json().expect("json body");
    assert_eq!(body["project"], "billing");
    assert_eq!(body["template"], "invoice");
    assert_eq!(body["version"], "1.0.0");
    assert_eq!(body["data"]["amount"], 1280);
    // Stripped fields never make the wire.
    let obj = body.as_object().unwrap();
    assert!(!obj.contains_key("idempotencyKey"));
    assert!(!obj.contains_key("timeout"));
}

#[tokio::test]
async fn preview_accepts_inline_mode_input() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render/preview"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "html": "<h1>inline</h1>", "totalPages": 1, "environment": "sandbox"
        })))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let result = client
        .render
        .preview(InlineModeInput {
            template: "<h1>inline</h1>".into(),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect("preview");
    assert_eq!(result.html, "<h1>inline</h1>");

    let req = &server.received_requests().await.expect("requests")[0];
    let body: serde_json::Value = req.body_json().expect("json body");
    assert_eq!(body["template"], "<h1>inline</h1>");
    // Inline mode has no project / version on the wire.
    let obj = body.as_object().unwrap();
    assert!(!obj.contains_key("project"));
    assert!(!obj.contains_key("version"));
}

#[tokio::test]
async fn preview_forwards_metadata_in_request_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render/preview"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "html": "<p>x</p>", "totalPages": 1, "environment": "sandbox"
        })))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let mut metadata = poli_page::RenderMetadata::new();
    metadata.insert("customerId".into(), "cust_1".into());
    metadata.insert("amount".into(), 1280i64.into());
    client
        .render
        .preview(ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            version: Some("1.0.0".into()),
            data: json!({}),
            metadata: Some(metadata),
            ..Default::default()
        })
        .await
        .expect("preview");

    let req = &server.received_requests().await.expect("requests")[0];
    let body: serde_json::Value = req.body_json().expect("json body");
    assert_eq!(body["metadata"]["customerId"], "cust_1");
    assert_eq!(body["metadata"]["amount"], 1280);
}

#[tokio::test]
async fn preview_sends_auto_generated_idempotency_key_for_post() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render/preview"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "html": "<p>x</p>", "totalPages": 1, "environment": "sandbox"
        })))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    client
        .render
        .preview(ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            version: Some("1.0.0".into()),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect("preview");

    let req = &server.received_requests().await.expect("requests")[0];
    let idem = req
        .headers
        .get("idempotency-key")
        .expect("idempotency-key header");
    // UUID v4 is 36 chars.
    assert_eq!(idem.to_str().unwrap().len(), 36);
}

#[tokio::test]
async fn preview_honors_caller_supplied_idempotency_key() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render/preview"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "html": "<p>x</p>", "totalPages": 1, "environment": "sandbox"
        })))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    client
        .render
        .preview(ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            version: Some("1.0.0".into()),
            data: json!({}),
            idempotency_key: Some("idem-caller-supplied".into()),
            ..Default::default()
        })
        .await
        .expect("preview");

    let req = &server.received_requests().await.expect("requests")[0];
    let idem = req.headers.get("idempotency-key").unwrap();
    assert_eq!(idem.to_str().unwrap(), "idem-caller-supplied");
}

#[tokio::test]
async fn preview_surfaces_validation_error_from_400_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render/preview"))
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
        .preview(ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            version: Some("1.0.0".into()),
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
async fn preview_retries_on_503_then_succeeds() {
    let server = MockServer::start().await;
    // First call: 503; second call: 200. Mock.up_to_n_times scopes the response.
    Mock::given(method("POST"))
        .and(path("/v1/render/preview"))
        .respond_with(ResponseTemplate::new(503).set_body_string("transient"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/render/preview"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "html": "<p>ok</p>", "totalPages": 1, "environment": "sandbox"
        })))
        .mount(&server)
        .await;

    // Use a client that DOES retry — overriding the helper's max_retries=0.
    let client = PoliPage::builder()
        .api_key("pp_test_x")
        .base_url(server.uri())
        .max_retries(1)
        .retry_delay(Duration::from_millis(1))
        .timeout(Duration::from_secs(2))
        .build()
        .expect("builder");

    let result = client
        .render
        .preview(ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            version: Some("1.0.0".into()),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect("retry should have produced success");
    assert_eq!(result.html, "<p>ok</p>");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn preview_does_not_retry_401_auth_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render/preview"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({ "code": "INVALID_API_KEY", "message": "nope" })),
        )
        .mount(&server)
        .await;

    let client = PoliPage::builder()
        .api_key("pp_test_x")
        .base_url(server.uri())
        .max_retries(3) // would retry if 401 were retryable
        .retry_delay(Duration::from_millis(1))
        .timeout(Duration::from_secs(2))
        .build()
        .expect("builder");

    let err = client
        .render
        .preview(ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            version: Some("1.0.0".into()),
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
async fn preview_honors_retry_after_header_on_429() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render/preview"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "0")
                .set_body_json(json!({ "code": "RATE_LIMITED", "message": "slow down" })),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/render/preview"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "html": "<p>x</p>", "totalPages": 1, "environment": "sandbox"
        })))
        .mount(&server)
        .await;

    let retry_count = Arc::new(AtomicU32::new(0));
    let rc = Arc::clone(&retry_count);
    let client = PoliPage::builder()
        .api_key("pp_test_x")
        .base_url(server.uri())
        .max_retries(1)
        .retry_delay(Duration::from_millis(50)) // would dominate wall-time if Retry-After ignored
        .timeout(Duration::from_secs(2))
        .on_retry(move |_evt| {
            rc.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .expect("builder");

    let start = std::time::Instant::now();
    client
        .render
        .preview(ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            version: Some("1.0.0".into()),
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
    // retry_delay (50ms) would suggest.
    assert!(elapsed < Duration::from_millis(40), "took {elapsed:?}");
}

#[tokio::test]
async fn preview_fires_on_error_when_retries_exhausted() {
    use std::sync::Arc;
    use std::sync::Mutex;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render/preview"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let last_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let captured = Arc::clone(&last_error);
    let client = PoliPage::builder()
        .api_key("pp_test_x")
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
        .preview(ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            version: Some("1.0.0".into()),
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
async fn preview_hook_panic_does_not_break_the_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render/preview"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "html": "<p>x</p>", "totalPages": 1, "environment": "sandbox"
        })))
        .mount(&server)
        .await;

    let client = PoliPage::builder()
        .api_key("pp_test_x")
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
        .preview(ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            version: Some("1.0.0".into()),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect("ok");

    // Now make on_error fire by triggering a non-retryable error.
    let server2 = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render/preview"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server2)
        .await;
    let client2 = PoliPage::builder()
        .api_key("pp_test_x")
        .base_url(server2.uri())
        .max_retries(0)
        .timeout(Duration::from_secs(2))
        .on_error(|_| panic!("hook went boom on error path"))
        .build()
        .expect("builder");

    let err = client2
        .render
        .preview(ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            version: Some("1.0.0".into()),
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
        "version": "1.0.0",
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
        .document(ProjectModeInput {
            project: "billing".into(),
            template: "invoice".into(),
            version: Some("1.0.0".into()),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect("document");

    assert_eq!(doc.document_id, "doc_abc123");
    assert_eq!(doc.template_slug.as_deref(), Some("invoice"));
    assert_eq!(doc.page_count, 2);
    assert_eq!(doc.format, PageFormat::A4);
    assert_eq!(doc.environment, Environment::Live);
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
        .pdf(ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            version: Some("1.0.0".into()),
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
        .api_key("pp_test_x")
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

#[tokio::test]
async fn render_pdf_inline_input_does_not_compile_typecheck() {
    // This test exists for its compile-time signal: render.pdf statically
    // requires ProjectModeInput. The line below is a structural sanity check
    // that we can construct a ProjectModeInput and pass it — paired with the
    // doc-text in §9.1 about InlineModeInput being un-typeable here.
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
    let _bytes = client
        .render
        .pdf(ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            version: Some("1.0.0".into()),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect("pdf");
    // Asserting on type only: this MUST fail to compile if uncommented.
    //   let _ = client.render.pdf(InlineModeInput { ... }).await;
    //
    // (We can't put a compile-fail assertion here without `trybuild`, which
    // is a Phase 6 deliverable.)
    let _ = InlineModeInput {
        template: "<p/>".into(),
        data: json!({}),
        ..Default::default()
    };
}

/// Proves `PoliPageBuilder::http_client` is actually used for requests:
/// the injected client carries a default header the SDK never sets; the
/// mock only matches when that header is present on the wire.
#[tokio::test]
async fn render_uses_injected_http_client() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/render/preview"))
        .and(header("x-sdk-test-marker", "injected-client"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "html": "<p>p</p>",
            "totalPages": 1,
            "environment": "sandbox"
        })))
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
        .api_key("pp_test_x")
        .base_url(server.uri())
        .max_retries(0)
        .http_client(injected)
        .build()
        .expect("builder");

    client
        .render
        .preview(project_input())
        .await
        .expect("preview");
}

fn project_input() -> ProjectModeInput {
    ProjectModeInput {
        project: "p".into(),
        template: "t".into(),
        version: Some("1.0.0".into()),
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
        .api_key("pp_test_x")
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
        .and(path("/v1/render/preview"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/render/preview"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "html": "<p>ok</p>", "totalPages": 1, "environment": "sandbox"
        })))
        .mount(&server)
        .await;

    let events: Arc<Mutex<Vec<(String, String, u32)>>> = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&events);
    let count = Arc::new(AtomicU32::new(0));
    let c = Arc::clone(&count);
    let client = PoliPage::builder()
        .api_key("pp_test_x")
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
        .preview(ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            version: Some("1.0.0".into()),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect("eventual success");

    assert_eq!(count.load(Ordering::SeqCst), 3, "fired once per attempt");
    let evts = events.lock().unwrap();
    assert_eq!(evts[0].0, "POST");
    assert!(evts[0].1.ends_with("/v1/render/preview"), "url was {}", evts[0].1);
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
        .and(path("/v1/render/preview"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/render/preview"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-request-id", "req_xyz")
                .set_body_json(json!({
                    "html": "<p>ok</p>", "totalPages": 1, "environment": "sandbox"
                })),
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
        .api_key("pp_test_x")
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
        .preview(ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            version: Some("1.0.0".into()),
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
        .and(path("/v1/render/preview"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let calls = Arc::new(AtomicU32::new(0));
    let c = Arc::clone(&calls);
    let client = PoliPage::builder()
        .api_key("pp_test_x")
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
        .preview(ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            version: Some("1.0.0".into()),
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
        .and(path("/v1/render/preview"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "html": "<p>x</p>", "totalPages": 1, "environment": "sandbox"
        })))
        .mount(&server)
        .await;

    let client = PoliPage::builder()
        .api_key("pp_test_x")
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
        .preview(ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            version: Some("1.0.0".into()),
            data: json!({}),
            ..Default::default()
        })
        .await
        .expect("hook panics must be swallowed");
    assert_eq!(result.total_pages, 1);
}
