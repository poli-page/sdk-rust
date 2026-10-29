//! Single `Error` enum + predicate helpers + wire-level code constants.
//! See spec §7.

use std::time::Duration;

/// Every fallible operation in this crate returns `Result<T, Error>`.
///
/// The variants split into two families:
///
/// - **Reserved (SDK-internal)** — `InvalidOptions`, `Connection`, `Timeout`,
///   `Aborted`, `Download`, `Internal`. Carry no API `code`; the wire-level
///   constant returned by [`Error::code`] is fixed per variant
///   (`"invalid_options"`, `"network_error"`, etc.).
/// - **API status** — `BadRequest`, `Auth`, `PermissionDenied`, `NotFound`,
///   `Gone`, `RateLimited`, `Api`. Carry the HTTP `status`, the wire
///   `code` field, the human `message`, and the `request_id` (when the
///   server returned one).
///
/// Pattern-matching is the idiomatic dispatch path
/// (`matches!(err, Error::Auth { .. })`); the [`is_auth_error`](Self::is_auth_error)
/// and friends are kept for cross-SDK spec parity.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    // --- Reserved (SDK-internal) variants ---
    /// Constructor / builder validation failed (e.g., empty `api_key`).
    /// Displays as its `message`: `apiKey is required` for a missing or
    /// empty key.
    #[error("{message}")]
    InvalidOptions {
        /// Human-readable explanation of what was invalid.
        message: String,
    },

    /// Underlying network failure — DNS, connection refused, TLS, a connection
    /// dropped before or while the answer was read, etc. Displays as its
    /// `message`, `Network request failed`.
    #[error("{message}")]
    Connection {
        /// Human-readable summary of the failure: `Network request failed`.
        message: String,
        /// The underlying `reqwest` / `hyper` error.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// An attempt exceeded the client's per-attempt `timeout`. Displays as
    /// `Request timed out after <timeout in ms>ms`.
    #[error("Request timed out after {}ms", .timeout.as_millis())]
    Timeout {
        /// The per-attempt timeout that was hit.
        timeout: Duration,
    },

    /// The in-flight future was dropped or externally cancelled. Never retried.
    #[error("Request was aborted")]
    Aborted,

    /// Presigned-URL download (the PDF second-hop fetch) failed. Never
    /// retried. Displays as its `message`.
    #[error("{message}")]
    Download {
        /// `Failed to download PDF: HTTP <status>` when the storage answered
        /// with a non-2xx status, `Failed to download PDF: network error`
        /// when it did not answer or the body failed while it was read.
        message: String,
        /// The S3 response status, if the request reached the storage layer.
        status: Option<u16>,
        /// The underlying transport error, if any.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// SDK-side parse / decode failure, or other "shouldn't happen" conditions.
    #[error("internal error: {message}")]
    Internal {
        /// Human-readable summary.
        message: String,
        /// HTTP status from the upstream response, if one was received.
        status: Option<u16>,
    },

    // --- API status variants ---
    /// 4xx validation / shape error returned by the API.
    #[error("bad request ({status}): {message}")]
    BadRequest {
        /// HTTP status code (typically 400 or 422).
        status: u16,
        /// Wire-level error `code` field.
        code: String,
        /// Human-readable error message.
        message: String,
        /// `X-Request-Id` header from the response, if present.
        request_id: Option<String>,
        /// The error that broke the answer's body while it was read, when the
        /// code is `INTERNAL_ERROR` for that reason; `None` otherwise.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// 401 — authentication required or invalid credentials.
    #[error("authentication failed ({status}): {message}")]
    Auth {
        /// HTTP status code (401).
        status: u16,
        /// Wire-level error `code` field.
        code: String,
        /// Human-readable error message.
        message: String,
        /// `X-Request-Id` header from the response, if present.
        request_id: Option<String>,
        /// The error that broke the answer's body while it was read, when the
        /// code is `INTERNAL_ERROR` for that reason; `None` otherwise.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// 403 — the credentials are valid but lack permission for the action.
    #[error("permission denied ({status}): {message}")]
    PermissionDenied {
        /// HTTP status code (403).
        status: u16,
        /// Wire-level error `code` field.
        code: String,
        /// Human-readable error message.
        message: String,
        /// `X-Request-Id` header from the response, if present.
        request_id: Option<String>,
        /// The error that broke the answer's body while it was read, when the
        /// code is `INTERNAL_ERROR` for that reason; `None` otherwise.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// 404 — the requested resource doesn't exist.
    #[error("not found ({status}): {message}")]
    NotFound {
        /// HTTP status code (404).
        status: u16,
        /// Wire-level error `code` field.
        code: String,
        /// Human-readable error message.
        message: String,
        /// `X-Request-Id` header from the response, if present.
        request_id: Option<String>,
        /// The error that broke the answer's body while it was read, when the
        /// code is `INTERNAL_ERROR` for that reason; `None` otherwise.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// 410 — the resource was deleted / no longer available.
    #[error("gone ({status}): {message}")]
    Gone {
        /// HTTP status code (410).
        status: u16,
        /// Wire-level error `code` field.
        code: String,
        /// Human-readable error message.
        message: String,
        /// `X-Request-Id` header from the response, if present.
        request_id: Option<String>,
        /// The error that broke the answer's body while it was read, when the
        /// code is `INTERNAL_ERROR` for that reason; `None` otherwise.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// 429 — caller exceeded the rate limit.
    #[error("rate limited ({status}): {message}")]
    RateLimited {
        /// HTTP status code (429).
        status: u16,
        /// Wire-level error `code` field.
        code: String,
        /// Human-readable error message.
        message: String,
        /// `X-Request-Id` header from the response, if present.
        request_id: Option<String>,
        /// The error that broke the answer's body while it was read, when the
        /// code is `INTERNAL_ERROR` for that reason; `None` otherwise.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Catch-all for any 4xx or 5xx that doesn't match a more specific variant.
    #[error("API error ({status}): {message}")]
    Api {
        /// HTTP status code.
        status: u16,
        /// Wire-level error `code` field.
        code: String,
        /// Human-readable error message.
        message: String,
        /// `X-Request-Id` header from the response, if present.
        request_id: Option<String>,
        /// The error that broke the answer's body while it was read, when the
        /// code is `INTERNAL_ERROR` for that reason; `None` otherwise.
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },
}

impl Clone for Error {
    /// Hand-rolled `Clone` so `RetryEvent` can carry an owned `Error`.
    ///
    /// The `Box<dyn std::error::Error + Send + Sync>` source on `Connection`,
    /// `Download` and the API variants is NOT `Clone` (the trait object holds whatever the
    /// original error was — `reqwest::Error`, `io::Error`, etc.). Per spec
    /// §10.2 the cloned event drops the source — `message`, `code`, `status`,
    /// and `request_id` are preserved; the underlying chain isn't. Good
    /// enough for observability; users who need the source should inspect
    /// the original `Error` returned by the method call.
    fn clone(&self) -> Self {
        // The API variants share their fields; the source is dropped.
        macro_rules! api {
            ($variant:ident, $status:ident, $code:ident, $message:ident, $request_id:ident) => {
                Error::$variant {
                    status: *$status,
                    code: $code.clone(),
                    message: $message.clone(),
                    request_id: $request_id.clone(),
                    source: None,
                }
            };
        }
        match self {
            Error::InvalidOptions { message } => Error::InvalidOptions {
                message: message.clone(),
            },
            Error::Connection { message, .. } => Error::Connection {
                message: message.clone(),
                source: format!("(source dropped on clone): {message}").into(),
            },
            Error::Timeout { timeout } => Error::Timeout { timeout: *timeout },
            Error::Aborted => Error::Aborted,
            Error::Download {
                message, status, ..
            } => Error::Download {
                message: message.clone(),
                status: *status,
                source: None,
            },
            Error::Internal { message, status } => Error::Internal {
                message: message.clone(),
                status: *status,
            },
            Error::BadRequest {
                status,
                code,
                message,
                request_id,
                ..
            } => {
                api!(BadRequest, status, code, message, request_id)
            }
            Error::Auth {
                status,
                code,
                message,
                request_id,
                ..
            } => {
                api!(Auth, status, code, message, request_id)
            }
            Error::PermissionDenied {
                status,
                code,
                message,
                request_id,
                ..
            } => {
                api!(PermissionDenied, status, code, message, request_id)
            }
            Error::NotFound {
                status,
                code,
                message,
                request_id,
                ..
            } => {
                api!(NotFound, status, code, message, request_id)
            }
            Error::Gone {
                status,
                code,
                message,
                request_id,
                ..
            } => {
                api!(Gone, status, code, message, request_id)
            }
            Error::RateLimited {
                status,
                code,
                message,
                request_id,
                ..
            } => {
                api!(RateLimited, status, code, message, request_id)
            }
            Error::Api {
                status,
                code,
                message,
                request_id,
                ..
            } => {
                api!(Api, status, code, message, request_id)
            }
        }
    }
}

/// The codes a 429 or 5xx can carry that waiting cannot fix: the two quota
/// codes last until the monthly reset, `STORAGE_ACCESS_DENIED` until the
/// customer restores its bucket role. Never retried (spec §9.1).
pub(crate) const NON_RETRYABLE_CODES: [&str; 3] = [
    error_codes::QUOTA_EXCEEDED,
    error_codes::OVERAGE_CAP_EXCEEDED,
    error_codes::STORAGE_ACCESS_DENIED,
];

impl Error {
    /// `true` when [`status`](Self::status) is 401 or 403 — spec §8.1. That
    /// covers `Auth`, `PermissionDenied` and a `Download` refused by the
    /// storage with 401 or 403.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use poli_page::{Error, PoliPage, RenderInput};
    /// # async fn run(client: PoliPage, input: RenderInput) {
    /// if let Err(err) = client.render.pdf(input).await {
    ///     if err.is_auth_error() {
    ///         eprintln!("refresh credentials: {err}");
    ///     }
    /// }
    /// # }
    /// ```
    #[must_use]
    pub fn is_auth_error(&self) -> bool {
        matches!(self.status(), Some(401 | 403))
    }

    /// `true` when [`status`](Self::status) is 429 (`RateLimited`, or a
    /// `Download` the storage answered with 429).
    #[must_use]
    pub fn is_rate_limit_error(&self) -> bool {
        self.status() == Some(429)
    }

    /// `true` when [`status`](Self::status) is 400 — spec §8.1, the same in
    /// every Poli Page SDK. A 422 (an `Error::BadRequest` too) is not a
    /// validation error.
    #[must_use]
    pub fn is_validation_error(&self) -> bool {
        self.status() == Some(400)
    }

    /// `true` for `Connection` and `Timeout` — both indicate the request
    /// never produced a complete response.
    #[must_use]
    pub fn is_network_error(&self) -> bool {
        matches!(self, Error::Connection { .. } | Error::Timeout { .. })
    }

    /// `true` when the SDK retries this error (spec §9.1): a network failure,
    /// a timeout, a 5xx or a 429, except `QUOTA_EXCEEDED`,
    /// `OVERAGE_CAP_EXCEEDED` and `STORAGE_ACCESS_DENIED`. Never for
    /// `Aborted`, and never for `Download` (`DOWNLOAD_FAILED`): the
    /// presigned-URL download is not retried, whatever the storage status.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            Error::Aborted | Error::Download { .. } | Error::InvalidOptions { .. } => false,
            Error::Connection { .. } | Error::Timeout { .. } => true,
            _ => match self.status() {
                Some(s) if s >= 500 || s == 429 => !NON_RETRYABLE_CODES.contains(&self.code()),
                _ => false,
            },
        }
    }

    /// The human-readable explanation (spec §8.1 `message`), without the
    /// variant prefix that `Display` adds to API errors. For the SDK-side
    /// variants it is the exact text of the spec (§8.3): `apiKey is
    /// required`, `Network request failed`, `Request timed out after <ms>ms`,
    /// `Request was aborted`, `Failed to download PDF: HTTP <status>` or
    /// `Failed to download PDF: network error`. For API errors it is the
    /// body's `detail`, else `title`, else `message`, else `HTTP <status>`.
    #[must_use]
    pub fn message(&self) -> std::borrow::Cow<'_, str> {
        match self {
            Error::InvalidOptions { message }
            | Error::Connection { message, .. }
            | Error::Download { message, .. }
            | Error::Internal { message, .. }
            | Error::BadRequest { message, .. }
            | Error::Auth { message, .. }
            | Error::PermissionDenied { message, .. }
            | Error::NotFound { message, .. }
            | Error::Gone { message, .. }
            | Error::RateLimited { message, .. }
            | Error::Api { message, .. } => std::borrow::Cow::Borrowed(message),
            Error::Timeout { .. } | Error::Aborted => std::borrow::Cow::Owned(self.to_string()),
        }
    }

    /// The HTTP status code returned by the upstream API, if one was received.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        match self {
            Error::BadRequest { status, .. }
            | Error::Auth { status, .. }
            | Error::PermissionDenied { status, .. }
            | Error::NotFound { status, .. }
            | Error::Gone { status, .. }
            | Error::RateLimited { status, .. }
            | Error::Api { status, .. } => Some(*status),
            Error::Internal { status, .. } | Error::Download { status, .. } => *status,
            Error::InvalidOptions { .. }
            | Error::Connection { .. }
            | Error::Timeout { .. }
            | Error::Aborted => None,
        }
    }

    /// The wire-level error code. For reserved variants this is a fixed string
    /// (`"invalid_options"`, `"network_error"`, `"timeout"`, `"aborted"`,
    /// `"DOWNLOAD_FAILED"`, `"INTERNAL_ERROR"`); for API variants it's the
    /// `code` field returned by the server.
    #[must_use]
    pub fn code(&self) -> &str {
        match self {
            Error::InvalidOptions { .. } => "invalid_options",
            Error::Connection { .. } => "network_error",
            Error::Timeout { .. } => "timeout",
            Error::Aborted => "aborted",
            Error::Download { .. } => "DOWNLOAD_FAILED",
            Error::Internal { .. } => "INTERNAL_ERROR",
            Error::BadRequest { code, .. }
            | Error::Auth { code, .. }
            | Error::PermissionDenied { code, .. }
            | Error::NotFound { code, .. }
            | Error::Gone { code, .. }
            | Error::RateLimited { code, .. }
            | Error::Api { code, .. } => code,
        }
    }

    /// The `X-Request-Id` value the server returned with this error, if any.
    /// Always `None` for SDK-internal variants (no upstream response).
    #[must_use]
    pub fn request_id(&self) -> Option<&str> {
        match self {
            Error::BadRequest { request_id, .. }
            | Error::Auth { request_id, .. }
            | Error::PermissionDenied { request_id, .. }
            | Error::NotFound { request_id, .. }
            | Error::Gone { request_id, .. }
            | Error::RateLimited { request_id, .. }
            | Error::Api { request_id, .. } => request_id.as_deref(),
            _ => None,
        }
    }

    /// Returns the canonical wire payload for framework integrations.
    ///
    /// `status` surfaces 503 for `Connection`, 504 for `Timeout`, and the
    /// API status for status-bearing variants — independent of the
    /// [`Error::status`] accessor, which keeps returning `None` for
    /// transport variants so existing callers are unaffected. The
    /// `message` is [`Error::message`] (the bare reason), not the prefixed
    /// `Display` form. `status` and `request_id` serialize as `null` when
    /// unknown.
    #[must_use]
    pub fn to_payload(&self) -> ErrorPayload {
        let status = match self {
            Error::Connection { .. } => Some(503),
            Error::Timeout { .. } => Some(504),
            _ => self.status(),
        };
        let message = self.message().into_owned();
        ErrorPayload {
            code: self.code().to_string(),
            message,
            status,
            request_id: self.request_id().map(str::to_string),
        }
    }
}

/// Canonical wire payload for framework integrations:
/// `{code, message, status, requestId}` with camelCase JSON keys.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorPayload {
    /// Wire-level error code, verbatim from the API.
    pub code: String,
    /// Human-readable error message.
    pub message: String,
    /// HTTP status to surface on the wire. `None` is serialised as `null`.
    pub status: Option<u16>,
    /// Server-assigned request identifier, or `None`.
    pub request_id: Option<String>,
}

/// Wire-level error code constants — see spec §7.4. Pass-through verbatim from
/// the deployed API; users may still see codes not in this list.
///
/// `STORAGE_REQUIRED` was retired from the API and is intentionally absent.
pub mod error_codes {
    // --- Authentication and organization state (401 / 403) ---
    /// Returned when the request omits the `Authorization` header entirely.
    pub const MISSING_API_KEY: &str = "MISSING_API_KEY";
    /// The supplied API key is malformed or doesn't match any tenant.
    pub const INVALID_API_KEY: &str = "INVALID_API_KEY";
    /// The credentials are valid but lack permission for the action.
    pub const FORBIDDEN: &str = "FORBIDDEN";
    /// The tenant's organization was cancelled.
    pub const ORGANIZATION_CANCELLED: &str = "ORGANIZATION_CANCELLED";
    /// The tenant's organization data was purged.
    pub const ORGANIZATION_PURGED: &str = "ORGANIZATION_PURGED";
    /// The organization is in its payment grace period (403).
    pub const ORGANIZATION_PAYMENT_GRACE: &str = "ORGANIZATION_PAYMENT_GRACE";
    /// The API key's environment is protected against this operation (403).
    pub const ENVIRONMENT_PROTECTED: &str = "ENVIRONMENT_PROTECTED";
    /// Thumbnails are not included in the organization's plan (403).
    pub const THUMBNAILS_NOT_AVAILABLE: &str = "THUMBNAILS_NOT_AVAILABLE";

    // --- Not found / gone (404 / 410) ---
    /// Generic 404 — no resource matched the request.
    pub const NOT_FOUND: &str = "NOT_FOUND";
    /// No project matches the requested slug (404).
    pub const PROJECT_NOT_FOUND: &str = "PROJECT_NOT_FOUND";
    /// The project exists but is not published in the API key's environment (404).
    pub const PROJECT_NOT_IN_ENVIRONMENT: &str = "PROJECT_NOT_IN_ENVIRONMENT";
    /// No template matches the requested slug in the published project (404).
    pub const TEMPLATE_NOT_FOUND: &str = "TEMPLATE_NOT_FOUND";
    /// The requested document ID doesn't exist.
    pub const DOCUMENT_NOT_FOUND: &str = "DOCUMENT_NOT_FOUND";
    /// The document existed but was deleted (410).
    pub const DOCUMENT_GONE: &str = "DOCUMENT_GONE";

    // --- Validation (400 / 413) ---
    /// Request body failed schema validation (including unknown fields).
    pub const VALIDATION_ERROR: &str = "VALIDATION_ERROR";
    /// `data` field was omitted from a render request.
    pub const MISSING_DATA: &str = "MISSING_DATA";
    /// `template` slug missing from a render request.
    pub const MISSING_TEMPLATE_SLUG: &str = "MISSING_TEMPLATE_SLUG";
    /// Storing a document requires a `project` selector.
    pub const PROJECT_REQUIRED_FOR_DOCUMENT: &str = "PROJECT_REQUIRED_FOR_DOCUMENT";
    /// The request targets an environment other than the API key's (400).
    pub const ENVIRONMENT_MISMATCH: &str = "ENVIRONMENT_MISMATCH";
    /// The template failed to render with the supplied data (400).
    pub const TEMPLATE_ERROR: &str = "TEMPLATE_ERROR";
    /// The template, a partial or an SVG asset holds an injection attempt
    /// (400); `detail` names it. Not retried.
    pub const TEMPLATE_UNSAFE: &str = "TEMPLATE_UNSAFE";
    /// The render produced a document with no pages (400).
    pub const EMPTY_DOCUMENT: &str = "EMPTY_DOCUMENT";
    /// The render request body exceeds the size limit (413).
    pub const RENDER_BODY_TOO_LARGE: &str = "RENDER_BODY_TOO_LARGE";
    /// The rendered document exceeds the size limit (413).
    pub const DOCUMENT_TOO_LARGE: &str = "DOCUMENT_TOO_LARGE";

    // --- Quotas and rate limits (429) ---
    /// Tenant exceeded its plan quota.
    pub const QUOTA_EXCEEDED: &str = "QUOTA_EXCEEDED";
    /// Tenant exceeded its overage cap.
    pub const OVERAGE_CAP_EXCEEDED: &str = "OVERAGE_CAP_EXCEEDED";
    /// Too many requests in the rate-limit window (429). Retried.
    pub const RATE_LIMIT_EXCEEDED: &str = "RATE_LIMIT_EXCEEDED";

    // --- Server side (5xx) ---
    /// Generic 5xx — server-side internal failure.
    pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
    /// The API could not read or write document storage (502).
    pub const STORAGE_ACCESS_DENIED: &str = "STORAGE_ACCESS_DENIED";
    /// A publish holds the project in this environment (503, with
    /// `Retry-After`). Retried like any 5xx.
    pub const PUBLISH_IN_PROGRESS: &str = "PUBLISH_IN_PROGRESS";
    /// The organization is being migrated (503). Retried.
    pub const ORGANIZATION_MIGRATING: &str = "ORGANIZATION_MIGRATING";
    /// The render did not finish in time (504). Retried.
    pub const RENDER_TIMEOUT: &str = "RENDER_TIMEOUT";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api(status: u16) -> Error {
        Error::Api {
            status,
            code: "GENERIC".into(),
            message: "x".into(),
            request_id: Some("req_1".into()),
            source: None,
        }
    }

    fn auth() -> Error {
        Error::Auth {
            status: 401,
            code: error_codes::INVALID_API_KEY.into(),
            message: "x".into(),
            request_id: None,
            source: None,
        }
    }

    fn perm() -> Error {
        Error::PermissionDenied {
            status: 403,
            code: error_codes::FORBIDDEN.into(),
            message: "x".into(),
            request_id: None,
            source: None,
        }
    }

    fn rate() -> Error {
        Error::RateLimited {
            status: 429,
            code: error_codes::RATE_LIMIT_EXCEEDED.into(),
            message: "x".into(),
            request_id: None,
            source: None,
        }
    }

    fn bad() -> Error {
        Error::BadRequest {
            status: 400,
            code: error_codes::VALIDATION_ERROR.into(),
            message: "x".into(),
            request_id: None,
            source: None,
        }
    }

    fn conn() -> Error {
        Error::Connection {
            message: "dns".into(),
            source: Box::<dyn std::error::Error + Send + Sync>::from("inner"),
        }
    }

    fn timeout() -> Error {
        Error::Timeout {
            timeout: Duration::from_secs(60),
        }
    }

    // --- predicates ---

    #[test]
    fn is_auth_error_covers_401_and_403() {
        assert!(auth().is_auth_error());
        assert!(perm().is_auth_error());
        assert!(!api(500).is_auth_error());
        assert!(!rate().is_auth_error());
    }

    #[test]
    fn is_rate_limit_error_is_status_429() {
        assert!(rate().is_rate_limit_error());
        assert!(!auth().is_rate_limit_error());
        // Spec §8.1: the helpers read the status, whatever the variant.
        assert!(api(429).is_rate_limit_error());
        assert!(download(Some(429)).is_rate_limit_error());
    }

    #[test]
    fn is_validation_error_is_status_400_only() {
        assert!(bad().is_validation_error());
        assert!(api(400).is_validation_error());
        assert!(!api(404).is_validation_error());
        // A 422 is a `BadRequest`, but not a validation error (spec §8.1).
        let unprocessable = Error::BadRequest {
            status: 422,
            code: error_codes::VALIDATION_ERROR.into(),
            message: "x".into(),
            request_id: None,
            source: None,
        };
        assert!(!unprocessable.is_validation_error());
    }

    #[test]
    fn is_auth_error_covers_a_download_refused_with_401_or_403() {
        assert!(download(Some(403)).is_auth_error());
        assert!(download(Some(401)).is_auth_error());
        assert!(!download(Some(404)).is_auth_error());
        assert!(!download(None).is_auth_error());
    }

    fn download(status: Option<u16>) -> Error {
        Error::Download {
            message: "Failed to download PDF: HTTP 503".into(),
            status,
            source: None,
        }
    }

    fn with_code(status: u16, code: &str) -> Error {
        Error::RateLimited {
            status,
            code: code.into(),
            message: "x".into(),
            request_id: None,
            source: None,
        }
    }

    #[test]
    fn is_retryable_says_no_for_quota_overage_and_storage_access_denied() {
        // Spec §9.1, R-NONRETRY: waiting cannot fix them.
        assert!(!with_code(429, error_codes::QUOTA_EXCEEDED).is_retryable());
        assert!(!with_code(429, error_codes::OVERAGE_CAP_EXCEEDED).is_retryable());
        assert!(!with_code(502, error_codes::STORAGE_ACCESS_DENIED).is_retryable());
        assert!(with_code(429, error_codes::RATE_LIMIT_EXCEEDED).is_retryable());
        assert!(with_code(503, error_codes::PUBLISH_IN_PROGRESS).is_retryable());
    }

    #[test]
    fn is_retryable_is_false_for_download_failed_whatever_the_storage_status() {
        // R-DOWNLOAD: the presigned-URL download is never retried, so its
        // error is never retryable, even for a storage 503 or 429.
        for status in [None, Some(403), Some(429), Some(500), Some(502), Some(503)] {
            assert!(!download(status).is_retryable(), "status {status:?}");
        }
    }

    #[test]
    fn message_is_the_spec_text_for_sdk_side_errors() {
        assert_eq!(
            Error::Timeout {
                timeout: Duration::from_millis(150)
            }
            .message(),
            "Request timed out after 150ms"
        );
        assert_eq!(timeout().message(), "Request timed out after 60000ms");
        assert_eq!(Error::Aborted.message(), "Request was aborted");
        assert_eq!(
            download(Some(503)).message(),
            "Failed to download PDF: HTTP 503"
        );
        assert_eq!(api(500).message(), "x");
    }

    #[test]
    fn sdk_side_errors_display_as_their_message() {
        let invalid = Error::InvalidOptions {
            message: "apiKey is required".into(),
        };
        assert_eq!(invalid.to_string(), "apiKey is required");
        let conn = Error::Connection {
            message: "Network request failed".into(),
            source: Box::<dyn std::error::Error + Send + Sync>::from("inner"),
        };
        assert_eq!(conn.to_string(), "Network request failed");
        assert_eq!(timeout().to_string(), "Request timed out after 60000ms");
        assert_eq!(
            download(None).to_string(),
            "Failed to download PDF: HTTP 503"
        );
    }

    #[test]
    fn api_error_keeps_the_body_read_error_as_its_source() {
        use std::error::Error as _;
        let err = Error::Api {
            status: 503,
            code: error_codes::INTERNAL_ERROR.into(),
            message: "HTTP 503".into(),
            request_id: Some("req_1".into()),
            source: Some(Box::<dyn std::error::Error + Send + Sync>::from("cut off")),
        };
        assert_eq!(
            err.source().map(ToString::to_string).as_deref(),
            Some("cut off")
        );
        assert!(api(503).source().is_none());
        // A clone drops the source, keeping the rest.
        let cloned = err.clone();
        assert!(cloned.source().is_none());
        assert_eq!(cloned.request_id(), Some("req_1"));
    }

    #[test]
    fn is_network_error_covers_connection_and_timeout() {
        assert!(conn().is_network_error());
        assert!(timeout().is_network_error());
        assert!(!api(500).is_network_error());
        assert!(!Error::Aborted.is_network_error());
    }

    #[test]
    fn is_retryable_says_yes_for_5xx_and_429() {
        assert!(api(500).is_retryable());
        assert!(api(503).is_retryable());
        assert!(rate().is_retryable());
    }

    #[test]
    fn is_retryable_says_no_for_4xx_other_than_429() {
        assert!(!auth().is_retryable());
        assert!(!perm().is_retryable());
        assert!(!bad().is_retryable());
        assert!(!api(404).is_retryable());
    }

    #[test]
    fn is_retryable_says_yes_for_network_and_timeout() {
        assert!(conn().is_retryable());
        assert!(timeout().is_retryable());
    }

    #[test]
    fn is_retryable_says_no_for_aborted_even_when_network_ish() {
        assert!(!Error::Aborted.is_retryable());
    }

    // --- accessors ---

    #[test]
    fn status_extracts_for_api_variants_and_is_none_for_reserved() {
        assert_eq!(api(500).status(), Some(500));
        assert_eq!(auth().status(), Some(401));
        assert_eq!(perm().status(), Some(403));
        assert_eq!(rate().status(), Some(429));
        assert_eq!(bad().status(), Some(400));
        assert_eq!(conn().status(), None);
        assert_eq!(timeout().status(), None);
        assert_eq!(Error::Aborted.status(), None);
        assert_eq!(
            Error::InvalidOptions {
                message: "x".into()
            }
            .status(),
            None,
        );
    }

    #[test]
    fn status_for_download_and_internal_passes_through_option() {
        assert_eq!(
            Error::Download {
                message: "x".into(),
                status: Some(404),
                source: None,
            }
            .status(),
            Some(404),
        );
        assert_eq!(
            Error::Internal {
                message: "x".into(),
                status: None,
            }
            .status(),
            None,
        );
    }

    #[test]
    fn code_returns_fixed_string_for_reserved_variants() {
        assert_eq!(
            Error::InvalidOptions {
                message: "x".into()
            }
            .code(),
            "invalid_options",
        );
        assert_eq!(conn().code(), "network_error");
        assert_eq!(timeout().code(), "timeout");
        assert_eq!(Error::Aborted.code(), "aborted");
        assert_eq!(
            Error::Download {
                message: "x".into(),
                status: None,
                source: None,
            }
            .code(),
            "DOWNLOAD_FAILED",
        );
        assert_eq!(
            Error::Internal {
                message: "x".into(),
                status: None,
            }
            .code(),
            "INTERNAL_ERROR",
        );
    }

    #[test]
    fn code_returns_wire_code_for_api_variants() {
        assert_eq!(auth().code(), error_codes::INVALID_API_KEY);
        assert_eq!(perm().code(), error_codes::FORBIDDEN);
        assert_eq!(rate().code(), error_codes::RATE_LIMIT_EXCEEDED);
        assert_eq!(bad().code(), error_codes::VALIDATION_ERROR);
        assert_eq!(api(503).code(), "GENERIC");
    }

    #[test]
    fn request_id_present_only_when_server_returned_one() {
        assert_eq!(api(500).request_id(), Some("req_1"));
        assert_eq!(auth().request_id(), None);
        assert_eq!(conn().request_id(), None);
        assert_eq!(Error::Aborted.request_id(), None);
    }

    // --- Display impls (driven by thiserror's #[error("...")]) ---

    #[test]
    fn display_includes_status_for_api_variants() {
        assert!(api(500).to_string().contains("500"));
        assert!(auth().to_string().contains("401"));
    }

    #[test]
    fn display_for_aborted_is_the_spec_message() {
        assert_eq!(Error::Aborted.to_string(), "Request was aborted");
    }

    // --- to_payload ---

    #[test]
    fn to_payload_uses_api_status_for_status_bearing_variants() {
        let p = auth().to_payload();
        assert_eq!(p.status, Some(401));
        assert_eq!(p.code, error_codes::INVALID_API_KEY);
        assert_eq!(p.message, "x");
    }

    #[test]
    fn to_payload_uses_503_for_connection() {
        let p = conn().to_payload();
        assert_eq!(p.status, Some(503));
        // Error::status() attribute stays None for transport variants.
        assert_eq!(conn().status(), None);
    }

    #[test]
    fn to_payload_uses_504_for_timeout() {
        let p = timeout().to_payload();
        assert_eq!(p.status, Some(504));
        assert_eq!(timeout().status(), None);
    }

    #[test]
    fn to_payload_message_does_not_include_status_prefix() {
        // Bare reason — not the thiserror Display string ("API error (500): x").
        let p = api(500).to_payload();
        assert_eq!(p.message, "x");
    }

    // --- error_codes ---

    /// The 32 API codes of the environments contract, and nothing else.
    const CONTRACT_API_CODES: [&str; 32] = [
        "DOCUMENT_GONE",
        "DOCUMENT_NOT_FOUND",
        "DOCUMENT_TOO_LARGE",
        "EMPTY_DOCUMENT",
        "ENVIRONMENT_MISMATCH",
        "ENVIRONMENT_PROTECTED",
        "FORBIDDEN",
        "INTERNAL_ERROR",
        "INVALID_API_KEY",
        "MISSING_API_KEY",
        "MISSING_DATA",
        "MISSING_TEMPLATE_SLUG",
        "NOT_FOUND",
        "ORGANIZATION_CANCELLED",
        "ORGANIZATION_MIGRATING",
        "ORGANIZATION_PAYMENT_GRACE",
        "ORGANIZATION_PURGED",
        "OVERAGE_CAP_EXCEEDED",
        "PROJECT_NOT_FOUND",
        "PROJECT_NOT_IN_ENVIRONMENT",
        "PROJECT_REQUIRED_FOR_DOCUMENT",
        "PUBLISH_IN_PROGRESS",
        "QUOTA_EXCEEDED",
        "RATE_LIMIT_EXCEEDED",
        "RENDER_BODY_TOO_LARGE",
        "RENDER_TIMEOUT",
        "STORAGE_ACCESS_DENIED",
        "TEMPLATE_ERROR",
        "TEMPLATE_NOT_FOUND",
        "TEMPLATE_UNSAFE",
        "THUMBNAILS_NOT_AVAILABLE",
        "VALIDATION_ERROR",
    ];

    /// `(name, value)` of every constant in the `error_codes` module of `src`, sorted.
    fn error_code_constants(src: &str) -> Vec<(&str, &str)> {
        let start = src
            .find("pub mod error_codes {")
            .expect("error_codes module");
        let module = &src[start..];
        // The first column-0 `}` closes the module; `\n}` also matches CRLF.
        let module = &module[..module.find("\n}").expect("module end")];
        let mut found: Vec<(&str, &str)> = module
            .lines()
            .filter_map(|line| line.trim().strip_prefix("pub const "))
            .map(|rest| {
                let (name, value) = rest.split_once(": &str = ").expect("const shape");
                (name, value.trim_end_matches(';').trim_matches('"'))
            })
            .collect();
        found.sort_unstable();
        found
    }

    #[test]
    fn error_code_constants_reads_a_crlf_checkout() {
        // A Windows checkout can turn the source into CRLF.
        let lf = include_str!("error.rs").replace("\r\n", "\n");
        let crlf = lf.replace('\n', "\r\n");
        assert_eq!(error_code_constants(&lf).len(), CONTRACT_API_CODES.len());
        assert_eq!(error_code_constants(&crlf), error_code_constants(&lf));
    }

    #[test]
    fn error_codes_module_holds_exactly_the_contract_api_codes() {
        // Read the constants from the source so a removed code that lingers,
        // or a new code that is not in the contract, fails here.
        let found = error_code_constants(include_str!("error.rs"));
        for (name, value) in &found {
            assert_eq!(name, value, "constant {name} must equal its own name");
        }
        let names: Vec<&str> = found.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, CONTRACT_API_CODES);
    }

    #[test]
    fn error_codes_new_in_the_environments_contract_are_exported() {
        assert_eq!(error_codes::PUBLISH_IN_PROGRESS, "PUBLISH_IN_PROGRESS");
        assert_eq!(error_codes::ENVIRONMENT_MISMATCH, "ENVIRONMENT_MISMATCH");
        assert_eq!(
            error_codes::PROJECT_NOT_IN_ENVIRONMENT,
            "PROJECT_NOT_IN_ENVIRONMENT"
        );
        assert_eq!(error_codes::TEMPLATE_NOT_FOUND, "TEMPLATE_NOT_FOUND");
        assert_eq!(error_codes::DOCUMENT_GONE, "DOCUMENT_GONE");
        assert_eq!(error_codes::ENVIRONMENT_PROTECTED, "ENVIRONMENT_PROTECTED");
        assert_eq!(
            error_codes::ORGANIZATION_PAYMENT_GRACE,
            "ORGANIZATION_PAYMENT_GRACE"
        );
        assert_eq!(error_codes::EMPTY_DOCUMENT, "EMPTY_DOCUMENT");
        assert_eq!(error_codes::STORAGE_ACCESS_DENIED, "STORAGE_ACCESS_DENIED");
        assert_eq!(
            error_codes::ORGANIZATION_MIGRATING,
            "ORGANIZATION_MIGRATING"
        );
        assert_eq!(error_codes::PROJECT_NOT_FOUND, "PROJECT_NOT_FOUND");
        assert_eq!(error_codes::TEMPLATE_ERROR, "TEMPLATE_ERROR");
        assert_eq!(
            error_codes::THUMBNAILS_NOT_AVAILABLE,
            "THUMBNAILS_NOT_AVAILABLE"
        );
        assert_eq!(error_codes::RATE_LIMIT_EXCEEDED, "RATE_LIMIT_EXCEEDED");
        assert_eq!(error_codes::RENDER_BODY_TOO_LARGE, "RENDER_BODY_TOO_LARGE");
        assert_eq!(error_codes::DOCUMENT_TOO_LARGE, "DOCUMENT_TOO_LARGE");
        assert_eq!(error_codes::RENDER_TIMEOUT, "RENDER_TIMEOUT");
    }

    #[test]
    fn to_payload_writes_null_status_and_request_id_when_unknown() {
        // Spec §8.1: both keys are present, as null.
        let raw = serde_json::to_value(Error::Aborted.to_payload()).expect("serialise payload");
        assert_eq!(
            raw,
            serde_json::json!({
                "code": "aborted",
                "message": "Request was aborted",
                "status": null,
                "requestId": null,
            })
        );
        let raw = serde_json::to_value(timeout().to_payload()).expect("serialise payload");
        assert_eq!(raw["message"], "Request timed out after 60000ms");
        assert_eq!(raw["status"], 504);
        assert!(raw["requestId"].is_null());
    }

    #[test]
    fn to_payload_serializes_camel_case_request_id() {
        let p = api(500).to_payload();
        let raw = serde_json::to_value(&p).expect("serialise payload");
        assert!(raw.get("requestId").is_some(), "payload was {raw}");
        assert!(raw.get("request_id").is_none(), "payload was {raw}");
    }
}
