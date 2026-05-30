//! Demonstrates: client.documents.get(id) — refresh descriptor + presigned URL.
use poli_page::PoliPage;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = PoliPage::new(std::env::var("POLI_PAGE_API_KEY")?)?;

    let doc = client.documents.get("doc_INV-001").await?;
    println!("Document {} (created {})", doc.document_id, doc.created_at);
    println!("Presigned URL expires at {}", doc.expires_at);
    Ok(())
}
