//! `render.preview` against the deployed develop API.

use std::env;

use poli_page::{PoliPage, ProjectModeInput};
use serde_json::json;

/// Default test target — `getting-started/welcome` template at `1.0.0`.
/// Mirrors the Node integration tests and the demo script. Override via
/// `POLI_PAGE_TEST_PROJECT` / `POLI_PAGE_TEST_TEMPLATE` / `POLI_PAGE_TEST_VERSION`
/// env vars (spec §16.1).
const DEFAULT_PROJECT: &str = "getting-started";
const DEFAULT_TEMPLATE: &str = "welcome";
const DEFAULT_VERSION: &str = "1.0.0";

fn client() -> PoliPage {
    let key = env::var("POLI_PAGE_API_KEY").expect(
        "POLI_PAGE_API_KEY must be set to run integration tests \
         (these only run with --features integration --ignored)",
    );
    let base_url = env::var("POLI_PAGE_BASE_URL")
        .unwrap_or_else(|_| "https://api-develop.poli.page".to_string());
    PoliPage::builder()
        .api_key(key)
        .base_url(base_url)
        .build()
        .expect("builder")
}

#[tokio::test]
#[ignore = "hits the deployed develop API — run with --ignored + POLI_PAGE_API_KEY set"]
async fn render_preview_returns_html_and_metadata() {
    let client = client();
    let project =
        env::var("POLI_PAGE_TEST_PROJECT").unwrap_or_else(|_| DEFAULT_PROJECT.to_string());
    let template =
        env::var("POLI_PAGE_TEST_TEMPLATE").unwrap_or_else(|_| DEFAULT_TEMPLATE.to_string());
    let version =
        env::var("POLI_PAGE_TEST_VERSION").unwrap_or_else(|_| DEFAULT_VERSION.to_string());

    let result = client
        .render
        .preview(ProjectModeInput {
            project,
            template,
            version: Some(version),
            data: json!({ "name": "World" }),
            ..Default::default()
        })
        .await
        .expect("preview against develop");

    assert!(!result.html.is_empty(), "preview html was empty");
    assert!(result.total_pages > 0, "expected at least one page");
}
