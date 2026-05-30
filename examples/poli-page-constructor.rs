//! Demonstrates: PoliPage::new + PoliPage::builder — the only entry points.
use poli_page::PoliPage;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Default config — just the API key from the env.
    let _client = PoliPage::new(std::env::var("POLI_PAGE_API_KEY")?)?;

    // Builder for non-default options (timeouts, retries, hooks).
    let client = PoliPage::builder()
        .api_key(std::env::var("POLI_PAGE_API_KEY")?)
        .timeout(Duration::from_secs(60))
        .max_retries(2)
        .build()?;

    // Reuse the same client across every render and document call.
    let _ = &client.render;
    let _ = &client.documents;
    Ok(())
}
