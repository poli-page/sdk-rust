//! `render.document` against the deployed API.

use std::env;

use poli_page::{PoliPage, RenderInput};
use serde_json::json;

/// Default test target — the `getting-started` project's `welcome` template,
/// provisioned in every organization's sandbox. Mirrors the Node integration
/// tests and the demo script. Override via `POLI_PAGE_TEST_PROJECT` /
/// `POLI_PAGE_TEST_TEMPLATE` env vars (spec §16.1).
const DEFAULT_PROJECT: &str = "getting-started";
const DEFAULT_TEMPLATE: &str = "welcome";

fn client() -> PoliPage {
    let key = env::var("POLI_PAGE_API_KEY").expect(
        "POLI_PAGE_API_KEY must be set to run integration tests \
         (these only run with --features integration --ignored)",
    );
    let mut builder = PoliPage::builder().api_key(key);
    if let Ok(base_url) = env::var("POLI_PAGE_TEST_BASE_URL") {
        builder = builder.base_url(base_url);
    }
    builder.build().expect("builder")
}

#[tokio::test]
#[ignore = "hits the deployed API — run with --ignored + POLI_PAGE_API_KEY set"]
async fn render_document_returns_a_stored_document() {
    let client = client();
    let project =
        env::var("POLI_PAGE_TEST_PROJECT").unwrap_or_else(|_| DEFAULT_PROJECT.to_string());
    let template =
        env::var("POLI_PAGE_TEST_TEMPLATE").unwrap_or_else(|_| DEFAULT_TEMPLATE.to_string());

    let doc = client
        .render
        .document(RenderInput {
            project,
            template,
            data: json!({ "name": "World" }),
            ..Default::default()
        })
        .await
        .expect("render.document against the deployed API");

    assert!(doc.page_count > 0, "expected at least one page");
    assert!(!doc.environment.is_empty(), "environment was empty");
    let pdf = doc.download_pdf().await.expect("download_pdf");
    assert_eq!(&pdf[..4], b"%PDF");

    client
        .documents
        .delete(&doc.document_id)
        .await
        .expect("delete");
}
