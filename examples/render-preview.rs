//! Demonstrates: client.render.preview(input) — paginated HTML preview.
//!
//! `render.preview` accepts both `ProjectModeInput` and `InlineModeInput` via
//! `impl Into<RenderInput>`. This example uses the inline variant since
//! preview is the only render method that supports inline mode.
use poli_page::{InlineModeInput, PoliPage};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = PoliPage::new(std::env::var("POLI_PAGE_API_KEY")?)?;

    let preview = client
        .render
        .preview(InlineModeInput {
            template: "<h1>Hello {{ name }}</h1>".into(),
            data: json!({ "name": "World" }),
            ..Default::default()
        })
        .await?;

    println!(
        "Preview: {} pages, environment={:?}, html.len={}",
        preview.total_pages,
        preview.environment,
        preview.html.len(),
    );
    Ok(())
}
