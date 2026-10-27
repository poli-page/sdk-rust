//! End-to-end runnable demo for the Poli Page Rust SDK.
//!
//! It reads `POLI_PAGE_API_KEY` (a `pp_sandbox_` key) and the optional
//! `POLI_PAGE_BASE_URL`, and asks for the key when neither the environment
//! nor `.env` has it. It renders the `welcome` template of the
//! `getting-started` project, which every organization gets published in its
//! `sandbox` environment, then runs nine steps and writes the results under
//! `examples/outputs/`:
//!
//! 1. `render.pdf`
//! 2. `render.pdf_stream`
//! 3. `render_to_file`
//! 4. `render.document`
//! 5. `documents.get`
//! 6. `documents.thumbnails`
//! 7. `documents.preview`
//! 8. `documents.delete`
//! 9. `not-found`: renders template `does-not-exist` and prints the error's
//!    code, status, message and request id (`404 TEMPLATE_NOT_FOUND`); any
//!    other error, or a success, fails the demo
//!
//! API key resolution:
//!   1. `POLI_PAGE_API_KEY` environment variable.
//!   2. `.env` file at the repo root (manually parsed — no `dotenvy` dep).
//!   3. Interactive `stdin` prompt; a pasted key that has the format of a
//!      Poli Page key is appended to `.env` so future runs are silent.
//!
//! Run with:
//! ```text
//! cargo run --example demo
//! ```

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use poli_page::{
    error_codes, Error, ErrorPayload, PoliPage, RenderInput, ThumbnailFormat, ThumbnailOptions,
};
use regex::Regex;
use serde_json::json;

const DEFAULT_PROJECT: &str = "getting-started";
const DEFAULT_TEMPLATE: &str = "welcome";
const TOTAL_STEPS: u32 = 9;

/// The contract's full-key regex
/// `^pp_(?=[a-z][a-z0-9-]{0,19}_)[a-z][a-z0-9]*(?:-[a-z0-9]+)*_[0-9a-f]{32}$`
/// without its lookahead, which the `regex` crate does not support.
/// [`is_api_key`] applies the lookahead's 20-character limit to the captured
/// environment name instead.
const API_KEY_PATTERN: &str = r"^pp_([a-z][a-z0-9]*(?:-[a-z0-9]+)*)_[0-9a-f]{32}$";
const MAX_ENVIRONMENT_LEN: usize = 20;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = resolve_api_key()?;
    let mut builder = PoliPage::builder().api_key(api_key);
    // An empty value counts as unset, so the SDK default applies.
    if let Some(base_url) = std::env::var("POLI_PAGE_BASE_URL")
        .ok()
        .filter(|v| !v.is_empty())
    {
        builder = builder.base_url(base_url);
    }
    let client = builder.build()?;

    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join("outputs");
    fs::create_dir_all(&out_dir)?;

    println!("Output directory: {}", out_dir.display());
    println!();

    // ── 1. render.pdf ────────────────────────────────────────────────
    step(1, "render.pdf — buffer the full PDF into bytes");
    let pdf = client.render.pdf(welcome_input()).await?;
    let pdf_path = out_dir.join("render-pdf.pdf");
    fs::write(&pdf_path, &pdf)?;
    println!("  → {} bytes ({})", pdf.len(), pdf_path.display());

    // ── 2. render.pdf_stream ──────────────────────────────────────────
    step(2, "render.pdf_stream — async stream of chunks");
    let mut stream = std::pin::pin!(client.render.pdf_stream(welcome_input()).await?);
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
    step(3, "render_to_file — bounded-memory disk write");
    let to_file_path = out_dir.join("render-to-file.pdf");
    poli_page::render_to_file(&client, welcome_input(), &to_file_path).await?;
    let written = fs::metadata(&to_file_path)?.len();
    println!("  → {} bytes ({})", written, to_file_path.display());

    // ── 4. render.document ───────────────────────────────────────────
    step(4, "render.document — stored document descriptor");
    let descriptor = client.render.document(welcome_input()).await?;
    println!(
        "  → document_id={} environment={} pages={} size={}b",
        descriptor.document_id,
        descriptor.environment,
        descriptor.page_count,
        descriptor.size_bytes,
    );

    // ── 5. documents.get ─────────────────────────────────────────────
    step(5, "documents.get — refresh descriptor + presigned URL");
    let fresh = client.documents.get(&descriptor.document_id).await?;
    println!("  → fresh presigned URL (expires {})", fresh.expires_at,);

    // ── 6. documents.thumbnails ─────────────────────────────────────
    // Plan-gated on the API side: a plan without thumbnails returns 403
    // THUMBNAILS_NOT_AVAILABLE, and the demo carries on.
    step(6, "documents.thumbnails — generate PNG thumbnails");
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
            println!("  → skipped — the organization's plan does not include thumbnails (403 THUMBNAILS_NOT_AVAILABLE)");
        }
        Err(err) => return Err(err.into()),
    }

    // ── 7. documents.preview ────────────────────────────────────────
    step(7, "documents.preview — stored paginated HTML");
    let stored_preview = client.documents.preview(&descriptor.document_id).await?;
    println!(
        "  → {} pages, html.len={}",
        stored_preview.page_count,
        stored_preview.html.len(),
    );

    // ── 8. documents.delete ─────────────────────────────────────────
    step(8, "documents.delete — soft-delete the stored document");
    client.documents.delete(&descriptor.document_id).await?;
    println!("  → ok (deletes are idempotent: a re-delete also returns Ok)");

    // ── 9. not-found ────────────────────────────────────────────────
    step(9, "not-found — render a template that does not exist");
    println!("  WARNING: this step is meant to fail. The demo catches the error");
    println!("  and prints it; it is NOT crashing.");
    println!();
    let payload = expect_template_not_found(client.render.pdf(not_found_input()).await)?;
    println!("  caught poli_page::Error:");
    println!("    code       = {}", payload.code);
    println!("    status     = {}", display_or_none(payload.status));
    println!("    message    = {}", payload.message);
    println!("    request_id = {}", display_or_none(payload.request_id));

    println!();
    println!("Done. Output files in: {}", out_dir.display());
    Ok(())
}

