# Poli Page SDK for Rust

[![Crates.io](https://img.shields.io/crates/v/poli-page?style=flat&logo=rust&logoColor=ffffff&label=Crates.io)](https://crates.io/crates/poli-page)
[![Downloads](https://img.shields.io/crates/d/poli-page?style=flat&logo=rust&logoColor=ffffff&label=Downloads)](https://crates.io/crates/poli-page)
[![Ci](https://img.shields.io/github/actions/workflow/status/poli-page/sdk-rust/ci.yml?branch=main&style=flat&logo=githubactions&logoColor=ffffff&label=Ci)](https://github.com/poli-page/sdk-rust/actions/workflows/ci.yml)
[![Codeql](https://img.shields.io/github/actions/workflow/status/poli-page/sdk-rust/codeql.yml?branch=main&style=flat&logo=github&logoColor=ffffff&label=Codeql)](https://github.com/poli-page/sdk-rust/actions/workflows/codeql.yml)
[![Coverage](https://img.shields.io/codecov/c/github/poli-page/sdk-rust?style=flat&logo=codecov&logoColor=ffffff&label=Coverage)](https://codecov.io/gh/poli-page/sdk-rust)
[![Msrv](https://img.shields.io/badge/dynamic/toml?url=https%3A%2F%2Fraw.githubusercontent.com%2Fpoli-page%2Fsdk-rust%2Fmain%2FCargo.toml&query=%24.package.rust-version&prefix=v&style=flat&logo=rust&logoColor=ffffff&label=Msrv)](https://www.rust-lang.org/)
[![Lint](https://img.shields.io/badge/Lint-clippy-blue?style=flat&logo=rust&logoColor=ffffff)](https://github.com/rust-lang/rust-clippy)
[![Deps](https://img.shields.io/badge/Deps-up%20to%20date-brightgreen?style=flat&logo=rust&logoColor=ffffff)](https://deps.rs/repo/github/poli-page/sdk-rust)
[![Docs](https://img.shields.io/badge/Docs-online-brightgreen?style=flat&logo=readthedocs&logoColor=ffffff)](https://poli-page.github.io/sdk-rust/)
[![License](https://img.shields.io/github/license/poli-page/sdk-rust?style=flat&logo=gnu&logoColor=ffffff&label=License)](LICENSE-MIT)

Official Rust SDK for [Poli Page](https://poli.page) — render polished PDFs from HTML templates via the Poli Page API.

→ **Documentation**: **<https://poli-page.github.io/sdk-rust/>**
→ API reference on docs.rs: <https://docs.rs/poli-page>

## Install

```toml
[dependencies]
poli-page  = "0.10"
serde_json = "1"
tokio      = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Or via `cargo add`:

```bash
cargo add poli-page serde_json
cargo add tokio --features macros,rt-multi-thread
```

`cargo add` and a `"0.10"` requirement never pick a prerelease. To opt in
to one, pin it exactly, e.g. `cargo add poli-page@1.0.0-rc.1`.

Requires Rust 1.93 or later. See [MSRV policy](#msrv) below.

## Quick start

The SDK ships both an async client (default) and an optional blocking wrapper. Pick the one that matches your call site.

### Async

#### Render the welcome template

```rust,no_run
use poli_page::{PoliPage, RenderInput};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = PoliPage::new(std::env::var("POLI_PAGE_API_KEY")?)?;

    let pdf = client.render.pdf(RenderInput {
        project: "getting-started".into(),
        template: "welcome".into(),
        data: json!({ "name": "SDK Demo" }),
        ..Default::default()
    }).await?;
    // pdf is `bytes::Bytes`
    std::fs::write("welcome.pdf", &pdf)?;
    Ok(())
}
```

Run it with a sandbox key: `POLI_PAGE_API_KEY=pp_sandbox_x cargo run` (replace `pp_sandbox_x` with your key). Every organization gets a `getting-started` project with a `welcome` template, published into its `sandbox` environment when the organization is created, so the snippet runs as-is on a fresh organization. `getting-started` is a normal, editable project: if your organization renamed it, deleted it or removed it from `sandbox`, the API answers `404 PROJECT_NOT_FOUND` or `404 PROJECT_NOT_IN_ENVIRONMENT`. Use a fresh organization in that case.

A render uses the project's copy published in the API key's environment; there is no version to pick. For your own templates, publish the project to that environment, then swap the slugs.

To try a template or raw HTML without an API key, use the playground at [play.poli.page](https://play.poli.page).

#### Write a PDF to disk

```rust,no_run
use poli_page::{PoliPage, RenderInput};
use serde_json::json;

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let client = PoliPage::new(std::env::var("POLI_PAGE_API_KEY")?)?;
poli_page::render_to_file(
    &client,
    RenderInput {
        project: "getting-started".into(),
        template: "welcome".into(),
        data: json!({ "name": "SDK Demo" }),
        ..Default::default()
    },
    "./welcome.pdf",
).await?;
# Ok(()) }
```

`render_to_file` streams response bytes directly to disk (bounded memory). It creates parent directories if missing and overwrites existing files.

#### Stream — for large PDFs or piping to S3 / HTTP responses

```rust,no_run
use poli_page::{PoliPage, RenderInput};
use serde_json::json;
use std::future::poll_fn;
use std::pin::Pin;
use futures_core::Stream;

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let client = PoliPage::new(std::env::var("POLI_PAGE_API_KEY")?)?;
let mut stream = std::pin::pin!(client.render.pdf_stream(RenderInput {
    project: "billing".into(),
    template: "invoice".into(),
    data: json!({ "invoiceNumber": "INV-001" }),
    ..Default::default()
}).await?);

// stream implements `futures_core::Stream<Item = Result<bytes::Bytes, Error>>`
while let Some(chunk) = poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
    let chunk = chunk?;
    // forward to your sink — S3 upload, HTTP body, etc.
    # let _ = chunk;
}
# Ok(()) }
```

(Or `cargo add futures-util` and call `stream.next().await` if you'd rather not hand-roll `poll_fn`.)

#### Try it locally — runnable demo

```bash
cargo run --example demo
```

The demo exercises every public method end-to-end against the API. It reads `POLI_PAGE_API_KEY` and the optional `POLI_PAGE_BASE_URL` from the environment, and the key from `./.env`. With no key, it asks for a sandbox key (`pp_sandbox_…`), checks its format and saves it to `.env`, so later runs are silent. It renders `getting-started`/`welcome` and writes its output to `examples/outputs/` (git-ignored), in nine steps:

1. `render.pdf`
2. `render.pdf_stream`
3. `render_to_file`
4. `render.document`
5. `documents.get`
6. `documents.thumbnails` (the demo carries on if the organization's plan does not include thumbnails: the API answers `403 THUMBNAILS_NOT_AVAILABLE`)
7. `documents.preview`
8. `documents.delete`
9. `not-found`: renders template `does-not-exist` with empty data. The API answers `404 TEMPLATE_NOT_FOUND`, and the demo prints the error's code, status, message and request id. Any other error, or a success, fails the demo.

### Blocking

For sync callers (short scripts, CLI tools, callers from a non-async host), opt into the `blocking` feature:

```toml
[dependencies]
poli-page = { version = "0.10", features = ["blocking"] }
```

```rust,no_run
use poli_page::{RenderInput, blocking::PoliPage};
use serde_json::json;

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let client = PoliPage::new(std::env::var("POLI_PAGE_API_KEY")?)?;
let pdf = client.render.pdf(RenderInput {
    project: "getting-started".into(),
    template: "welcome".into(),
    data: json!({ "name": "SDK Demo" }),
    ..Default::default()
})?;
std::fs::write("welcome.pdf", &pdf)?;
# Ok(()) }
```

The blocking client owns a single-threaded tokio runtime and forwards each method through `runtime.block_on(...)` — same code path as the async surface, no protocol differences.

## Working with stored documents

Every render produces a stored document, accessible via `document_id` for later download or thumbnails. `render.pdf` and `render.pdf_stream` are conveniences that chain a presigned-URL fetch internally to return bytes; `render.document` returns just the descriptor (skip the auto-download when you'll fetch the bytes later).

```rust,no_run
use poli_page::{PoliPage, RenderInput, RenderMetadata, ThumbnailOptions};
use serde_json::json;

# async fn run() -> Result<(), Box<dyn std::error::Error>> {
let client = PoliPage::new(std::env::var("POLI_PAGE_API_KEY")?)?;
// 1. Render and store
let mut metadata = RenderMetadata::new();
metadata.insert("customerId".into(), "cust_123".into());
let doc = client.render.document(RenderInput {
    project: "billing".into(),
    template: "invoice".into(),
    data: json!({ "invoiceNumber": "INV-001" }),
    metadata: Some(metadata),
    ..Default::default()
}).await?;
// doc.document_id, doc.environment, doc.page_count, doc.size_bytes, doc.presigned_pdf_url, doc.metadata, …

// 2. Later, fetch a fresh presigned URL + download
let fresh = client.documents.get(&doc.document_id).await?;
let pdf = fresh.download_pdf().await?;

// 3. Generate thumbnails (`None` uses the API defaults: width 400, JPEG)
let thumbs = client
    .documents
    .thumbnails(&doc.document_id, Some(ThumbnailOptions::new().width(320)))
    .await?;

// 4. When done, soft-delete (deleting it again also succeeds)
client.documents.delete(&doc.document_id).await?;
# let _ = (pdf, thumbs); Ok(()) }
```

The presigned URL has a 15-minute TTL. If `download_pdf()` fails with `Error::Download { status: Some(403), .. }`, call `documents.get(id)` to refresh and retry. A document rendered in `sandbox` expires 30 days after creation.

## Authentication & environments

An API key belongs to one environment of your organization, and its text names that environment: `pp_{env}_` followed by 32 lowercase hex characters. Environment names are 1-20 characters: lowercase letters, digits and single inner hyphens, starting with a letter. A full key matches:

```text
^pp_(?=[a-z][a-z0-9-]{0,19}_)[a-z][a-z0-9]*(?:-[a-z0-9]+)*_[0-9a-f]{32}$
```

| Environment                           | Key            | Watermark | Counted |
| ------------------------------------- | -------------- | --------- | ------- |
| `sandbox` (every org)                 | `pp_sandbox_…` | yes       | no      |
| `live` (every org, deletable on Pro+) | `pp_live_…`    | no        | yes     |
| created on Pro+ (e.g. `staging`)      | `pp_staging_…` | no        | yes     |

"Counted" means the render counts toward the organization's monthly PDF quota. `sandbox` documents are watermarked, are not counted and expire 30 days after creation. There is no watermark option: a template that wants a watermark outside `sandbox` uses the `poli-watermark` class.

Standard and service keys share this one format. A render uses the project's copy published in the key's environment, and `DocumentDescriptor::environment` reports the environment a document was rendered in. Every key uses the same endpoint, `https://api.poli.page`. The SDK sends the key as a Bearer token and never inspects or validates it.

## Methods

| Method | Returns | Description |
|---|---|---|
| `client.render.pdf(input)` | `Result<Bytes, Error>` | Render a PDF, return bytes |
| `client.render.pdf_stream(input)` | `Result<impl Stream<Item = Result<Bytes, Error>>, Error>` | Render and stream the response |
| `client.render.document(input)` | `Result<DocumentDescriptor, Error>` | Render and return descriptor (skip auto-download) |
| `client.documents.get(id)` | `Result<DocumentDescriptor, Error>` | Retrieve a stored document |
| `client.documents.preview(id)` | `Result<DocumentPreviewResult, Error>` | Stored document's paginated HTML |
| `client.documents.thumbnails(id, options)` | `Result<Vec<Thumbnail>, Error>` | Page thumbnails (JPEG/WebP/PNG, base64); `options` is `Option<ThumbnailOptions>` |
| `client.documents.delete(id)` | `Result<(), Error>` | Soft-delete a stored document (a re-delete also succeeds) |
| `poli_page::render_to_file(client, input, path)` | `Result<(), Error>` | Render and stream to disk |
| `DocumentDescriptor::download_pdf()` | `Result<Bytes, Error>` | Fetch PDF bytes from the descriptor's presigned URL |

All methods are mirrored on `blocking::PoliPage` / `blocking::Render` / `blocking::Documents` under the `blocking` feature (sync signatures — no `.await`).

## Configuration

Use the builder for non-default options:

```rust,no_run
use std::time::Duration;
use poli_page::PoliPage;

# fn run() -> Result<(), poli_page::Error> {
let client = PoliPage::builder()
    .api_key(std::env::var("POLI_PAGE_API_KEY").unwrap())
    .max_retries(3)
    .retry_delay(Duration::from_millis(500))
    .timeout(Duration::from_secs(60))
    .build()?;
# let _ = client; Ok(()) }
```

| Option | Type | Default | Description |
|---|---|---|---|
| `api_key` | `String` | (required) | API key; it names its environment (`pp_{env}_…`) |
| `base_url` | `String` | `https://api.poli.page` | API base URL |
| `max_retries` | `u32` | `2` | Max retry attempts on retryable errors |
| `retry_delay` | `Duration` | 500 ms | Base delay before the first retry |
| `timeout` | `Duration` | 60 s | Per-attempt timeout |
| `on_retry` | `Fn(&RetryEvent) + Send + Sync` | — | Called before each retry sleep |
| `on_error` | `Fn(&Error) + Send + Sync` | — | Called once per terminal failure |

## Error handling

The SDK returns a single `Error` enum for every failure (API errors, network failures, timeouts, builder validation):

```rust,no_run
use poli_page::{Error, PoliPage, RenderInput};
# async fn run(client: PoliPage, input: RenderInput) -> Result<(), Error> {
match client.render.pdf(input).await {
    Ok(pdf) => { /* … */ }
    Err(err) if err.is_auth_error()       => { /* refresh credentials */ }
    Err(err) if err.is_rate_limit_error() => { /* queue for later */ }
    Err(err) if err.is_validation_error() => { eprintln!("bad input: {err}"); }
    Err(err) if err.is_network_error()    => { eprintln!("network/timeout: {err}"); }
    Err(Error::NotFound { code, .. })     => { eprintln!("not found: {code}"); }
    Err(err)                              => { return Err(err); }
}
# Ok(()) }
```

Every API-error variant exposes `status()` (HTTP status), `code()` (the wire error code) and `request_id()` (the `X-Request-Id` header — invaluable for support). `to_payload()` returns all four, with the bare message, as one `ErrorPayload`.

For not-found, lifecycle and billing failures, match the wire code directly:

```rust,no_run
use poli_page::{Error, error_codes};

# fn handle(err: &Error) {
match err.code() {
    error_codes::PROJECT_NOT_FOUND
    | error_codes::PROJECT_NOT_IN_ENVIRONMENT => { /* the project is not published in this key's environment */ }
    error_codes::TEMPLATE_NOT_FOUND           => { /* show 404 */ }
    error_codes::ORGANIZATION_PAYMENT_GRACE   => { /* "Billing is past due." */ }
    error_codes::ORGANIZATION_CANCELLED       => { /* "Subscription cancelled." */ }
    error_codes::DOCUMENT_GONE                => { /* the document was deleted */ }
    _ => { /* fall through to predicate-based handling above */ }
}
# }
```

Unknown codes from a newer API pass through verbatim in `code()`.

## Cancellation

Rust async cancellation is **drop-based**: dropping the future cancels the in-flight request. Combine with `tokio::time::timeout` for an external deadline:

```rust,no_run
use std::time::Duration;
use poli_page::{PoliPage, RenderInput};
# async fn run(client: PoliPage, input: RenderInput) -> Result<(), Box<dyn std::error::Error>> {
let pdf = tokio::time::timeout(
    Duration::from_secs(10),
    client.render.pdf(input),
).await??; // outer `?` for Elapsed, inner `?` for the SDK Error
# let _ = pdf; Ok(()) }
```

Per-attempt timeout is set via the builder's `timeout(Duration)`. Mid-retry sleeps are also cancellable for free — dropping the future stops the `tokio::time::sleep`.

## Observability

The SDK emits structured `tracing` spans and events out of the box. Add a subscriber to your binary:

```rust,no_run
tracing_subscriber::fmt::init();
```

Each HTTP attempt creates a `polipage.request` span with `method`, `url`, `attempt`, and `request_id` (recorded once the response arrives, on success or error). The `request_id` field appears on both happy-path and error spans — log correlation works on the 99% case, not just on failures.

Two SDK-level hooks complement tracing:

- **`on_retry(|&RetryEvent|)`** — fires before each retry sleep with the attempt number, sleep duration, and triggering error.
- **`on_error(|&Error|)`** — fires once per terminal failure (retries exhausted, non-retryable error).

Hooks are wrapped in `catch_unwind` — a panicking callback never breaks the request.

## Retries & idempotency

The SDK retries on **5xx** (including `503 PUBLISH_IN_PROGRESS` while a publish holds the project), **429**, **network errors**, and **timeouts**. Backoff is exponential (`retry_delay × 2^attempt`) with jitter in `[0.5, 1.5)`, capped by `Retry-After` when the server provides it (max 30 s). 4xx errors other than 429 are never retried.

Every POST sends an auto-generated `Idempotency-Key` (UUID v4) — safe to retry across network blips without producing duplicate documents. Override per call via `RenderInput::idempotency_key`.

The second-hop presigned-URL fetch (for `render.pdf` / `render.pdf_stream` / `DocumentDescriptor::download_pdf`) is **single-attempt** and **unauthenticated** — it carries its own signature and doesn't participate in the SDK's retry policy.

## Type system

`RenderInput` is a struct and the only input of `render.pdf`, `render.pdf_stream`, `render.document` and `render_to_file`:

```rust,ignore
pub struct RenderInput {
    pub project: String,
    pub template: String,
    pub data: serde_json::Value,
    pub format: Option<PageFormat>,
    pub orientation: Option<Orientation>,
    pub locale: Option<String>,
    pub metadata: Option<RenderMetadata>,
    pub idempotency_key: Option<String>, // sent as the Idempotency-Key header
    pub timeout: Option<Duration>,       // per-call timeout, not sent
}
```

`project`, `template` and `data` are required by the API; the optional fields are left out of the request when `None`. `RenderInput::default()` sets `data` to JSON `null`, so always set it: `json!({})` is sent as `{}`.

Wire fields that are `string | null` in the API map to `Option<String>` in Rust. `DocumentDescriptor::environment` is a plain `String`: any environment name the API returns. Forward-compatible enums (`PageFormat`, `Orientation`, `ThumbnailFormat`) carry a `#[serde(other)]` catch-all `Unknown` variant so a server-side addition is a silent no-op for old SDK versions.

## Concurrency & thread-safety

The client is `Send + Sync + Clone`. Share it across tasks by cloning — clones share the underlying connection pool — or wrap it in `Arc` if you prefer. Concurrent calls to `render` are independent; there is no per-request mutable state on the client itself.

## Runtime support

- **Server-side only.** API keys are secrets, whatever their environment, and must never ship to a browser. Call the SDK from your backend and proxy results to the client.
- **Async-first** on the tokio runtime (default).
- **Sync wrapper** via the `blocking` Cargo feature.
- **TLS backend** defaults to `rustls` (pure Rust). Switch to system OpenSSL with `features = ["native-tls"]` if your environment needs it.

## MSRV

Current MSRV: **Rust 1.93**.

We track *current stable minus 2 minor releases* — the same cadence `tokio`, `reqwest`, and `serde` document. With Rust on a 6-week release cycle, that's a ~3-month window. MSRV bumps are MINOR releases with a clear note in [MIGRATION.md](MIGRATION.md).

## Documentation & support

- Platform docs: [docs.poli.page](https://docs.poli.page)
- crate rustdoc: [docs.rs/poli-page](https://docs.rs/poli-page)
- Sign up & generate API keys: [app.poli.page](https://app.poli.page)
- Issues: [github.com/poli-page/sdk-rust/issues](https://github.com/poli-page/sdk-rust/issues)

## License

Dual-licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. This is the Rust-ecosystem convention; pick whichever fits your project's compatibility requirements.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual-licensed as above, without any additional terms or conditions.
