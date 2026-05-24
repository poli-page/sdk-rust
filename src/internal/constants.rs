//! Compile-time constants shared by the transport core.
//!
//! Tests import from here — string literals never get re-typed at call sites.
//! Values mirror the Node reference SDK (`/Users/mickael/Projects/sdk-node/src/index.ts`).

// Phase 1 ships the constants; their consumers (client, render, documents)
// land in Phase 2+. Suppress the per-item dead-code lint module-wide rather
// than annotate each constant individually.
#![allow(dead_code)]

use std::time::Duration;

// API paths
pub(crate) const PATH_RENDER: &str = "/v1/render";
pub(crate) const PATH_RENDER_PREVIEW: &str = "/v1/render/preview";

/// Prefix for per-document endpoints — append `{id}` (URL-encoded) and
/// optionally `/preview` or `/thumbnails`.
pub(crate) const PATH_DOCUMENTS: &str = "/v1/documents";

// Client defaults
pub(crate) const DEFAULT_BASE_URL: &str = "https://api.poli.page";
pub(crate) const DEFAULT_MAX_RETRIES: u32 = 2;
pub(crate) const DEFAULT_RETRY_DELAY: Duration = Duration::from_millis(500);
pub(crate) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// Maximum honored `Retry-After` value — caps server-supplied or computed
/// retry delays so a malicious or buggy server can't pin a client for hours.
pub(crate) const RETRY_AFTER_CAP: Duration = Duration::from_secs(30);

// Custom (non-standard) header names — standard ones come from `http::header`.
pub(crate) const HEADER_IDEMPOTENCY_KEY: &str = "Idempotency-Key";
pub(crate) const HEADER_REQUEST_ID: &str = "X-Request-Id";
pub(crate) const HEADER_DOCUMENT_PAGE_COUNT: &str = "X-Document-Page-Count";
