//! End-to-end runnable demo for the Poli Page Rust SDK.
//!
//! Method order mirrors the Node demo per spec §13 Phase 5:
//! `render.pdf` → `render.pdf_stream` → `render_to_file` →
//! `render.document` → `documents.get` → `documents.thumbnails` →
//! `documents.preview` → `documents.delete` → deliberately trigger
//! `TEMPLATE_NOT_FOUND` to exercise the typed-error path.
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

use poli_page::{Error, PoliPage, RenderInput, ThumbnailFormat, ThumbnailOptions};
use serde_json::json;

const DEFAULT_PROJECT: &str = "getting-started";
const DEFAULT_TEMPLATE: &str = "welcome";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = resolve_api_key()?;
    let mut builder = PoliPage::builder().api_key(api_key);
    if let Ok(base_url) = std::env::var("POLI_PAGE_BASE_URL") {
        builder = builder.base_url(base_url);
    }
    let client = builder.build()?;

    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("outputs");
    fs::create_dir_all(&out_dir)?;

    println!("Output directory: {}", out_dir.display());
    println!();

    let input = || RenderInput {
        project: DEFAULT_PROJECT.into(),
        template: DEFAULT_TEMPLATE.into(),
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

    // ── 4. render.document ───────────────────────────────────────────
    step(4, 9, "render.document — stored document descriptor");
    let descriptor = client.render.document(input()).await?;
    println!(
        "  → document_id={} pages={} size={}b",
        descriptor.document_id, descriptor.page_count, descriptor.size_bytes,
    );

    // ── 5. documents.get ─────────────────────────────────────────────
    step(5, 9, "documents.get — refresh descriptor + presigned URL");
    let fresh = client.documents.get(&descriptor.document_id).await?;
    println!("  → fresh presigned URL (expires {})", fresh.expires_at,);

    // ── 6. documents.thumbnails ─────────────────────────────────────
    // Tier-gated on the API side: Free tier returns 403
    // THUMBNAILS_NOT_AVAILABLE. Mirrors the Node SDK integration test
    // (tests/integration/documents.integration.test.ts:39-52).
    step(6, 9, "documents.thumbnails — generate PNG thumbnails");
    match client
        .documents
        .thumbnails(
            &descriptor.document_id,
            Some(ThumbnailOptions {
                format: Some(ThumbnailFormat::Png),
                ..ThumbnailOptions::new().width(320)
            }),
        )
        .await
    {
        Ok(thumbs) => {
            println!("  → {} thumbnail(s)", thumbs.len());
            for thumb in &thumbs {
                println!(
                    "    · page {} — {}×{} {}",
                    thumb.page, thumb.width, thumb.height, thumb.content_type,
                );
            }
        }
        Err(Error::PermissionDenied { code, .. }) if code == "THUMBNAILS_NOT_AVAILABLE" => {
            println!("  → skipped — requires Starter+ tier (Free tier returns 403 THUMBNAILS_NOT_AVAILABLE)");
        }
        Err(err) => return Err(err.into()),
    }

    // ── 7. documents.preview ────────────────────────────────────────
    step(7, 9, "documents.preview — stored paginated HTML");
    let stored_preview = client.documents.preview(&descriptor.document_id).await?;
    println!(
        "  → {} pages, html.len={}",
        stored_preview.page_count,
        stored_preview.html.len(),
    );

    // ── 8. documents.delete ─────────────────────────────────────────
    step(8, 9, "documents.delete — soft-delete the stored document");
    client.documents.delete(&descriptor.document_id).await?;
    println!("  → ok (deletes are idempotent: a re-delete also returns Ok)");

    // ── 9. error handling ───────────────────────────────────────────
    // Deliberately ask for a template that does not exist to make the API
    // return 404 TEMPLATE_NOT_FOUND. The SDK is *expected* to return Err
    // here; the demo catches it and inspects the typed Error.
    step(
        9,
        9,
        "error handling — DEMO ONLY (we trigger an error on purpose)",
    );
    println!("  WARNING: This step is intentional — the SDK is about to return");
    println!("  an error, but the demo will catch and inspect it. The demo is NOT crashing.");
    println!("  (We ask for template \"does-not-exist\", expecting 404 TEMPLATE_NOT_FOUND.)");
    println!();
    let bad_input = RenderInput {
        project: DEFAULT_PROJECT.into(),
        template: "does-not-exist".into(),
        data: json!({}),
        ..Default::default()
    };
    match client.render.pdf(bad_input).await {
        Ok(_) => println!("  unexpected: the call succeeded but should have failed"),
        Err(err) => {
            println!("  caught — typed Error fields:");
            println!("    code       = {}", err.code());
            println!("    status     = {:?}", err.status());
            println!("    request_id = {:?}", err.request_id());
            println!("    is_auth    = {}", err.is_auth_error());
            println!("    is_retry   = {}", err.is_retryable());
            match &err {
                Error::NotFound { code, .. } => {
                    println!("    matched Error::NotFound with code {code}");
                }
                other => {
                    println!("    matched other variant: {other:?}");
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
