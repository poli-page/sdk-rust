//! Demonstrates: client.documents.thumbnails(id, options) — page thumbnails.
//!
//! Pass `None` for the API defaults (400 px wide JPEG).
use poli_page::{PoliPage, ThumbnailFormat, ThumbnailOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = PoliPage::new(std::env::var("POLI_PAGE_API_KEY")?)?;

    let thumbs = client
        .documents
        .thumbnails(
            "doc_INV-001",
            Some(ThumbnailOptions {
                format: Some(ThumbnailFormat::Png),
                ..ThumbnailOptions::new().width(320)
            }),
        )
        .await?;

    for t in &thumbs {
        println!(
            "page {} — {}×{} {}",
            t.page, t.width, t.height, t.content_type
        );
    }
    Ok(())
}
