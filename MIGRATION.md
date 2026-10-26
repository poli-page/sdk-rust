# Migration Guide

This document records breaking changes between versions of `poli-page`
and the upgrade path for downstream code. Before `1.0.0`, a breaking
change bumps the minor version (`0.(x+1).0`), per Cargo's 0.x semver rules.

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
| — | `ThumbnailFormat::Webp` |

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
