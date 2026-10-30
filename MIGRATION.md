# Migration Guide

This document records breaking changes between versions of `poli-page`
and the upgrade path for downstream code. Before `1.0.0`, a breaking
change bumps the minor version (`0.(x+1).0`), per Cargo's 0.x semver rules.

## From 0.10.0 to 0.11.0

The SDK now follows the retry policy and the error messages of the Poli Page
API contract.

### Three codes are no longer retried

`QUOTA_EXCEEDED` and `OVERAGE_CAP_EXCEEDED` (429) and `STORAGE_ACCESS_DENIED`
(502) are returned at once, and `is_retryable()` returns `false` for them. A
queue that requeues on `is_retryable()` now drops these errors: handle them by
code instead, for example hold quota errors until the monthly reset and alert
on `STORAGE_ACCESS_DENIED` until the bucket role is restored.
`is_rate_limit_error()` is still `true` for the two 429s.

```rust,ignore
// 0.10.0
if err.is_retryable() { // was true for QUOTA_EXCEEDED
    requeue(job);
}

// 0.11.0
match err.code() {
    error_codes::QUOTA_EXCEEDED | error_codes::OVERAGE_CAP_EXCEEDED => hold_until_monthly_reset(job),
    _ if err.is_retryable() => requeue(job),
    _ => fail(job, err),
}
```

`DOWNLOAD_FAILED` is never retryable either, whatever the storage status: re-mint
the URL with `documents.get(id)` and download again.

### Predicates

`is_auth_error()`, `is_rate_limit_error()` and `is_validation_error()` read the
status (401/403, 429, 400). A `DOWNLOAD_FAILED` refused with 403 is now an auth
error: the presigned URL expired or was refused. A 422 (`Error::BadRequest`) is
no longer a validation error: match `Error::BadRequest { .. }` if you need it.

### Messages and `Display`

If you match error messages, match `code()` instead: the SDK-side messages are
now the specification's exact texts (`apiKey is required`,
`Network request failed`, `Request timed out after <ms>ms`,
`Request was aborted`, `Failed to download PDF: HTTP <status>`,
`Failed to download PDF: network error`, `HTTP <status>` for a non-JSON error
body), and the `Display` of `InvalidOptions`, `Connection`, `Timeout`,
`Aborted` and `Download` is that text, with no prefix. `Error::message()` gives
the bare message of any error. The reqwest error behind a network or download
failure is the error's `source()`; `Error::Timeout` has no source.

### A `source` field on the API variants

`BadRequest`, `Auth`, `PermissionDenied`, `NotFound`, `Gone`, `RateLimited` and
`Api` carry `source: Option<Box<dyn std::error::Error + Send + Sync>>`, the
error that broke the answer's body while it was read. Add `source: None` where
you build one (in tests, for example), and `..` to a pattern that lists every
field.

### `PageFormat`, `Orientation` and `ThumbnailFormat`

`Unknown` now carries the value: `Unknown(String)`. Match it as
`PageFormat::Unknown(value)` (or `Unknown(_)`), and send it back as it came.
The enums are no longer `Copy`: clone where you copied (`d.format.clone()`),
or borrow. To build one from a string, use `PageFormat::from("A4")`, which
gives the known variant; `as_str()` returns the wire value.

```rust,ignore
// 0.10.0
if descriptor.format == PageFormat::Unknown { /* the value was lost */ }

// 0.11.0
if let PageFormat::Unknown(name) = &descriptor.format {
    log::info!("page format {name} is newer than this SDK");
}
```

### `Retry-After`

Only digits (seconds) and HTTP-dates are read, clamped to 30 s; any other value
falls back to the backoff (`-1` used to retry at once).

### Injected `reqwest::Client`

The SDK's `timeout` now bounds the whole attempt, body included, whatever
client you inject: drop any read timeout you set only to bound the body. Build
an injected client with `.retry(reqwest::retry::never())`, as the SDK does for
its own: reqwest's default policy resends some refused HTTP/2 requests, which
the SDK cannot count and the API does not deduplicate.

### Bodies that break off

A 2xx answer cut off while its body is read is now `network_error` (or
`timeout`), retried by the SDK, instead of a non-retried error. A non-2xx answer
whose body breaks off is `INTERNAL_ERROR` with its status and `HTTP <status>`,
retried by that status, instead of `network_error`.

## From 0.9.x to 0.10.0

The API moved from template versions to environments. An API key belongs
to one environment and a render uses the project's copy published in that
environment, so there is no version to pick. The authenticated preview and
inline-HTML rendering are gone from the API. Everything ships at once, with
no deprecation window.

### API keys

