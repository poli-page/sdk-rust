//! Demonstrates: client.render.pdf(input) — project mode, in-memory bytes.
use poli_page::{PoliPage, ProjectModeInput};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = PoliPage::new(std::env::var("POLI_PAGE_API_KEY")?)?;

    let pdf = client
        .render
        .pdf(ProjectModeInput {
            project: "billing".into(),
            template: "invoice".into(),
            version: Some("1.0.0".into()),
            data: json!({ "invoiceNumber": "INV-001", "total": 1280 }),
            ..Default::default()
        })
        .await?;

    // `pdf` is a `bytes::Bytes` of PDF bytes.
    println!("Rendered {} bytes", pdf.len());
    Ok(())
}
