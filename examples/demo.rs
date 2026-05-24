//! End-to-end runnable demo for the Poli Page Rust SDK.
//!
//! Method order mirrors the Node demo per spec §13 Phase 5:
//! `render.pdf` → `render.pdf_stream` → `render_to_file` → `render.preview`
//! → `render.document` → `documents.get` → `documents.thumbnails` →
//! `documents.preview` → `documents.delete` → trigger an auth-error path.
//!
//! API key resolution:
//!   1. `POLI_PAGE_API_KEY` environment variable.
//!   2. `.env` file at the repo root (manually parsed — no `dotenvy` dep).
//!   3. Interactive `stdin` prompt; pasted keys append to `.env` so future
//!      runs are silent.
//!
//! Run with:
//! ```text
//! cargo run --example demo
//! ```

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use poli_page::{Error, PoliPage, ProjectModeInput, ThumbnailFormat, ThumbnailOptions};
use serde_json::json;

const DEFAULT_PROJECT: &str = "getting-started";
const DEFAULT_TEMPLATE: &str = "welcome";
const DEFAULT_VERSION: &str = "1.0.0";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = resolve_api_key()?;
    let base_url = std::env::var("POLI_PAGE_BASE_URL")
        .unwrap_or_else(|_| "https://api-develop.poli.page".to_string());
    let client = PoliPage::builder()
        .api_key(api_key)
        .base_url(base_url)
        .build()?;

    let out_dir = std::env::temp_dir().join("poli-page-rust-demo");
    fs::create_dir_all(&out_dir)?;

    println!("Output directory: {}", out_dir.display());
    println!();

    let input = || ProjectModeInput {
        project: DEFAULT_PROJECT.into(),
        template: DEFAULT_TEMPLATE.into(),
        version: Some(DEFAULT_VERSION.into()),
        data: json!({ "name": "Mickael" }),
        ..Default::default()
    };

    // ── 1. render.pdf ────────────────────────────────────────────────
    step(1, 9, "render.pdf — buffer the full PDF into bytes");
    let pdf = client.render.pdf(input()).await?;
    let pdf_path = out_dir.join("render-pdf.pdf");
    fs::write(&pdf_path, &pdf)?;
    println!("  → {} bytes ({})", pdf.len(), pdf_path.display());

    // ── 2. render.pdf_stream ──────────────────────────────────────────
    step(2, 9, "render.pdf_stream — async stream of chunks");
    let mut stream = std::pin::pin!(client.render.pdf_stream(input()).await?);
    let stream_path = out_dir.join("render-pdf-stream.pdf");
    let mut file = tokio::fs::File::create(&stream_path).await?;
    let mut total = 0usize;
    use std::future::poll_fn;
    use std::pin::Pin;
    use tokio::io::AsyncWriteExt;
    while let Some(chunk) = poll_fn(|cx| {
        use futures_core::Stream;
        Pin::new(&mut stream).poll_next(cx)
    })
    .await
    {
        let chunk = chunk?;
        total += chunk.len();
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    println!("  → {} bytes streamed ({})", total, stream_path.display(),);

    // ── 3. render_to_file ────────────────────────────────────────────
    step(3, 9, "render_to_file — bounded-memory disk write");
    let to_file_path = out_dir.join("render-to-file.pdf");
    poli_page::render_to_file(&client, input(), &to_file_path).await?;
    let written = fs::metadata(&to_file_path)?.len();
    println!("  → {} bytes ({})", written, to_file_path.display());

    // ── 4. render.preview ────────────────────────────────────────────
    step(4, 9, "render.preview — paginated HTML preview");
    let preview = client.render.preview(input()).await?;
    println!(
        "  → {} pages, environment={:?}, html.len={}",
        preview.total_pages,
        preview.environment,
        preview.html.len(),
    );

    // ── 5. render.document ───────────────────────────────────────────
    step(5, 9, "render.document — stored document descriptor");
    let descriptor = client.render.document(input()).await?;
    println!(
        "  → document_id={} pages={} size={}b",
        descriptor.document_id, descriptor.page_count, descriptor.size_bytes,
    );

    // ── 6. documents.get ─────────────────────────────────────────────
    step(6, 9, "documents.get — refresh descriptor + presigned URL");
    let fresh = client.documents.get(&descriptor.document_id).await?;
    println!("  → fresh presigned URL (expires {})", fresh.expires_at,);

    // ── 7. documents.thumbnails ─────────────────────────────────────
    step(7, 9, "documents.thumbnails — generate PNG thumbnails");
    let thumbs = client
        .documents
        .thumbnails(
            &descriptor.document_id,
            ThumbnailOptions {
                width: 320,
                format: Some(ThumbnailFormat::Png),
                ..ThumbnailOptions::new(320)
            },
        )
        .await?;
    println!("  → {} thumbnail(s)", thumbs.len());
    for thumb in &thumbs {
        println!(
            "    · page {} — {}×{} {}",
            thumb.page, thumb.width, thumb.height, thumb.content_type,
        );
    }

    // ── 8. documents.preview ────────────────────────────────────────
    step(8, 9, "documents.preview — stored paginated HTML");
    let stored_preview = client.documents.preview(&descriptor.document_id).await?;
    println!(
        "  → {} pages, html.len={}",
        stored_preview.page_count,
        stored_preview.html.len(),
    );

    // ── 9. documents.delete ─────────────────────────────────────────
    step(9, 9, "documents.delete — soft-delete the stored document");
    client.documents.delete(&descriptor.document_id).await?;
    println!("  → ok (re-delete now returns Error::Gone)");

    // ── Error-path demo ──────────────────────────────────────────────
    println!();
    println!("─── Error path: building a client with a bad API key ───");
    let bad_client = PoliPage::builder()
        .api_key("pp_test_invalid_key_for_demo")
        .base_url(
            std::env::var("POLI_PAGE_BASE_URL")
                .unwrap_or_else(|_| "https://api-develop.poli.page".to_string()),
        )
        .build()?;
    match bad_client.render.preview(input()).await {
        Ok(_) => println!("(unexpected: bad key worked)"),
        Err(err) => {
            println!("  code       = {}", err.code());
            println!("  status     = {:?}", err.status());
            println!("  request_id = {:?}", err.request_id());
            println!("  is_auth    = {}", err.is_auth_error());
            println!("  is_retry   = {}", err.is_retryable());
            // Pattern-match: spec §3.3 shape.
            match &err {
                Error::Auth { code, .. } | Error::PermissionDenied { code, .. } => {
                    println!("  matched Auth/PermissionDenied with code {code}");
                }
                other => {
                    println!("  matched other variant: {other:?}");
                }
            }
        }
    }

    println!();
    println!("Done. Output files in: {}", out_dir.display());
    Ok(())
}

