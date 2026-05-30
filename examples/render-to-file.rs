//! Demonstrates: poli_page::render_to_file(&client, input, path) — bounded-memory disk write.
use poli_page::{PoliPage, ProjectModeInput};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = PoliPage::new(std::env::var("POLI_PAGE_API_KEY")?)?;

    poli_page::render_to_file(
        &client,
        ProjectModeInput {
            project: "billing".into(),
            template: "invoice".into(),
            version: Some("1.0.0".into()),
            data: json!({ "invoiceNumber": "INV-001", "total": 1280 }),
            ..Default::default()
        },
        "./invoices/INV-001.pdf",
    )
    .await?;

    // Streams response bytes directly to disk with bounded memory.
    // Parent directories are created automatically.
    Ok(())
}
