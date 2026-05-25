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
    #[error("invalid options: {message}")]
    InvalidOptions {
        /// Human-readable explanation of what was invalid.
        message: String,
    },

    /// Underlying network failure — DNS, connection refused, TLS, decode, etc.
    #[error("network error: {message}")]
    Connection {
        /// Human-readable summary of the failure.
        message: String,
        /// The underlying `reqwest` / `hyper` error, if available.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Request deadline exceeded.
    #[error("request timed out after {timeout:?}")]
    Timeout {
        /// The deadline that was hit.
        timeout: Duration,
    },

    /// The in-flight future was dropped or externally cancelled. Never retried.
    #[error("request was aborted")]
    Aborted,

    /// Presigned-URL download (the PDF second-hop fetch) failed.
    #[error("download failed: {message}")]
    Download {
        /// Human-readable summary.
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
    },
}

impl Clone for Error {
    /// Hand-rolled `Clone` so `RetryEvent` can carry an owned `Error`.
    ///
    /// The `Box<dyn std::error::Error + Send + Sync>` source on `Connection`
    /// and `Download` is NOT `Clone` (the trait object holds whatever the
    /// original error was — `reqwest::Error`, `io::Error`, etc.). Per spec
    /// §10.2 the cloned event drops the source — `message`, `code`, `status`,
    /// and `request_id` are preserved; the underlying chain isn't. Good
    /// enough for observability; users who need the source should inspect
    /// the original `Error` returned by the method call.
    fn clone(&self) -> Self {
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
            } => Error::BadRequest {
                status: *status,
                code: code.clone(),
                message: message.clone(),
                request_id: request_id.clone(),
            },
            Error::Auth {
                status,
                code,
                message,
                request_id,
            } => Error::Auth {
                status: *status,
                code: code.clone(),
                message: message.clone(),
                request_id: request_id.clone(),
            },
            Error::PermissionDenied {
                status,
                code,
                message,
                request_id,
            } => Error::PermissionDenied {
                status: *status,
                code: code.clone(),
                message: message.clone(),
                request_id: request_id.clone(),
            },
            Error::NotFound {
                status,
                code,
                message,
                request_id,
            } => Error::NotFound {
                status: *status,
                code: code.clone(),
                message: message.clone(),
                request_id: request_id.clone(),
            },
            Error::Gone {
                status,
                code,
                message,
                request_id,
            } => Error::Gone {
                status: *status,
                code: code.clone(),
                message: message.clone(),
                request_id: request_id.clone(),
            },
            Error::RateLimited {
                status,
                code,
                message,
                request_id,
            } => Error::RateLimited {
                status: *status,
                code: code.clone(),
                message: message.clone(),
                request_id: request_id.clone(),
            },
            Error::Api {
                status,
                code,
                message,
                request_id,
            } => Error::Api {
                status: *status,
                code: code.clone(),
                message: message.clone(),
                request_id: request_id.clone(),
            },
        }
    }
}

impl Error {
    /// `true` for `Auth` (401) and `PermissionDenied` (403) — spec §7.1.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use poli_page::{Error, PoliPage, ProjectModeInput};
    /// # async fn run(client: PoliPage, input: ProjectModeInput) {
    /// if let Err(err) = client.render.pdf(input).await {
    ///     if err.is_auth_error() {
    ///         eprintln!("refresh credentials: {err}");
    ///     }
    /// }
    /// # }
    /// ```
    #[must_use]
    pub fn is_auth_error(&self) -> bool {
        matches!(self, Error::Auth { .. } | Error::PermissionDenied { .. })
    }

    /// `true` for `RateLimited` (429).
    #[must_use]
    pub fn is_rate_limit_error(&self) -> bool {
        matches!(self, Error::RateLimited { .. })
    }

    /// `true` for `BadRequest` (400 / 422).
    #[must_use]
    pub fn is_validation_error(&self) -> bool {
        matches!(self, Error::BadRequest { .. })
    }

    /// `true` for `Connection` and `Timeout` — both indicate the request
    /// never produced a complete response.
    #[must_use]
    pub fn is_network_error(&self) -> bool {
        matches!(self, Error::Connection { .. } | Error::Timeout { .. })
    }

    /// `true` when the request can safely be retried: any 5xx, 429, or
    /// network/timeout failure. Aborted requests are never retried.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        if matches!(self, Error::Aborted) {
            return false;
        }
        if self.is_network_error() {
            return true;
        }
        match self.status() {
            Some(s) => s >= 500 || s == 429,
            None => false,
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
}