/// The demo's render input: the `welcome` template of `getting-started`, with
/// no version (a render uses the copy published in the key's environment).
fn welcome_input() -> RenderInput {
    RenderInput {
        project: DEFAULT_PROJECT.into(),
        template: DEFAULT_TEMPLATE.into(),
        data: json!({ "name": "SDK Demo" }),
        ..Default::default()
    }
}

/// A template that does not exist. `data` is an empty object, not
/// `RenderInput::default()`'s `null`, so the body carries `"data":{}`.
fn not_found_input() -> RenderInput {
    RenderInput {
        project: DEFAULT_PROJECT.into(),
        template: "does-not-exist".into(),
        data: json!({}),
        ..Default::default()
    }
}

/// The not-found step's check: only `404 TEMPLATE_NOT_FOUND` passes. Any
/// other error, or a render that succeeds, fails the demo.
fn expect_template_not_found<T>(
    result: Result<T, Error>,
) -> Result<ErrorPayload, Box<dyn std::error::Error>> {
    match result {
        Ok(_) => Err("unexpected: the not-found render succeeded".into()),
        Err(err) => {
            let payload = err.to_payload();
            if payload.status == Some(404) && payload.code == error_codes::TEMPLATE_NOT_FOUND {
                Ok(payload)
            } else {
                Err(err.into())
            }
        }
    }
}

/// Whether `key` has the format of a Poli Page API key. It does not check
/// which environment the key names.
fn is_api_key(key: &str) -> bool {
    let pattern = Regex::new(API_KEY_PATTERN).expect("API_KEY_PATTERN is a valid regex");
    pattern
        .captures(key)
        .is_some_and(|caps| caps[1].len() <= MAX_ENVIRONMENT_LEN)
}

fn display_or_none<T: std::fmt::Display>(value: Option<T>) -> String {
    value.map_or_else(|| "(none)".to_string(), |v| v.to_string())
}

fn step(n: u32, name: &str) {
    println!();
    println!("[{n}/{TOTAL_STEPS}] {name}");
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
    println!("The demo needs a sandbox API key (pp_sandbox_…). Sandbox renders are");
    println!("watermarked, do not count toward your monthly quota and expire after");
    println!("30 days. Create one in the Poli Page dashboard:");
    println!("  https://app.poli.page/orgs/{{YOUR_ORG}}/keys");
    println!();
    println!("The demo renders the getting-started project that every organization");
    println!("gets in its sandbox. If yours renamed, deleted or unpublished it, use a");
    println!("fresh organization.");
    println!();
    print!("Paste your sandbox API key (or press Ctrl+C to exit): ");
    std::io::stdout().flush()?;
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf)?;
    let key = buf.trim().to_string();
    if key.is_empty() {
        return Err("no api key provided".into());
    }
    if !is_api_key(&key) {
        return Err("that is not a Poli Page API key: expected pp_{environment}_ followed by 32 lowercase hex characters".into());
    }
    Ok(key)
}

