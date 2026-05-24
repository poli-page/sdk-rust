//! Phase 2 end-to-end test for `client.render.preview`.
//!
//! Ports the renderPreview describe block from
//! `sdk-node/tests/render.test.ts:316-371`. Uses `wiremock` (Rust's
//! async-native HTTP mock) so the test exercises the real `reqwest`
//! transport, headers, body serialization, and response parsing — only
//! the network is faked.

use std::time::Duration;

use poli_page::{Environment, InlineModeInput, PoliPage, ProjectModeInput};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

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
