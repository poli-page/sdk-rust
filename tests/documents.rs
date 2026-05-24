//! End-to-end wiremock coverage for the `documents` namespace.
//!
//! Ports the documentsGet / documentsPreview / documentsThumbnails /
//! documentsDelete describe blocks from
//! `sdk-node/tests/documents.test.ts`.

use std::time::Duration;

use poli_page::{Environment, PageFormat, PoliPage, ThumbnailFormat, ThumbnailOptions};
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

async fn build_client(uri: String) -> PoliPage {
    PoliPage::builder()
        .api_key("pp_test_x")
        .base_url(uri)
        .max_retries(0)
        .retry_delay(Duration::from_millis(1))
        .timeout(Duration::from_secs(2))
        .build()
        .expect("builder")
}

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

// =============================================================================
// documents.get
// =============================================================================

#[tokio::test]
async fn get_fetches_v1_documents_id_and_returns_descriptor() {
    let server = MockServer::start().await;
    let presigned = format!("{}/presigned/x.pdf", server.uri());
    Mock::given(method("GET"))
        .and(path("/v1/documents/doc_abc123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let doc = client.documents.get("doc_abc123").await.expect("get");
    assert_eq!(doc.document_id, "doc_abc123");
    assert_eq!(doc.template_slug.as_deref(), Some("invoice"));
    assert_eq!(doc.format, PageFormat::A4);
    assert_eq!(doc.environment, Environment::Live);
    assert_eq!(doc.presigned_pdf_url, presigned);
}

#[tokio::test]
async fn get_sends_no_request_body() {
    let server = MockServer::start().await;
    let presigned = format!("{}/x.pdf", server.uri());
    Mock::given(method("GET"))
        .and(path("/v1/documents/doc_abc123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
        .mount(&server)
        .await;
    let client = build_client(server.uri()).await;
    let _ = client.documents.get("doc_abc123").await.expect("get");
    let req = &server.received_requests().await.unwrap()[0];
    assert!(req.body.is_empty(), "GET sent a body: {:?}", req.body);
    // No idempotency-key on GETs either.
    assert!(req.headers.get("idempotency-key").is_none());
}

#[tokio::test]
async fn get_attaches_client_back_ref_so_download_pdf_succeeds() {
    let server = MockServer::start().await;
    let presigned = format!("{}/presigned/y.pdf", server.uri());
    Mock::given(method("GET"))
        .and(path("/v1/documents/doc_abc123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/presigned/y.pdf"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"%PDF-1.4 stub" as &[u8]))
        .mount(&server)
        .await;
    let client = build_client(server.uri()).await;
    let doc = client.documents.get("doc_abc123").await.expect("get");
    let bytes = doc.download_pdf().await.expect("download_pdf");
    assert_eq!(&bytes[..4], b"%PDF");
}

#[tokio::test]
async fn get_encodes_special_characters_in_id() {
    let server = MockServer::start().await;
    let presigned = format!("{}/x.pdf", server.uri());
    Mock::given(method("GET"))
        .and(path("/v1/documents/doc%2Fwith%2Fslashes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
        .mount(&server)
        .await;
    let client = build_client(server.uri()).await;
    let doc = client.documents.get("doc/with/slashes").await.expect("get");
    assert_eq!(doc.document_id, "doc_abc123");
}

// =============================================================================
// documents.preview
// =============================================================================

#[tokio::test]
async fn preview_returns_html_and_page_count_from_header() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/documents/doc_abc123/preview"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html; charset=utf-8")
                .insert_header("x-document-page-count", "4")
                .set_body_string("<p>stored preview</p>"),
        )
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let result = client
        .documents
        .preview("doc_abc123")
        .await
        .expect("preview");
    assert_eq!(result.html, "<p>stored preview</p>");
    assert_eq!(result.page_count, 4);
}

#[tokio::test]
async fn preview_defaults_page_count_to_zero_when_header_missing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/documents/doc_abc123/preview"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html; charset=utf-8")
                .set_body_string("<p>x</p>"),
        )
        .mount(&server)
        .await;
    let client = build_client(server.uri()).await;
    let result = client
        .documents
        .preview("doc_abc123")
        .await
        .expect("preview");
    assert_eq!(result.page_count, 0);
}

#[tokio::test]
async fn preview_defaults_page_count_to_zero_when_header_unparseable() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/documents/doc_abc123/preview"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html; charset=utf-8")
                .insert_header("x-document-page-count", "not-a-number")
                .set_body_string("<p>x</p>"),
        )
        .mount(&server)
        .await;
    let client = build_client(server.uri()).await;
    let result = client
        .documents
        .preview("doc_abc123")
        .await
        .expect("preview");
    assert_eq!(result.page_count, 0);
}

#[tokio::test]
async fn preview_sends_no_request_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/documents/doc_abc123/preview"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html; charset=utf-8")
                .insert_header("x-document-page-count", "1")
                .set_body_string("<p>x</p>"),
        )
        .mount(&server)
        .await;
    let client = build_client(server.uri()).await;
    let _ = client
        .documents
        .preview("doc_abc123")
        .await
        .expect("preview");
    let req = &server.received_requests().await.unwrap()[0];
    assert!(req.body.is_empty());
}

