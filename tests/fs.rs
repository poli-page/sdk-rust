//! `render_to_file` end-to-end test against a wiremock server.

use std::time::Duration;

use poli_page::{PoliPage, ProjectModeInput};
use serde_json::json;
use tempfile::tempdir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PDF_STUB: &[u8] = b"%PDF-1.4 stub-from-stream";

fn descriptor_wire(presigned_url: &str) -> serde_json::Value {
    json!({
        "documentId": "doc_abc",
        "organizationId": "org_x",
        "projectId": "proj_42",
        "projectSlug": "billing",
        "templateId": "tpl_1",
        "templateSlug": "invoice",
        "version": "1.0.0",
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

async fn make_servers() -> (MockServer, String) {
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
    (server, "/presigned/x.pdf".to_string())
}

fn input() -> ProjectModeInput {
    ProjectModeInput {
        project: "p".into(),
        template: "t".into(),
        version: Some("1.0.0".into()),
        data: json!({}),
        ..Default::default()
    }
}

#[tokio::test]
async fn render_to_file_writes_pdf_bytes_to_destination() {
    let (server, _) = make_servers().await;
    let client = PoliPage::builder()
        .api_key("pp_test_x")
        .base_url(server.uri())
        .timeout(Duration::from_secs(2))
        .max_retries(0)
        .build()
        .unwrap();

    let dir = tempdir().unwrap();
    let target = dir.path().join("out.pdf");
    poli_page::render_to_file(&client, input(), &target)
        .await
        .expect("render_to_file");

    let bytes = std::fs::read(&target).unwrap();
    assert_eq!(bytes, PDF_STUB);
}

#[tokio::test]
async fn render_to_file_creates_missing_parent_directories() {
    let (server, _) = make_servers().await;
    let client = PoliPage::builder()
        .api_key("pp_test_x")
        .base_url(server.uri())
        .timeout(Duration::from_secs(2))
        .max_retries(0)
        .build()
        .unwrap();

    let dir = tempdir().unwrap();
    let nested = dir.path().join("does/not/exist/yet/out.pdf");
    poli_page::render_to_file(&client, input(), &nested)
        .await
        .expect("render_to_file with nested path");
    assert!(nested.exists());
    assert_eq!(std::fs::read(&nested).unwrap(), PDF_STUB);
}

#[tokio::test]
async fn render_to_file_overwrites_existing_file() {
    let (server, _) = make_servers().await;
    let client = PoliPage::builder()
        .api_key("pp_test_x")
        .base_url(server.uri())
        .timeout(Duration::from_secs(2))
        .max_retries(0)
        .build()
        .unwrap();

    let dir = tempdir().unwrap();
    let target = dir.path().join("out.pdf");
    std::fs::write(&target, b"old contents that should be replaced").unwrap();
    poli_page::render_to_file(&client, input(), &target)
        .await
        .expect("render_to_file overwriting");
    assert_eq!(std::fs::read(&target).unwrap(), PDF_STUB);
}

#[tokio::test]
async fn render_to_file_surfaces_download_failed_on_presigned_404() {
    let server = MockServer::start().await;
    let presigned = format!("{}/presigned/missing.pdf", server.uri());
    Mock::given(method("POST"))
        .and(path("/v1/render"))
        .respond_with(ResponseTemplate::new(200).set_body_json(descriptor_wire(&presigned)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/presigned/missing.pdf"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let client = PoliPage::builder()
        .api_key("pp_test_x")
        .base_url(server.uri())
        .timeout(Duration::from_secs(2))
        .max_retries(0)
        .build()
        .unwrap();
    let dir = tempdir().unwrap();
    let err = poli_page::render_to_file(&client, input(), dir.path().join("out.pdf"))
        .await
        .expect_err("download must fail");
    assert!(matches!(err, poli_page::Error::Download { .. }));
    assert_eq!(err.status(), Some(404));
}
