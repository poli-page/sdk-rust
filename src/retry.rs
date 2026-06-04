//! Retry-loop event surfaced through the `on_retry` hook (spec §10.2).

use std::time::Duration;

use crate::Error;

/// Payload passed to the `on_retry` hook before each retry sleep.
///
/// Mirrors Node's `RetryEvent` (`types.ts:183-187`).
#[derive(Debug, Clone)]
pub struct RetryEvent {
    /// The attempt number about to be made (1-based). The first retry is `2`.
    pub attempt: u32,
    /// The sleep duration before this attempt starts.
    pub delay: Duration,
    /// The error that triggered the retry. Cloned for the event — the
    /// `Connection` and `Download` variants' boxed `source` is dropped on
    /// clone (see [`crate::Error`]'s `Clone` impl); `message`, `code`,
    /// `status`, and `request_id` survive.
    pub reason: Error,
}

/// Payload passed to the `on_request` hook just before each HTTP attempt is
/// dispatched (including retries).
///
/// Mirrors Node's `RequestEvent` (`types.ts:171-175`). The `attempt` field is
/// 1-based: the initial try is `1`, the first retry is `2`, etc. — matching
/// Node `index.ts:186-190`.
#[derive(Debug, Clone)]
pub struct RequestEvent {
    /// The HTTP method about to be issued (e.g. `"GET"`, `"POST"`, `"DELETE"`).
    pub method: String,
    /// The fully-resolved URL the request will hit, including the base URL.
    pub url: String,
    /// 1-based attempt counter. `1` is the initial try; `2..=max_retries+1`
    /// are subsequent retries.
    pub attempt: u32,
}

/// Payload passed to the `on_response` hook once a successful (2xx) response
/// has been received and its body has been fully read.
///
/// Mirrors Node's `ResponseEvent` (`types.ts:177-181`).
#[derive(Debug, Clone)]
pub struct ResponseEvent {
    /// HTTP status code (always in the 200..=299 range — the hook does not
    /// fire for error responses; those go through `on_retry` / `on_error`).
    pub status: u16,
    /// `X-Request-Id` header value when the server provides one. None
    /// otherwise.
    pub request_id: Option<String>,
    /// Wall-clock time in milliseconds between issuing the request and
    /// finishing the body read.
    pub duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_event_clones_with_owned_reason() {
        let evt = RetryEvent {
            attempt: 2,
            delay: Duration::from_millis(500),
            reason: Error::Timeout {
                timeout: Duration::from_secs(60),
            },
        };
        let cloned = evt.clone();
        assert_eq!(cloned.attempt, 2);
        assert_eq!(cloned.delay, Duration::from_millis(500));
        assert!(matches!(cloned.reason, Error::Timeout { .. }));
    }

    #[test]
    fn retry_event_carries_request_id_through_clone() {
        let evt = RetryEvent {
            attempt: 3,
            delay: Duration::from_millis(1000),
            reason: Error::Api {
                status: 503,
                code: "INTERNAL".into(),
                message: "boom".into(),
                request_id: Some("req_xyz".into()),
            },
        };
        let cloned = evt.clone();
        assert_eq!(cloned.reason.request_id(), Some("req_xyz"));
    }

    #[test]
    fn retry_event_clone_drops_connection_source() {
        // Spec §10.2: the cloned reason loses the boxed source.
        let original = Error::Connection {
            message: "dns".into(),
            source: Box::<dyn std::error::Error + Send + Sync>::from("real source"),
        };
        let cloned = original.clone();
        // Both still display the message.
        assert!(cloned.to_string().contains("dns"));
        // The cloned `source` exists but is a placeholder — verify by formatting
        // both and confirming the source string of the clone references the
        // "dropped" marker.
        if let Error::Connection { source, .. } = &cloned {
            assert!(source.to_string().contains("dropped on clone"));
        } else {
            panic!("expected Connection");
        }
    }

    #[test]
    fn request_event_carries_method_url_attempt() {
        let evt = RequestEvent {
            method: "POST".into(),
            url: "https://api.poli.page/v1/render/document".into(),
            attempt: 1,
        };
        let cloned = evt.clone();
        assert_eq!(cloned.method, "POST");
        assert_eq!(cloned.url, "https://api.poli.page/v1/render/document");
        assert_eq!(cloned.attempt, 1);
    }

    #[test]
    fn response_event_carries_status_request_id_duration() {
        let evt = ResponseEvent {
            status: 200,
            request_id: Some("req_abc".into()),
            duration_ms: 42,
        };
        let cloned = evt.clone();
        assert_eq!(cloned.status, 200);
        assert_eq!(cloned.request_id.as_deref(), Some("req_abc"));
        assert_eq!(cloned.duration_ms, 42);
    }

    #[test]
    fn response_event_request_id_is_optional() {
        let evt = ResponseEvent {
            status: 204,
            request_id: None,
            duration_ms: 0,
        };
        assert!(evt.request_id.is_none());
    }
}