fn append_to_env_file(path: &Path, key: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "POLI_PAGE_API_KEY={key}")?;
    println!("→ Saved to {} for future runs.", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const HEX32: &str = "0123456789abcdef0123456789abcdef";

    fn key(env: &str) -> String {
        format!("pp_{env}_{HEX32}")
    }

    #[test]
    fn is_api_key_accepts_any_environment_name() {
        for env in ["sandbox", "live", "staging", "eu-west-2", "qa", "test", "a"] {
            assert!(is_api_key(&key(env)), "{env} should pass");
        }
    }

    #[test]
    fn is_api_key_bounds_the_environment_name_at_20_characters() {
        assert!(is_api_key(&key(&"a".repeat(20))));
        assert!(!is_api_key(&key(&"a".repeat(21))));
        // The bound counts hyphens too, as the lookahead `[a-z0-9-]{0,19}` does.
        assert!(is_api_key(&key(&format!("ab{}", "-c".repeat(9)))));
        assert!(!is_api_key(&key(&format!("abc{}", "-d".repeat(9)))));
    }

    #[test]
    fn is_api_key_rejects_malformed_keys() {
        let rejected = [
            String::new(),
            "pp_sandbox_x".to_string(),
            format!("pp_sandbox_{}", HEX32.to_uppercase()),
            format!("pp_sandbox_{}", &HEX32[..31]),
            format!("pp_sandbox_{HEX32}0"),
            format!("pp_sandbox_{}g", &HEX32[..31]),
            key("Sandbox"),
            key("1sandbox"),
            key("-sandbox"),
            key("sandbox-"),
            key("sand--box"),
            key("sand_box"),
            key(""),
            format!("pk_sandbox_{HEX32}"),
            format!(" {}", key("sandbox")),
            format!("{}\n", key("sandbox")),
        ];
        for k in &rejected {
            assert!(!is_api_key(k), "{k:?} should be rejected");
        }
    }

    #[test]
    fn welcome_input_renders_getting_started_with_no_version() {
        assert_eq!(
            serde_json::to_value(welcome_input()).unwrap(),
            json!({
                "project": "getting-started",
                "template": "welcome",
                "data": { "name": "SDK Demo" },
            }),
        );
    }

    #[tokio::test]
    async fn not_found_step_sends_empty_data_object_and_surfaces_the_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/render"))
            .and(body_json(json!({
                "project": "getting-started",
                "template": "does-not-exist",
                "data": {},
            })))
            .respond_with(
                ResponseTemplate::new(404)
                    .insert_header("x-request-id", "req_not_found")
                    .set_body_json(json!({
                        "error": "TEMPLATE_NOT_FOUND",
                        "detail": "Template does-not-exist not found",
                    })),
            )
            .expect(1)
            .mount(&server)
            .await;
        let client = PoliPage::builder()
            .api_key(key("sandbox"))
            .base_url(server.uri())
            .build()
            .unwrap();

        let payload = expect_template_not_found(client.render.pdf(not_found_input()).await)
            .expect("404 TEMPLATE_NOT_FOUND passes the not-found step");

        assert_eq!(payload.code, "TEMPLATE_NOT_FOUND");
        assert_eq!(payload.status, Some(404));
        assert_eq!(payload.message, "Template does-not-exist not found");
        assert_eq!(payload.request_id.as_deref(), Some("req_not_found"));
    }

    #[test]
    fn not_found_step_fails_when_the_render_succeeds() {
        assert!(expect_template_not_found(Ok(())).is_err());
    }

    #[tokio::test]
    async fn not_found_step_fails_on_any_other_error() {
        for (status, code) in [
            (404, "PROJECT_NOT_FOUND"),
            (404, "PROJECT_NOT_IN_ENVIRONMENT"),
            (401, "INVALID_API_KEY"),
            (400, "TEMPLATE_NOT_FOUND"),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/v1/render"))
                .respond_with(
                    ResponseTemplate::new(status)
                        .set_body_json(json!({ "error": code, "detail": "nope" })),
                )
                .mount(&server)
                .await;
            let client = PoliPage::builder()
                .api_key(key("sandbox"))
                .base_url(server.uri())
                .max_retries(0)
                .build()
                .unwrap();

            let result = expect_template_not_found(client.render.pdf(not_found_input()).await);

            assert!(result.is_err(), "{status} {code} must fail the step");
        }
    }
}
