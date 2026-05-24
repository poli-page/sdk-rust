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
}
