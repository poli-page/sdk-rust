# Changelog

All notable changes to `poli-page` (Rust) are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Breaking changes and their upgrade path are summarized in [MIGRATION.md](MIGRATION.md).

## [Unreleased]

Conformance with the Poli Page API contract v1.2.0
([poli-page/api-contract](https://github.com/poli-page/api-contract)). See
[MIGRATION.md](MIGRATION.md#from-0100-to-the-next-release-unreleased).

### Changed

- **BREAKING**: `QUOTA_EXCEEDED` and `OVERAGE_CAP_EXCEEDED` (429) and
  `STORAGE_ACCESS_DENIED` (502) are no longer retried, even with a
  `Retry-After` header, and `Error::is_retryable()` now returns `false` for
  them (it returned `true` for any 429 or 5xx). They last until the monthly
  reset or until the bucket access is restored, so the SDK used to wait up to
  30 s per retry for nothing. `RATE_LIMIT_EXCEEDED` and every other 429 and
  5xx are still retried. The retry loop and `is_retryable()` now share one rule.
- **BREAKING**: `DOWNLOAD_FAILED` (`Error::Download`) is never retryable:
  `is_retryable()` returns `false` whatever the storage status, 429 and 5xx
  included (it returned `true` for a storage 5xx or 429). The SDK never
  retried the download; the predicate now says so.
- **BREAKING**: the predicates read the status, as the SDK specification
  says: `is_auth_error()` is status 401 or 403, `is_rate_limit_error()` 429,
  `is_validation_error()` 400 only. A `DOWNLOAD_FAILED` refused by the storage
  with 403 is now an auth error, and an `Error::Api` with status 429 or 400
  now answers `true` to the matching predicate. An `Error::BadRequest` with
  status 422 is no longer a validation error (`is_validation_error()` was
  `true` for any `BadRequest`).
- **BREAKING**: SDK error messages are the exact texts of the SDK
  specification, and the `Display` of the SDK-side variants is that text:
  - `invalid_options` for a missing or empty API key: `apiKey is required`
    (was `api_key is required`, displayed `invalid options: api_key is required`).
  - `network_error`: `Network request failed` (was the reqwest error text,
    displayed `network error: …`); the reqwest error is still the `source()`.
  - `timeout`: `Request timed out after <ms>ms`, the attempt's timeout in
    whole milliseconds (was `request timed out after 60s`); `to_payload()`
    said `request timed out`.
  - `aborted`: `Request was aborted` (was `request was aborted`).
  - `DOWNLOAD_FAILED`: `Failed to download PDF: HTTP <status>` (was
    `Failed to download PDF: <status> <reason>`, displayed
    `download failed: …`), and `Failed to download PDF: network error` when
    the storage does not answer or the body breaks off (was the reqwest error
    text, now the `source()`). A body that breaks off has no status any more.
  - A non-JSON error body: `HTTP <status>` (was
    `HTTP <status>: response body was not valid JSON`); the code is still
    `INTERNAL_ERROR` with the real status.
- **BREAKING**: the API-status variants (`BadRequest`, `Auth`,
  `PermissionDenied`, `NotFound`, `Gone`, `RateLimited`, `Api`) have a new
  `source: Option<Box<dyn std::error::Error + Send + Sync>>` field: the error
  that broke the answer's body while it was read. Code that builds these
  variants adds `source: None`; a pattern without `..` adds the field.
- **BREAKING**: `Retry-After` is read as integer seconds (digits only) or an
  HTTP-date. Any other value (`-1`, `1.5`, `soon`) falls back to the backoff;
  `-1` used to retry at once. An integer too large for `i64` clamps to the
  30 s cap (it fell back to the backoff). `Retry-After-Ms` is ignored, as
  before.
- The per-attempt `timeout` now bounds the whole attempt: the connection, the
  request, the answer and its whole body, with an injected `reqwest::Client`
  too. The body was read outside the timeout, so a slow-dripping answer could
  outlive it.
- A 2xx answer cut off mid-body is `network_error`, and one that outlasts the
  timeout is `timeout`: both are retried like any network failure (a retried
  render can then store a second document, as with any retry after the
  request reached the API). `on_response` fires once the body is read.
- A non-2xx answer whose body breaks off or outlasts the timeout while it is
  read is a malformed error body: `INTERNAL_ERROR`, `HTTP <status>`, the real
  status and request id, the read error as `source()`, retried by its status
  (a 503 is retried, a 400 is not). It was `network_error`, always retried.
- An error body whose `code`, `error`, `detail`, `title` or `message` is not a
  string ignores that field instead of being read as a non-JSON body; JSON
  that is not an object gives `unknown_error` and `HTTP <status>`.
- `to_payload()` uses `Error::message()` for its `message`; `status` and
  `requestId` serialize as `null` when unknown, as before.
- A retry wait is never shorter than its delay: if the timer wakes early, the
  SDK sleeps again for the rest.
- **BREAKING**: `PageFormat`, `Orientation` and `ThumbnailFormat` keep a value
  the SDK does not know: their catch-all is `Unknown(String)`, holding the
  wire value verbatim, and it serializes back unchanged (it was a
  payload-less `Unknown` that dropped the value, spec §10). The enums are no
  longer `Copy` (they are `Clone`, `Eq` and `Hash`); they gain `as_str()`,
  `Display` and `From<&str>` / `From<String>`, which map the known wire values
  to their variant.
- A retry with no delay (`Retry-After: 0`) still yields to the runtime once
  before the next attempt.
- The minimum `reqwest` is 0.12.28.

### Added

- `Error::message()`: the bare explanation (spec §8.1 `message`), without the
  prefix `Display` adds to API errors.
- Known error code `TEMPLATE_UNSAFE` (400: the template, a partial or an SVG
  asset holds an injection attempt; `detail` names it; not retried) in
  `error_codes` and the docs.
- `tests/contract.rs` runs every SDK case of the API contract against a local
  mock server, one test per case id; CI pins the contract by commit
  (`poli-page/api-contract` action) and Dependabot proposes each new version.
  Run it locally with
  `POLI_CONTRACT=<path to contract.json> cargo test --all-features --test contract`.

### Fixed

- A request is never resent behind the SDK's back. reqwest's default retry
  policy resent a request the server refused over HTTP/2 (`REFUSED_STREAM`, a
  graceful `GOAWAY`) on its own: `on_request` did not see that send,
  `max_retries` did not count it. Those were sends of a request the server
  said it never processed (RFC 9113 §8.7), so no render was stored twice,
  but they were sends the SDK neither counted nor reported. The SDK's own client
  turns that policy off, so the failure surfaces as `network_error` and goes
  through the SDK's retry. HTTP/2 is not one of the SDK's reqwest features,
  but Cargo turns it on when the application's own reqwest has it. A client
  injected with `http_client(...)` keeps its own policy: build it with
  `.retry(reqwest::retry::never())`. Over HTTP/1.1, hyper sends a request
  again only when none of it reached the reused connection, which is not a
  resend; a request lost after it was written comes back as `network_error`.
- Docs: the retries page and the README no longer claim the API deduplicates
  on `Idempotency-Key`. It does not: the key is stable across the retries of
  one call, and a render retried after a late 5xx, a `timeout` or a
  `network_error` can store a second document.

## [0.10.0] - 2026-10-27

Implements the Poli Page environments contract. Breaking, with no
deprecation window: the API has dropped template versions, the
authenticated preview and inline-HTML rendering.

### Removed

- **BREAKING**: `render.preview` (async and `blocking`), `PreviewResult` and the
  `/v1/render/preview` path. `documents.preview(id)`
  (`GET /v1/documents/:id/preview`) stays.
- **BREAKING**: `InlineModeInput` and the old `RenderInput` enum with its `From` impls.
- **BREAKING**: The `version` input field and `DocumentDescriptor::version`. A render
  uses the project's copy published in the API key's environment.
- **BREAKING**: The `Environment` enum.
- **BREAKING**: Error codes `VERSION_NOT_FOUND`, `INVALID_VERSION_FORMAT`,
  `VERSION_REQUIRED`, `INVALID_VERSION_FOR_KEY_ENV`,
  `MISSING_PROJECT_OR_TEMPLATE`, `GONE` (the API answers `410 DOCUMENT_GONE`)
  and `PAYMENT_REQUIRED` (payment grace is `403 ORGANIZATION_PAYMENT_GRACE`).

### Changed

- **BREAKING**: `ProjectModeInput` is renamed `RenderInput` and is the only render input
  type. The `POST /v1/render` body holds only `project`, `template`, `data`,
  `metadata`, `format`, `orientation` and `locale`; unset optional fields
  are omitted.
- **BREAKING**: `DocumentDescriptor::environment` is a `String`: any environment name the
  API returns (e.g. `staging`) is passed through unchanged.
- **BREAKING**: `documents.thumbnails(id, options)` takes `Option<ThumbnailOptions>`
  (async and `blocking`). Every option is optional: `ThumbnailOptions::width`
  is `Option<u32>`, `ThumbnailOptions` derives `Default`,
  `ThumbnailOptions::new()` takes no argument and `.width(n)` sets the
  width. With no option set the body is `{}` and the API defaults apply
  (width 400, JPEG; 0.9.0 documented PNG). The SDK does not check bounds;
  the API answers `400 VALIDATION_ERROR`.
- `documents.delete` is documented as idempotent: it resolves on `204`,
  including for an already-deleted document.
- Doc examples use `pp_sandbox_...` keys; a key is `pp_{env}_{32 hex}`.
- The integration test renders `getting-started`/`welcome` with no version
  (`POLI_PAGE_TEST_VERSION` is gone).
- README and docs site: the quick start renders `getting-started`/`welcome`
  with a sandbox key and no version; "Authentication & environments" has
  the key format and regex, the environment table, the sandbox properties
  (watermarked, not counted, 30-day expiry) and the `poli-watermark` note;
  the preview / inline-HTML sections are replaced by a link to the
  playground (https://play.poli.page); the errors pages list the current
  API codes; the install snippets use `poli-page = "0.10"`.
- Demo (`cargo run --example demo`): asks for a sandbox key at
  `https://app.poli.page/orgs/{YOUR_ORG}/keys`, checks a pasted key against
  the key-format regex only, renders `getting-started`/`welcome` with
  `{ "name": "SDK Demo" }`, and ends with the `not-found` step (template
  `does-not-exist`, `404 TEMPLATE_NOT_FOUND`), which prints the error's
  code, status, message and request id; any other error, or a success,
  fails the demo. An empty `POLI_PAGE_BASE_URL` counts as unset.
- Integration workflow: an unset `POLI_PAGE_TEST_BASE_URL` secret (which
  GitHub Actions passes as an empty string) is dropped, so the SDK's
  default base URL applies.

### Added

- **BREAKING**: `ThumbnailFormat::Webp`. `ThumbnailFormat` is not
  `#[non_exhaustive]`, so an exhaustive `match` on it needs a `Webp` arm.
- Known error codes `PUBLISH_IN_PROGRESS` (503, retried like any 5xx,
  honouring `Retry-After`), `ENVIRONMENT_MISMATCH`,
  `PROJECT_NOT_IN_ENVIRONMENT`, `TEMPLATE_NOT_FOUND`, `DOCUMENT_GONE`,
  `ENVIRONMENT_PROTECTED`, `ORGANIZATION_PAYMENT_GRACE`, `EMPTY_DOCUMENT`,
  `STORAGE_ACCESS_DENIED`, `ORGANIZATION_MIGRATING`, `PROJECT_NOT_FOUND`,
  `TEMPLATE_ERROR`, `THUMBNAILS_NOT_AVAILABLE`, `RATE_LIMIT_EXCEEDED`,
  `RENDER_BODY_TOO_LARGE`, `DOCUMENT_TOO_LARGE`, `RENDER_TIMEOUT` (31 API
  codes in all). Unknown codes still pass through verbatim.

## [0.9.0] - 2026-06-18

First published release of the Rust SDK on crates.io. Behaviour parity with
`@poli-page/sdk@1.0.0` (Node) is the explicit goal. Shipped pre-1.0 (`0.x`) so
the public API can settle against real downstream use before committing to a
stable `1.0.0`; until then, breaking changes may land in `0.(x+1).0` minor
bumps (per Cargo's 0.x semver rules).

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