/// Wire-level error code constants — see spec §7.4. Pass-through verbatim from
/// the deployed API; users may still see codes not in this list.
///
/// `STORAGE_REQUIRED` was retired from the API and is intentionally absent.
pub mod error_codes {
    /// Returned when the request omits the `Authorization` header entirely.
    pub const MISSING_API_KEY: &str = "MISSING_API_KEY";
    /// The supplied API key is malformed or doesn't match any tenant.
    pub const INVALID_API_KEY: &str = "INVALID_API_KEY";
    /// The tenant's billing is past due (402-class).
    pub const PAYMENT_REQUIRED: &str = "PAYMENT_REQUIRED";
    /// The credentials are valid but lack permission for the action.
    pub const FORBIDDEN: &str = "FORBIDDEN";
    /// The tenant's organization was cancelled.
    pub const ORGANIZATION_CANCELLED: &str = "ORGANIZATION_CANCELLED";
    /// The tenant's organization data was purged.
    pub const ORGANIZATION_PURGED: &str = "ORGANIZATION_PURGED";
    /// Generic 404 — no resource matched the request.
    pub const NOT_FOUND: &str = "NOT_FOUND";
    /// The requested template version doesn't exist for the project/template.
    pub const VERSION_NOT_FOUND: &str = "VERSION_NOT_FOUND";
    /// The requested document ID doesn't exist.
    pub const DOCUMENT_NOT_FOUND: &str = "DOCUMENT_NOT_FOUND";
    /// The resource was deleted (410-class).
    pub const GONE: &str = "GONE";
    /// Request body failed schema validation.
    pub const VALIDATION_ERROR: &str = "VALIDATION_ERROR";
    /// `data` field was omitted from a render request.
    pub const MISSING_DATA: &str = "MISSING_DATA";
    /// Neither `project` nor `template` was provided in project-mode input.
    pub const MISSING_PROJECT_OR_TEMPLATE: &str = "MISSING_PROJECT_OR_TEMPLATE";
    /// `template` slug missing in project-mode input.
    pub const MISSING_TEMPLATE_SLUG: &str = "MISSING_TEMPLATE_SLUG";
    /// `documents.preview` requires a `project` selector.
    pub const PROJECT_REQUIRED_FOR_DOCUMENT: &str = "PROJECT_REQUIRED_FOR_DOCUMENT";
    /// Version selector wasn't `draft` or an exact semver.
    pub const INVALID_VERSION_FORMAT: &str = "INVALID_VERSION_FORMAT";
    /// Project mode requires an explicit `version` selector.
    pub const VERSION_REQUIRED: &str = "VERSION_REQUIRED";
    /// `draft` versions can't be rendered with a live key (or vice versa).
    pub const INVALID_VERSION_FOR_KEY_ENV: &str = "INVALID_VERSION_FOR_KEY_ENV";
    /// Tenant exceeded its plan quota.
    pub const QUOTA_EXCEEDED: &str = "QUOTA_EXCEEDED";
    /// Tenant exceeded its overage cap.
    pub const OVERAGE_CAP_EXCEEDED: &str = "OVERAGE_CAP_EXCEEDED";
    /// Generic 5xx — server-side internal failure.
    pub const INTERNAL_ERROR: &str = "INTERNAL_ERROR";
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
        }
    }

    fn auth() -> Error {
        Error::Auth {
            status: 401,
            code: error_codes::INVALID_API_KEY.into(),
            message: "x".into(),
            request_id: None,
        }
    }

    fn perm() -> Error {
        Error::PermissionDenied {
            status: 403,
            code: error_codes::FORBIDDEN.into(),
            message: "x".into(),
            request_id: None,
        }
    }

    fn rate() -> Error {
        Error::RateLimited {
            status: 429,
            code: error_codes::QUOTA_EXCEEDED.into(),
            message: "x".into(),
            request_id: None,
        }
    }

    fn bad() -> Error {
        Error::BadRequest {
            status: 400,
            code: error_codes::VALIDATION_ERROR.into(),
            message: "x".into(),
            request_id: None,
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
    fn is_rate_limit_error_matches_only_rate_limited() {
        assert!(rate().is_rate_limit_error());
        assert!(!auth().is_rate_limit_error());
        assert!(!api(429).is_rate_limit_error()); // Api variant, not RateLimited
    }

    #[test]
    fn is_validation_error_matches_only_bad_request() {
        assert!(bad().is_validation_error());
        assert!(!api(400).is_validation_error());
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
        assert_eq!(rate().code(), error_codes::QUOTA_EXCEEDED);
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
    fn display_for_aborted_is_terse() {
        assert_eq!(Error::Aborted.to_string(), "request was aborted");
    }
}
