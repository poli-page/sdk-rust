# Changelog

All notable changes to `poli-page` (Rust) are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Breaking changes between major versions are summarized in [MIGRATION.md](MIGRATION.md).

## [Unreleased]

### Added

- `PoliPageBuilder::http_client(reqwest::Client)` (and the same on
  `blocking::PoliPageBuilder`) — inject a pre-built `reqwest::Client` to
  share a connection pool with the rest of your application or layer
  custom TLS / proxies / middleware at the `reqwest` level. The SDK's
  own `timeout` still applies; consumers must depend on a compatible
  `reqwest` 0.12.x.
- Nightly integration CI: `.github/workflows/integration.yml` runs the
  `--features integration --ignored` suite against the live API on a
  06:00 UTC cron + push-to-main, scoped to the upstream repo so
  fork PRs don't require the `POLI_PAGE_API_KEY` secret. Resolves the
  prior doc/CI mismatch with `CONTRIBUTING.md`.

### Changed

- `#[must_use]` added to the `PoliPage`, `Render`, and `Documents` client
  handle types (both async and `blocking` flavours), to `PoliPage::builder`,
  `ThumbnailOptions::new`, and to every `Error` accessor / predicate
  (`is_auth_error`, `is_rate_limit_error`, `is_validation_error`,
  `is_network_error`, `is_retryable`, `status`, `code`, `request_id`).
  Compile-time nudge only; no behaviour change.
- Lints tightened: `#![warn(clippy::pedantic, clippy::cargo)]` enabled at
  the crate root with a curated allow-list (`module_name_repetitions`,
  `missing_errors_doc`, `multiple_crate_versions`, the cast group,
  `duration_suboptimal_units`, `doc_markdown`). Caught a handful of
  cleanups in `render.rs`, `documents.rs`, and `tests/`.
- `examples/demo.rs` now writes output PDFs and HTML to `examples/outputs/`
  (git-ignored, excluded from the published crate) instead of the OS temp
  directory — easier to inspect across runs.
- `examples/demo.rs` step 7 (`documents.thumbnails`) tolerates Free-tier keys:
  on `403 THUMBNAILS_NOT_AVAILABLE` it prints a skip notice and continues to
  steps 8–9 instead of aborting the round-trip. Mirrors the Node SDK
  integration test pattern at `tests/integration/documents.integration.test.ts`.

## [1.0.0-rc.1] - 2026-05-24

Release-candidate cut for v1.0. Behaviour parity with `@poli-page/sdk@1.0.0`
(Node) is the explicit goal; the public Rust API is finalised pending RC
validation against the deployed develop API and downstream consumption.

### Added

- **Async-first client** `PoliPage` with `render` and `documents`
  sub-namespaces reachable as public fields. Cheap to clone — internally an
  `Arc<ClientInner>`.
- **`render` namespace**:
  - `render.pdf(input) -> bytes::Bytes` — two-hop: POST `/v1/render` then
    GET the descriptor's `presigned_pdf_url`.
  - `render.pdf_stream(input) -> impl Stream<Item = Result<Bytes, Error>>`
    — same two hops, streaming second hop chunk-by-chunk.
  - `render.preview(input)` — accepts both project-mode and inline-mode
    input via `impl Into<RenderInput>`. Returns `PreviewResult { html,
    total_pages, environment }`.
  - `render.document(input) -> DocumentDescriptor` — POST `/v1/render`
    only, no auto-download. Descriptor carries a back-reference to the
    parent client so `download_pdf()` works fluently.
- **`documents` namespace**:
  - `documents.get(id) -> DocumentDescriptor`
  - `documents.preview(id) -> DocumentPreviewResult { html, page_count }`
    — assembled from a `text/html` response body plus the
    `X-Document-Page-Count` header (NaN-tolerant: defaults to `0`).
  - `documents.thumbnails(id, opts) -> Vec<Thumbnail>` — body wrapped as
    `{ thumbnails: opts }`, response unwrapped from `{ thumbnails: [...] }`.
  - `documents.delete(id) -> ()`
  - All four URL-encode the `:id` path segment (JS `encodeURIComponent`
    parity, including `/` → `%2F`).
- **Native sum type for compile-time mode enforcement**:
  `pub enum RenderInput { Project(ProjectModeInput), Inline(InlineModeInput) }`
  with `From` impls for both. `render.pdf(InlineModeInput { … })` is a
  compile error — inline mode flows only through `render.preview`.
- **`DocumentDescriptor::download_pdf()`** — fluent helper that fetches
  PDF bytes from the descriptor's `presigned_pdf_url`. The S3 second-hop
  is unauthenticated and not subject to the SDK's retry policy.
