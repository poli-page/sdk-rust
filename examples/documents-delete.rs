//! Demonstrates: client.documents.delete(id) — soft-delete a stored document.
use poli_page::PoliPage;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = PoliPage::new(std::env::var("POLI_PAGE_API_KEY")?)?;

    client.documents.delete("doc_INV-001").await?;
    // Returns `()`. Re-deleting an already-deleted document yields Error::Gone.
    Ok(())
}
