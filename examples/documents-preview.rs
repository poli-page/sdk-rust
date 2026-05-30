//! Demonstrates: client.documents.preview(id) — paginated HTML for a stored document.
use poli_page::PoliPage;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = PoliPage::new(std::env::var("POLI_PAGE_API_KEY")?)?;

    let preview = client.documents.preview("doc_INV-001").await?;
    println!(
        "Preview: {} pages, html.len={}",
        preview.page_count,
        preview.html.len()
    );
    Ok(())
}
