//! Smoke tests for the `blocking` feature surface.
//!
//! Gated on `#[cfg(feature = "blocking")]` so default `cargo test` produces
//! zero tests here; `cargo test --features blocking` (or the all-features
//! run) compiles and runs them.

#![cfg(feature = "blocking")]

use std::time::Duration;

use poli_page::blocking::PoliPage as BlockingPoliPage;
use poli_page::{RenderInput, ThumbnailOptions};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PDF_STUB: &[u8] = b"%PDF-1.4 blocking-stub";

fn descriptor_wire(presigned_url: &str) -> serde_json::Value {
    json!({
        "documentId": "doc_blk",
        "organizationId": "org_x",
        "projectId": "proj_42",
        "projectSlug": "billing",
        "templateId": "tpl_1",
        "templateSlug": "invoice",
        "environment": "live",
        "apiKeyId": "key_x",
        "format": "A4",
        "orientation": "portrait",
        "locale": "en-US",
        "pageCount": 1,
        "sizeBytes": PDF_STUB.len() as i64,
        "createdAt": "2026-05-24T00:00:00Z",
        "metadata": {},
        "presignedPdfUrl": presigned_url,
        "expiresAt": "2026-05-24T00:15:00Z"
    })
}

fn input() -> RenderInput {
    RenderInput {
        project: "p".into(),
        template: "t".into(),
        data: json!({}),
        ..Default::default()
    }
}

// The wiremock MockServer is async-only — we spin up a small tokio runtime
// just to start it, then drive the blocking client against the resulting
// uri synchronously. This mirrors how a real sync user would talk to a real
// HTTP server.
fn mock_server() -> (MockServer, tokio::runtime::Runtime) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let server = rt.block_on(async {
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
        Mock::given(method("GET"))
            .and(path("/v1/documents/doc_blk"))
            .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/documents/doc_blk/thumbnails"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "thumbnails": [
                    { "page": 1, "width": 320, "height": 453, "contentType": "image/png", "data": "x=" }
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/v1/documents/doc_blk"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/documents/doc_blk/preview"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/html; charset=utf-8")
                    .insert_header("x-document-page-count", "1")
                    .set_body_string("<p>stored</p>"),
            )
            .mount(&server)
            .await;
        server
    });
    (server, rt)
}

fn client(uri: String) -> BlockingPoliPage {
    BlockingPoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(uri)
        .timeout(Duration::from_secs(2))
        .max_retries(0)
        .build()
        .expect("blocking builder")
}

#[test]
fn blocking_document_runs_without_a_caller_runtime() {
    let (server, _rt) = mock_server();
    let client = client(server.uri());
    // No #[tokio::test] — purely synchronous call site.
    let doc = client.render.document(input()).expect("document");
    assert_eq!(doc.document_id, "doc_blk");
    assert_eq!(doc.environment, "live");
}

#[test]
fn blocking_pdf_two_hop_returns_bytes() {
    let (server, _rt) = mock_server();
    let client = client(server.uri());
    let bytes = client.render.pdf(input()).expect("pdf");
    assert_eq!(&bytes[..4], b"%PDF");
}

#[test]
fn blocking_pdf_stream_implements_read() {
    use std::io::Read;
    let (server, _rt) = mock_server();
    let client = client(server.uri());
    let mut reader = client.render.pdf_stream(input()).expect("pdf_stream");
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf).expect("read_to_end");
    assert_eq!(buf, PDF_STUB);
}

#[test]
fn blocking_render_to_file_writes_bytes() {
    let (server, _rt) = mock_server();
    let client = client(server.uri());
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("blocking-out.pdf");
    poli_page::blocking::render_to_file(&client, input(), &target)
        .expect("blocking render_to_file");
    assert_eq!(std::fs::read(&target).unwrap(), PDF_STUB);
}

#[test]
fn blocking_documents_round_trip_get_thumbnails_preview_delete() {
    let (server, _rt) = mock_server();
    let client = client(server.uri());

    let doc = client.documents.get("doc_blk").expect("get");
    assert_eq!(doc.document_id, "doc_blk");

    let thumbs = client
        .documents
        .thumbnails("doc_blk", None)
        .expect("thumbnails");
    assert_eq!(thumbs.len(), 1);
    let thumbs = client
        .documents
        .thumbnails("doc_blk", Some(ThumbnailOptions::new().width(320)))
        .expect("thumbnails with options");
    assert_eq!(thumbs.len(), 1);

    let preview = client.documents.preview("doc_blk").expect("preview");
    assert_eq!(preview.html, "<p>stored</p>");
    assert_eq!(preview.page_count, 1);

    client.documents.delete("doc_blk").expect("delete");
}