#[tokio::test]
async fn preview_encodes_special_characters_in_id() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/documents/doc%2Fwith%2Fslashes/preview"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html; charset=utf-8")
                .insert_header("x-document-page-count", "1")
                .set_body_string("<p>x</p>"),
        )
        .mount(&server)
        .await;
    let client = build_client(server.uri()).await;
    let _ = client
        .documents
        .preview("doc/with/slashes")
        .await
        .expect("preview");
}

// =============================================================================
// documents.thumbnails
// =============================================================================

fn sample_thumb() -> serde_json::Value {
    json!({
        "page": 1,
        "width": 840,
        "height": 1188,
        "contentType": "image/png",
        "data": "iVBORw0KGgoAAAANSU="
    })
}

#[tokio::test]
async fn thumbnails_posts_with_options_wrapped_in_thumbnails_key() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/documents/doc_abc123/thumbnails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "thumbnails": [sample_thumb()]
        })))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let _ = client
        .documents
        .thumbnails(
            "doc_abc123",
            ThumbnailOptions {
                width: 840,
                format: Some(ThumbnailFormat::Png),
                ..ThumbnailOptions::new(840)
            },
        )
        .await
        .expect("thumbnails");
    let req = &server.received_requests().await.unwrap()[0];
    let body: serde_json::Value = req.body_json().expect("json body");
    assert_eq!(
        body,
        json!({ "thumbnails": { "width": 840, "format": "png" } })
    );
}

#[tokio::test]
async fn thumbnails_forwards_all_options_inside_the_wrap() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/documents/doc_abc123/thumbnails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "thumbnails": [] })))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let _ = client
        .documents
        .thumbnails(
            "doc_abc123",
            ThumbnailOptions {
                width: 320,
                format: Some(ThumbnailFormat::Jpeg),
                quality: Some(85),
                pages: Some(vec![1, 2, 3]),
            },
        )
        .await
        .expect("thumbnails");
    let req = &server.received_requests().await.unwrap()[0];
    let body: serde_json::Value = req.body_json().expect("json body");
    assert_eq!(
        body["thumbnails"],
        json!({ "width": 320, "format": "jpeg", "quality": 85, "pages": [1, 2, 3] }),
    );
}

#[tokio::test]
async fn thumbnails_unwraps_response_envelope_and_returns_vec() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/documents/doc_abc123/thumbnails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "thumbnails": [
                sample_thumb(),
                { "page": 2, "width": 840, "height": 1188, "contentType": "image/png", "data": "x=" }
            ]
        })))
        .mount(&server)
        .await;

    let client = build_client(server.uri()).await;
    let thumbs = client
        .documents
        .thumbnails("doc_abc123", ThumbnailOptions::new(840))
        .await
        .expect("thumbnails");
    assert_eq!(thumbs.len(), 2);
    assert_eq!(thumbs[0].page, 1);
    assert_eq!(thumbs[1].page, 2);
    assert_eq!(thumbs[0].content_type, "image/png");
}

#[tokio::test]
async fn thumbnails_encodes_special_characters_in_id() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/documents/doc%2Fwith%2Fslashes/thumbnails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "thumbnails": [] })))
        .mount(&server)
        .await;
    let client = build_client(server.uri()).await;
    let _ = client
        .documents
        .thumbnails("doc/with/slashes", ThumbnailOptions::new(100))
        .await
        .expect("thumbnails");
}

#[tokio::test]
async fn thumbnails_sends_auto_generated_idempotency_key() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/documents/doc_abc123/thumbnails"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "thumbnails": [] })))
        .mount(&server)
        .await;
    let client = build_client(server.uri()).await;
    let _ = client
        .documents
        .thumbnails("doc_abc123", ThumbnailOptions::new(100))
        .await
        .expect("thumbnails");
    let req = &server.received_requests().await.unwrap()[0];
    let idem = req
        .headers
        .get("idempotency-key")
        .expect("idempotency-key header");
    assert_eq!(idem.to_str().unwrap().len(), 36); // UUID v4
}

// =============================================================================
// documents.delete
// =============================================================================

#[tokio::test]
async fn delete_sends_delete_to_v1_documents_id_and_returns_unit() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/v1/documents/doc_abc123"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let client = build_client(server.uri()).await;
    let result = client.documents.delete("doc_abc123").await;
    assert!(result.is_ok());
    let req = &server.received_requests().await.unwrap()[0];
    assert_eq!(req.method.as_str(), "DELETE");
    assert_eq!(req.url.path(), "/v1/documents/doc_abc123");
    assert!(req.body.is_empty());
    // DELETE is naturally idempotent — no Idempotency-Key per spec §5.3.
    assert!(req.headers.get("idempotency-key").is_none());
}

#[tokio::test]
async fn delete_encodes_special_characters_in_id() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/v1/documents/doc%2Fwith%2Fslashes"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let client = build_client(server.uri()).await;
    let result = client.documents.delete("doc/with/slashes").await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn delete_surfaces_gone_when_already_deleted() {
    let server = MockServer::start().await;
    Mock::given(method("DELETE"))
        .and(path("/v1/documents/doc_abc123"))
        .respond_with(
            ResponseTemplate::new(410)
                .set_body_json(json!({ "code": "GONE", "message": "already deleted" })),
        )
        .mount(&server)
        .await;
    let client = build_client(server.uri()).await;
    let err = client
        .documents
        .delete("doc_abc123")
        .await
        .expect_err("gone");
    assert!(matches!(err, poli_page::Error::Gone { .. }));
    assert_eq!(err.status(), Some(410));
}

#[allow(dead_code)]
fn _ensure_request_import_used(_: &Request) {}
