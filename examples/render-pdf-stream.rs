//! Demonstrates: client.render.pdf_stream(input) — bounded-memory streaming.
use futures_core::Stream;
use poli_page::{PoliPage, ProjectModeInput};
use serde_json::json;
use std::future::poll_fn;
use std::pin::Pin;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = PoliPage::new(std::env::var("POLI_PAGE_API_KEY")?)?;

    let stream = client
        .render
        .pdf_stream(ProjectModeInput {
            project: "billing".into(),
            template: "invoice".into(),
            version: Some("1.0.0".into()),
            data: json!({ "invoiceNumber": "INV-001", "total": 1280 }),
            ..Default::default()
        })
        .await?;

    // Pipe directly into an HTTP response, an S3 upload, or any other sink.
    let mut stream = std::pin::pin!(stream);
    let mut total = 0usize;
    while let Some(chunk) = poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
        let chunk = chunk?;
        total += chunk.len();
    }
    println!("Streamed {} bytes", total);
    Ok(())
}