A key is `pp_{env}_` followed by 32 lowercase hex characters, where `{env}`
names the key's environment (`pp_sandbox_…`, `pp_live_…`, `pp_staging_…`).
The `pp_test_` and `pp_sa_` prefixes are gone; standard and service keys
share the one format. Replace a `pp_test_` key with a `pp_sandbox_` key.
The SDK still sends the key as-is and never checks it. A full key matches:

```text
^pp_(?=[a-z][a-z0-9-]{0,19}_)[a-z][a-z0-9]*(?:-[a-z0-9]+)*_[0-9a-f]{32}$
```

### Types and methods

| 0.9.x | 0.10.0 |
|---|---|
| `ProjectModeInput { project, template, version, data, .. }` | `RenderInput { project, template, data, .. }`; drop `version` |
| `RenderInput` enum (`Project` / `Inline`) and its `From` impls | `RenderInput` is the struct above |
| `InlineModeInput` | Removed. Try raw HTML in the playground, <https://play.poli.page> |
| `client.render.preview(input)` → `PreviewResult` | Removed (async and `blocking`). Use the playground for a template preview; `documents.preview(id)` stays for a stored document |
| `DocumentDescriptor::version` | Removed |
| `DocumentDescriptor::environment: Environment` | `String`: any environment name, e.g. `"staging"` |
| `Environment` enum | Removed; compare the string |
| `ThumbnailOptions::new(width)` | `ThumbnailOptions::new().width(width)`; every option is optional and `ThumbnailOptions` derives `Default` |
| `documents.thumbnails(id, ThumbnailOptions)` | `documents.thumbnails(id, Option<ThumbnailOptions>)`; `None` uses the API defaults (width 400, JPEG) |
| — | `ThumbnailFormat::Webp`. The enum is not `#[non_exhaustive]`: an exhaustive `match` on `ThumbnailFormat` needs a `Webp` arm |

Before:

```rust,ignore
use poli_page::ProjectModeInput;

let pdf = client.render.pdf(ProjectModeInput {
    project: "getting-started".into(),
    template: "welcome".into(),
    version: Some("1.0.0".into()),
    data: json!({ "name": "SDK Demo" }),
    ..Default::default()
}).await?;
let thumbs = client.documents.thumbnails(&id, ThumbnailOptions::new(320)).await?;
```

After:

```rust,ignore
use poli_page::RenderInput;

let pdf = client.render.pdf(RenderInput {
    project: "getting-started".into(),
    template: "welcome".into(),
    data: json!({ "name": "SDK Demo" }),
    ..Default::default()
}).await?;
let thumbs = client
    .documents
    .thumbnails(&id, Some(ThumbnailOptions::new().width(320)))
    .await?;
```

An omitted `format` gives JPEG, the API's default, including when only
`width` is set. 0.9.0 documented PNG as the default; to get PNG, set
`format: Some(ThumbnailFormat::Png)`.

### Error codes

| Removed from `error_codes` | What the API answers now |
|---|---|
| `VERSION_NOT_FOUND`, `INVALID_VERSION_FORMAT`, `VERSION_REQUIRED`, `INVALID_VERSION_FOR_KEY_ENV` | No longer sent: there are no versions. A project that is not published in the key's environment answers `404 PROJECT_NOT_IN_ENVIRONMENT` |
| `MISSING_PROJECT_OR_TEMPLATE` | No longer sent; a request without `project` or `template` fails validation with a `400` |
| `GONE` | `410 DOCUMENT_GONE` (still `Error::Gone`) |
| `PAYMENT_REQUIRED` | `403 ORGANIZATION_PAYMENT_GRACE` |

New constants: `PROJECT_NOT_FOUND`, `PROJECT_NOT_IN_ENVIRONMENT`,
`TEMPLATE_NOT_FOUND`, `DOCUMENT_GONE`, `ENVIRONMENT_MISMATCH`,
`ENVIRONMENT_PROTECTED`, `ORGANIZATION_PAYMENT_GRACE`, `EMPTY_DOCUMENT`,
`TEMPLATE_ERROR`, `THUMBNAILS_NOT_AVAILABLE`, `RATE_LIMIT_EXCEEDED`,
`RENDER_BODY_TOO_LARGE`, `DOCUMENT_TOO_LARGE`, `STORAGE_ACCESS_DENIED`,
`PUBLISH_IN_PROGRESS`, `ORGANIZATION_MIGRATING` and `RENDER_TIMEOUT`.
`503 PUBLISH_IN_PROGRESS` is retried like any 5xx, honouring `Retry-After`.

### Behaviour

- `documents.delete(id)` resolves on `204`, including for an already
  deleted document.
- `sandbox` documents are watermarked, do not count toward the monthly PDF
  quota and expire 30 days after creation.

## 0.9.0

No migration needed — `0.9.0` is the first release of the Rust SDK.
Downstream code starts here.

---

*Future breaking releases and MSRV bumps will be documented as new
sections above.*