fn step(n: u32, total: u32, name: &str) {
    println!();
    println!("[{n}/{total}] {name}");
}

/// API key resolution: env var → `.env` at workspace root → interactive prompt.
fn resolve_api_key() -> Result<String, Box<dyn std::error::Error>> {
    if let Ok(key) = std::env::var("POLI_PAGE_API_KEY") {
        if !key.is_empty() {
            return Ok(key);
        }
    }
    let env_path = workspace_env_path();
    if let Some(key) = read_env_file(&env_path)? {
        return Ok(key);
    }
    let key = prompt_for_api_key()?;
    append_to_env_file(&env_path, &key)?;
    Ok(key)
}

fn workspace_env_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(".env")
}

/// Minimal `.env` parser — looks for a `POLI_PAGE_API_KEY=...` line, strips
/// surrounding whitespace and surrounding quotes. No interpolation, no
/// multiline, no escape sequences (this is a demo helper, not dotenvy).
fn read_env_file(path: &Path) -> Result<Option<String>, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(None);
    }
    let file = fs::File::open(path)?;
    for line in BufReader::new(file).lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("POLI_PAGE_API_KEY=") {
            let value = rest.trim().trim_matches('"').trim_matches('\'').to_string();
            if !value.is_empty() {
                return Ok(Some(value));
            }
        }
    }
    Ok(None)
}

fn prompt_for_api_key() -> Result<String, Box<dyn std::error::Error>> {
    println!();
    println!("No POLI_PAGE_API_KEY found in the environment or .env file.");
    println!();
    println!("You can find your API key in the Poli Page dashboard:");
    println!("  https://poli.page/settings/api-keys");
    println!();
    print!("Paste your API key (or press Ctrl+C to exit): ");
    std::io::stdout().flush()?;
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf)?;
    let key = buf.trim().to_string();
    if key.is_empty() {
        return Err("no api key provided".into());
    }
    Ok(key)
}

fn append_to_env_file(path: &Path, key: &str) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "POLI_PAGE_API_KEY={key}")?;
    println!("→ Saved to {} for future runs.", path.display());
    Ok(())
}