- **`poli_page::render_to_file(client, input, path)`** — async free
  function that creates parent dirs and streams chunks to disk in
  bounded memory.
- **Optional `blocking` Cargo feature** — `blocking::PoliPage` plus
  parallel `blocking::Render` / `blocking::Documents` / `blocking::render_to_file`.
  Each method wraps the async equivalent in `runtime.block_on(...)`
  against a single current-thread tokio runtime owned by the client.
  `blocking::Render::pdf_stream` returns a `std::io::Read` adapter
  (`BlockingPdfReader`).
- **Single `Error` enum** with `thiserror` derive — 13 variants split
  into reserved (`InvalidOptions`, `Connection`, `Timeout`, `Aborted`,
  `Download`, `Internal`) and per-API-status (`BadRequest`, `Auth`,
  `PermissionDenied`, `NotFound`, `Gone`, `RateLimited`, `Api`).
  `Error` derives `Clone` (the boxed `source` is dropped on clone — see
  `RetryEvent`).
- **Predicate helpers** on `Error`: `is_auth_error` (401 + 403),
  `is_rate_limit_error`, `is_validation_error`, `is_network_error`,
  `is_retryable`. Accessors: `status()`, `code()`, `request_id()`.
- **`poli_page::error_codes` module** — the full §7.4 set of API code
  constants (`MISSING_API_KEY`, `INVALID_API_KEY`, `PAYMENT_REQUIRED`,
  `FORBIDDEN`, `ORGANIZATION_CANCELLED`, `ORGANIZATION_PURGED`,
  `NOT_FOUND`, `VERSION_NOT_FOUND`, `DOCUMENT_NOT_FOUND`, `GONE`,
  `VALIDATION_ERROR`, `MISSING_DATA`, `MISSING_PROJECT_OR_TEMPLATE`,
  `MISSING_TEMPLATE_SLUG`, `PROJECT_REQUIRED_FOR_DOCUMENT`,
  `INVALID_VERSION_FORMAT`, `VERSION_REQUIRED`,
  `INVALID_VERSION_FOR_KEY_ENV`, `QUOTA_EXCEEDED`,
  `OVERAGE_CAP_EXCEEDED`, `INTERNAL_ERROR`).
- **Auto-generated `Idempotency-Key`** (UUID v4) on every POST; per-call
  override via `ProjectModeInput::idempotency_key`.
- **Retry policy** — 5xx, 429, network, timeout retried up to
  `max_retries` (default 2). Exponential backoff with jitter in
  `[0.5, 1.5)`; honours `Retry-After` (cap 30 s); past dates clamp to
  `Duration::ZERO`; unparseable values fall back to computed backoff.
  The second-hop presigned fetch is never retried.
- **Observability hooks** — `on_retry(|&RetryEvent|)` and
  `on_error(|&Error|)` registered via the builder. Wrapped in
  `catch_unwind` so a panicking hook never breaks the request.
- **`tracing` integration** — `polipage.request` span per attempt with
  `method`, `url`, `attempt`, and `request_id` (recorded once the
  response arrives, on success or error).
- **Per-attempt timeout** via `tokio::time::timeout`; drop-based
  cancellation works natively.
- **TLS backend choice**: pure-Rust `rustls` by default;
  opt into `native-tls` for system OpenSSL.
- **`docs.rs` feature badge** on the `blocking` module via
  `#[cfg_attr(docsrs, doc(cfg(feature = "blocking")))]`.
- **MSRV `1.93`** — `current stable - 2 minors` per §12.6. Enforced in CI
  with `cargo +1.93 build --locked --all-features`.
- **`#![forbid(unsafe_code)]`** at the crate root.
- **Doctests on every public method** — `cargo test --doc` is a CI gate.
- **Dual-licensed MIT OR Apache-2.0** — Rust-ecosystem convention.

### Notes

- This is the **first release of the Rust SDK**. There is no prior
  `0.x` line to migrate from. The Node SDK's
  [CHANGELOG](https://github.com/poli-page/sdk-node/blob/main/CHANGELOG.md)
  documents the underlying API contract evolution.
- Behaviour parity is verified by porting Node's `tests/internal/http.test.ts`
  (~30 unit tests) and the `renderPreview` / `renderPdf` / `renderDocument`
  / `renderPdfStream` / `documents.*` describe blocks (40+ wiremock tests).
- The `examples/demo.rs` produces a PDF byte-equivalent to the Node
  demo's `render.pdf` output when run against the same template.
