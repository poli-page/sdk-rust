//! Demonstrates: client.render.document(input) — render + store, return descriptor.
use poli_page::{PoliPage, ProjectModeInput};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = PoliPage::new(std::env::var("POLI_PAGE_API_KEY")?)?;

    let doc = client
        .render
        .document(ProjectModeInput {
            project: "billing".into(),
            template: "invoice".into(),
            version: Some("1.0.0".into()),
            data: json!({ "invoiceNumber": "INV-001", "total": 1280 }),
            ..Default::default()
        })
        .await?;

    // `doc.document_id` is stored server-side; use it later via
    // client.documents.* to fetch, preview, thumbnail, or delete.
    println!(
        "Stored as {} ({} pages, {} bytes)",
        doc.document_id, doc.page_count, doc.size_bytes
    );

    // The descriptor knows how to fetch its own PDF bytes via the
    // 15-minute presigned URL it carries.
    let pdf = doc.download_pdf().await?;
    println!("Downloaded {} bytes", pdf.len());
    Ok(())
}