#[test]
fn blocking_client_is_clone() {
    // Cloning the blocking client must not move the runtime out — Arc bump.
    let (server, _rt) = mock_server();
    let client = client(server.uri());
    let c2 = client.clone();
    let _ = c2.render.document(input()).expect("clone-then-document");
}

#[test]
fn blocking_builder_accepts_on_request_setter() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    let (server, _rt) = mock_server();
    let count = Arc::new(AtomicU32::new(0));
    let c = Arc::clone(&count);
    let client = poli_page::blocking::PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .max_retries(0)
        .timeout(Duration::from_secs(2))
        .on_request(move |_evt: &poli_page::RequestEvent| {
            c.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .expect("builder");

    let _ = client.render.document(input()).expect("document");
    assert_eq!(count.load(Ordering::SeqCst), 1, "fired once");
}

#[test]
fn blocking_builder_accepts_on_response_setter() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    let (server, _rt) = mock_server();
    let count = Arc::new(AtomicU32::new(0));
    let c = Arc::clone(&count);
    let client = poli_page::blocking::PoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(server.uri())
        .max_retries(0)
        .timeout(Duration::from_secs(2))
        .on_response(move |evt: &poli_page::ResponseEvent| {
            assert_eq!(evt.status, 200);
            c.fetch_add(1, Ordering::SeqCst);
        })
        .build()
        .expect("builder");

    let _ = client.render.document(input()).expect("document");
    assert_eq!(count.load(Ordering::SeqCst), 1, "fired once on success");
}

// ---------------------------------------------------------------------------
// The download's timeout and io_error, through the blocking client
// ---------------------------------------------------------------------------

/// A server whose presigned URL answers only after `delay`.
fn slow_storage(delay: Duration) -> (MockServer, tokio::runtime::Runtime) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let server = rt.block_on(async {
        let server = MockServer::start().await;
        let presigned = format!("{}/presigned/slow.pdf", server.uri());
        Mock::given(method("POST"))
            .and(path("/v1/render"))
            .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/presigned/slow.pdf"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(PDF_STUB)
                    .set_delay(delay),
            )
            .mount(&server)
            .await;
        server
    });
    (server, rt)
}

fn quick_client(uri: String, timeout: Duration) -> BlockingPoliPage {
    BlockingPoliPage::builder()
        .api_key("pp_sandbox_x")
        .base_url(uri)
        .max_retries(0)
        .timeout(timeout)
        .build()
        .unwrap()
}

#[test]
fn blocking_render_to_file_times_out_and_keeps_the_target() {
    let (server, _rt) = slow_storage(Duration::from_secs(5));
    let client = quick_client(server.uri(), Duration::from_millis(300));
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("out.pdf");
    std::fs::write(&target, b"the previous PDF").unwrap();
    let err = poli_page::blocking::render_to_file(&client, input(), &target).expect_err("timeout");
    assert_eq!(err.code(), "DOWNLOAD_FAILED", "{err:?}");
    assert_eq!(err.message(), "Failed to download PDF: timeout");
    assert_eq!(err.status(), None);
    assert!(!err.is_retryable());
    assert_eq!(std::fs::read(&target).unwrap(), b"the previous PDF");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn blocking_pdf_times_out_on_the_download() {
    let (server, _rt) = slow_storage(Duration::from_secs(5));
    let client = quick_client(server.uri(), Duration::from_millis(300));
    let err = client.render.pdf(input()).expect_err("timeout");
    assert_eq!(err.message(), "Failed to download PDF: timeout");
}

#[test]
fn blocking_render_to_file_is_io_error_when_the_target_is_a_directory() {
    let (server, _rt) = mock_server();
    let client = client(server.uri());
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("out.pdf");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep.txt"), b"kept").unwrap();
    let err = poli_page::blocking::render_to_file(&client, input(), &target)
        .expect_err("target is a directory");
    assert_eq!(err.code(), "io_error", "{err:?}");
    assert!(err.message().starts_with("Failed to write PDF: "), "{err}");
    assert_eq!(err.status(), None);
    assert!(std::error::Error::source(&err).is_some());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    assert_eq!(std::fs::read(target.join("keep.txt")).unwrap(), b"kept");
}
