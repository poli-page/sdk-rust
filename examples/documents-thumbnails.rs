//! Demonstrates: client.documents.thumbnails(id, options) — page thumbnails.
use poli_page::{PoliPage, ThumbnailFormat, ThumbnailOptions};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = PoliPage::new(std::env::var("POLI_PAGE_API_KEY")?)?;

    let thumbs = client
        .documents
        .thumbnails(
            "doc_INV-001",
            ThumbnailOptions {
                width: 320,
                format: Some(ThumbnailFormat::Png),
                ..ThumbnailOptions::new(320)
            },
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
