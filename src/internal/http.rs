//! Pure transport helpers. No I/O — every function here is a pure mapping from
//! inputs to outputs (or to a `Result`). The orchestrator in `src/client.rs`
//! (Phase 2) composes these into actual HTTP calls.
//!
//! Behavior parity with the Node SDK's `src/internal/http.ts` is mandatory;
//! the inline tests below port `tests/internal/http.test.ts` 1:1.
//!
//! Per spec §14.1, internal `pub(crate)` helpers get their tests inside the
//! module (`#[cfg(test)] mod tests`) — external `tests/*.rs` crates can't
//! see `pub(crate)` items. Plan §13 Phase 1 references `tests/unit_http.rs`
//! but §14.1's rule wins; the file lives inline here for visibility reasons.

use std::time::{Duration, SystemTime};

use http::{header, HeaderMap, HeaderValue, Method};
use url::Url;

use crate::internal::constants::{HEADER_IDEMPOTENCY_KEY, RETRY_AFTER_CAP};

/// Parse the `Retry-After` response header. Accepts either an integer number
/// of seconds or an HTTP-date. Returns the delay, capped at `RETRY_AFTER_CAP`
/// (30 s). Returns `None` for empty or unparseable inputs. Past dates yield
/// `Some(Duration::ZERO)` — the caller decides whether to retry immediately
/// or skip the sleep entirely.
pub(crate) fn parse_retry_after(header_value: &str) -> Option<Duration> {
    if header_value.is_empty() {
        return None;
    }
    if let Ok(secs) = header_value.parse::<i64>() {
        let clamped = secs.max(0) as u64;
        return Some(Duration::from_secs(clamped).min(RETRY_AFTER_CAP));
    }
    let when = httpdate::parse_http_date(header_value).ok()?;
    let now = SystemTime::now();
    let delta = when.duration_since(now).unwrap_or(Duration::ZERO);
    Some(delta.min(RETRY_AFTER_CAP))
}

/// Compute the delay before the next retry attempt. When `retry_after` is
/// `Some`, return it verbatim (the server was explicit — no jitter). Otherwise
/// apply exponential backoff `base_delay * 2^(attempt - 1)` multiplied by a
/// jitter factor sampled uniformly from `[0.5, 1.5)`.
///
/// `attempt` is 1-based: `1` means the first retry.
pub(crate) fn compute_backoff(
    attempt: u32,
    base_delay: Duration,
    retry_after: Option<Duration>,
) -> Duration {
    if let Some(d) = retry_after {
        return d;
    }
    let jitter = 0.5 + rand::random::<f64>();
    compute_backoff_with_jitter(attempt, base_delay, jitter)
}

/// Deterministic core of [`compute_backoff`]. Exposed `pub(crate)` so the math
/// can be tested without mocking the global RNG.
pub(crate) fn compute_backoff_with_jitter(
    attempt: u32,
    base_delay: Duration,
    jitter_factor: f64,
) -> Duration {
    let exp_multiplier = 2u32.saturating_pow(attempt.saturating_sub(1));
    base_delay
        .saturating_mul(exp_multiplier)
        .mul_f64(jitter_factor)
}

/// Parse a non-2xx response body into a `(code, message)` pair.
///
/// Mirrors the Node SDK's fallback chain `code → message → error →
/// "unknown_error"` (see `src/internal/http.ts:42-58`). Bodies that aren't
/// valid JSON (HTML error pages, empty responses) collapse to
/// `INTERNAL_ERROR` with a status-bearing message.
pub(crate) fn parse_error_body(body: &[u8], status: u16) -> (String, String) {
    #[derive(serde::Deserialize)]
    struct ErrorBody {
        code: Option<String>,
        detail: Option<String>,
        title: Option<String>,
        message: Option<String>,
        error: Option<String>,
    }

    // RFC 7807: prefer `detail` (specific reason) over `title` (generic name)
    // over the legacy `message` field; fall back to a short canned status string.
    // The code is verbatim from the API — never inferred from message.
    match serde_json::from_slice::<ErrorBody>(body) {
        Ok(b) => {
            let code = b
                .code
                .or(b.error)
                .unwrap_or_else(|| "unknown_error".to_string());
            let message = b
                .detail
                .or(b.title)
                .or(b.message)
                .unwrap_or_else(|| format!("HTTP {status}"));
            (code, message)
        }
        Err(_) => (
            "INTERNAL_ERROR".to_string(),
            format!("HTTP {status}: response body was not valid JSON"),
        ),
    }
}

/// Build the standard request headers (spec §5.3).
///
/// `POST` requests get `Content-Type: application/json` and an
/// `Idempotency-Key` when one is supplied. `GET` and `DELETE` skip both.
/// `Accept`, `Authorization`, and `User-Agent` are set on every request.
///
/// `user_agent` is supplied by the caller rather than built here so this
/// module stays free of `env!("CARGO_PKG_VERSION")` (mirrors the Node SDK's
/// rationale at `internal/http.ts:66`).
pub(crate) fn build_headers(
    method: &Method,
    api_key: &str,
    idempotency_key: Option<&str>,
    user_agent: &str,
) -> HeaderMap {
    let mut h = HeaderMap::with_capacity(5);
    h.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    // Build the Bearer value defensively — caller-supplied api_keys could
    // contain stray bytes that would panic HeaderValue::from_str. We surface
    // the failure as an empty-ish header rather than a panic; the request
    // will be rejected upstream with a clearer error.
    if let Ok(auth) = HeaderValue::from_str(&format!("Bearer {api_key}")) {
        h.insert(header::AUTHORIZATION, auth);
    }
    if let Ok(ua) = HeaderValue::from_str(user_agent) {
        h.insert(header::USER_AGENT, ua);
    }
    if method == Method::POST {
        h.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        if let Some(key) = idempotency_key {
            if let Ok(v) = HeaderValue::from_str(key) {
                h.insert(HEADER_IDEMPOTENCY_KEY, v);
            }
        }
    }
    h
}

/// Build the absolute request URL from a base URL and an absolute path.
///
/// Semantics mirror the Node SDK's `${baseUrl}${path}` string concat — base is
/// treated as a fixed prefix (including any path component, e.g., when the
/// API sits behind a reverse-proxy path), `path` is the suffix.
///
/// `url::Url::join` is intentionally NOT used: per RFC 3986 an absolute `path`
/// (starting with `/`) replaces the base's path entirely, which silently
/// breaks deployments where the base carries a path prefix.
pub(crate) fn build_url(base: &Url, path: &str) -> Result<Url, url::ParseError> {
    let prefix = base.as_str().trim_end_matches('/');
    Url::parse(&format!("{prefix}{path}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // --- parse_retry_after: port of http.test.ts lines 9-52 ---

    #[test]
    fn parse_retry_after_returns_none_for_empty_string() {
        assert_eq!(parse_retry_after(""), None);
    }

    #[test]
    fn parse_retry_after_returns_zero_for_zero_string() {
        assert_eq!(parse_retry_after("0"), Some(Duration::ZERO));
    }

    #[test]
    fn parse_retry_after_returns_5s_for_five_string() {
        assert_eq!(parse_retry_after("5"), Some(Duration::from_secs(5)));
    }

    #[test]
    fn parse_retry_after_caps_large_second_values_at_30s() {
        assert_eq!(parse_retry_after("999"), Some(Duration::from_secs(30)));
        assert_eq!(parse_retry_after("100000"), Some(Duration::from_secs(30)));
    }

    #[test]
    fn parse_retry_after_returns_none_for_non_numeric_non_date() {
        assert_eq!(parse_retry_after("abc"), None);
        assert_eq!(parse_retry_after("not a date"), None);
    }

    #[test]
    fn parse_retry_after_returns_zero_for_past_http_date() {
        // 60s in the past — rendered as an RFC 7231 IMF-fixdate.
        let past = httpdate::fmt_http_date(std::time::SystemTime::now() - Duration::from_secs(60));
        assert_eq!(parse_retry_after(&past), Some(Duration::ZERO));
    }

    #[test]
    fn parse_retry_after_returns_delta_for_future_http_date() {
        // 5s in the future — expect roughly 5s back, clamped above 3s.
        let future = httpdate::fmt_http_date(std::time::SystemTime::now() + Duration::from_secs(5));
        let result = parse_retry_after(&future).expect("should parse");
        assert!(
            result > Duration::from_secs(3) && result <= Duration::from_secs(5),
            "result was {result:?}",
        );
    }

    #[test]
    fn parse_retry_after_caps_far_future_http_date_at_30s() {
        let far_future =
            httpdate::fmt_http_date(std::time::SystemTime::now() + Duration::from_secs(60 * 60));
        assert_eq!(
            parse_retry_after(&far_future),
            Some(Duration::from_secs(30))
        );
    }

    // --- compute_backoff: port of http.test.ts lines 54-106 ---
    //
    // Node mocks `Math.random`. Rust has no global RNG to mock cleanly, so we
    // split the math into `compute_backoff_with_jitter(attempt, base, jitter)`
    // and have `compute_backoff` call it with `0.5 + rand::random()`. The
    // math tests target the deterministic helper; the property test ([0.5,
    // 1.5) bounds over 200 samples) targets the wrapper.

    const BASE: Duration = Duration::from_millis(500);

    #[test]
    fn compute_backoff_returns_retry_after_as_is_when_some() {
        assert_eq!(
            compute_backoff(1, BASE, Some(Duration::from_secs(1))),
            Duration::from_secs(1),
        );
        assert_eq!(
            compute_backoff(3, BASE, Some(Duration::from_millis(250))),
            Duration::from_millis(250),
        );
    }

    #[test]
    fn compute_backoff_returns_zero_when_retry_after_is_zero() {
        // Mirrors Node's "treats falsy 0 as defined": Some(0) is honored,
        // not treated as "no value given".
        assert_eq!(
            compute_backoff(1, BASE, Some(Duration::ZERO)),
            Duration::ZERO,
        );
    }

    #[test]
    fn compute_backoff_with_jitter_applies_exponential_backoff() {
        // jitter = 0.5 (minimum)
        assert_eq!(
            compute_backoff_with_jitter(1, BASE, 0.5),
            Duration::from_millis(250), // 500 * 1 * 0.5
        );
        assert_eq!(
            compute_backoff_with_jitter(2, BASE, 0.5),
            Duration::from_millis(500), // 500 * 2 * 0.5
        );
        assert_eq!(
            compute_backoff_with_jitter(3, BASE, 0.5),
            Duration::from_millis(1000), // 500 * 4 * 0.5
        );
    }

    #[test]
    fn compute_backoff_with_jitter_applies_max_jitter() {
        // jitter ≈ 1.499 (matches Node's mocked Math.random = 0.999)
        let result = compute_backoff_with_jitter(1, BASE, 1.499);
        let ms = result.as_millis() as f64;
        // 500 * 1 * 1.499 = 749.5
        assert!((740.0..=760.0).contains(&ms), "result was {ms}ms");
    }

    #[test]
    fn compute_backoff_jitter_factor_stays_within_bounds_over_200_samples() {
        // Port http.test.ts:83-93 — the wrapper's jitter must produce delays
        // in [base * 0.5, base * 1.5] regardless of the underlying RNG draw.
        for _ in 0..200 {
            let d = compute_backoff(1, Duration::from_secs(1), None);
            assert!(d >= Duration::from_millis(500), "delay was {d:?}");
            assert!(d <= Duration::from_millis(1500), "delay was {d:?}");
        }
    }

    // --- parse_error_body: port of http.test.ts lines 108-152 ---

    #[test]
    fn parse_error_body_extracts_code_and_message_from_complete_json() {
        let (code, message) = parse_error_body(
            br#"{"code":"VALIDATION_ERROR","message":"data is required"}"#,
            400,
        );
        assert_eq!(code, "VALIDATION_ERROR");
        assert_eq!(message, "data is required");
    }

    #[test]
    fn parse_error_body_code_stays_unknown_when_only_message_present() {
        let (code, message) = parse_error_body(br#"{"message":"something broke"}"#, 400);
        assert_eq!(code, "unknown_error");
        assert_eq!(message, "something broke");
    }

    #[test]
    fn parse_error_body_falls_back_to_error_field_as_code() {
        let (code, message) = parse_error_body(br#"{"error":"oops"}"#, 400);
        assert_eq!(code, "oops");
        assert_eq!(message, "HTTP 400");
    }

    #[test]
    fn parse_error_body_returns_unknown_error_when_json_has_no_recognised_fields() {
        let (code, message) = parse_error_body(b"{}", 400);
        assert_eq!(code, "unknown_error");
        assert_eq!(message, "HTTP 400");
    }

    #[test]
    fn parse_error_body_returns_internal_error_for_non_json_body() {
        let (code, message) = parse_error_body(b"not json", 502);
        assert_eq!(code, "INTERNAL_ERROR");
        assert_eq!(message, "HTTP 502: response body was not valid JSON");
    }

    #[test]
    fn parse_error_body_uses_rfc7807_detail_as_message() {
        let (code, message) = parse_error_body(
            br#"{"code":"authentication_failed","detail":"Forbidden","title":"Authentication failed"}"#,
            401,
        );
        assert_eq!(code, "authentication_failed");
        assert_eq!(message, "Forbidden");
    }

    #[test]
    fn parse_error_body_falls_back_to_title_when_detail_absent() {
        let (code, message) =
            parse_error_body(br#"{"code":"forbidden","title":"Access denied"}"#, 403);
        assert_eq!(code, "forbidden");
        assert_eq!(message, "Access denied");
    }

    #[test]
    fn parse_error_body_does_not_synthesise_api_error_prefix() {
        let (_, message) = parse_error_body(br#"{"code":"THUMBNAILS_NOT_AVAILABLE"}"#, 403);
        assert!(!message.contains("API error"), "message was {message}");
        assert_eq!(message, "HTTP 403");
    }

    #[test]
    fn parse_error_body_returns_internal_error_for_html_error_page() {
        let (code, message) = parse_error_body(b"<html>upstream gone</html>", 502);
        assert_eq!(code, "INTERNAL_ERROR");
        assert!(message.contains("502"), "message was {message}");
    }

    #[test]
    fn parse_error_body_returns_internal_error_for_empty_body() {
        let (code, _) = parse_error_body(b"", 500);
        assert_eq!(code, "INTERNAL_ERROR");
    }

    // --- build_headers: port of http.test.ts lines 155-195 ---
    //
    // The Rust signature drops Node's unused `path` parameter (the Node
    // comment at http.ts:74-77 calls it "intentionally unused"). Method is
    // typed via `http::Method` rather than a string literal — invalid methods
    // become un-typeable instead of a runtime panic.

    const UA: &str = "poli-page-sdk-rust/1.0.0";

    fn get(h: &HeaderMap, name: impl header::AsHeaderName) -> Option<&HeaderValue> {
        h.get(name)
    }

    #[test]
    fn build_headers_always_sets_accept_application_json() {
        let h = build_headers(&Method::POST, "pp_test_x", Some("idem-1"), UA);
        assert_eq!(get(&h, header::ACCEPT).unwrap(), "application/json");
    }

    #[test]
    fn build_headers_post_sets_content_type_application_json() {
        let h = build_headers(&Method::POST, "pp_test_x", Some("idem-1"), UA);
        assert_eq!(get(&h, header::CONTENT_TYPE).unwrap(), "application/json");
    }

    #[test]
    fn build_headers_sets_authorization_with_bearer_prefix() {
        let h = build_headers(&Method::POST, "pp_test_xyz", Some("idem-1"), UA);
        assert_eq!(
            get(&h, header::AUTHORIZATION).unwrap(),
            "Bearer pp_test_xyz"
        );
    }

    #[test]
    fn build_headers_sets_user_agent_verbatim() {
        let h = build_headers(
            &Method::POST,
            "pp_test_x",
            Some("idem-1"),
            "custom-ua/9.9.9",
        );
        assert_eq!(get(&h, header::USER_AGENT).unwrap(), "custom-ua/9.9.9");
    }

    #[test]
    fn build_headers_sets_idempotency_key_from_arg_when_post() {
        let h = build_headers(&Method::POST, "pp_test_x", Some("idem-abc-123"), UA);
        assert_eq!(get(&h, HEADER_IDEMPOTENCY_KEY).unwrap(), "idem-abc-123");
    }

    #[test]
    fn build_headers_get_omits_content_type_and_idempotency_key() {
        let h = build_headers(&Method::GET, "pp_test_x", None, UA);
        assert!(get(&h, header::CONTENT_TYPE).is_none());
        assert!(get(&h, HEADER_IDEMPOTENCY_KEY).is_none());
        // auth + UA + Accept still present
        assert_eq!(get(&h, header::AUTHORIZATION).unwrap(), "Bearer pp_test_x");
        assert_eq!(get(&h, header::USER_AGENT).unwrap(), UA);
        assert_eq!(get(&h, header::ACCEPT).unwrap(), "application/json");
    }

    #[test]
    fn build_headers_delete_omits_content_type_and_idempotency_key() {
        let h = build_headers(&Method::DELETE, "pp_test_x", None, UA);
        assert!(get(&h, header::CONTENT_TYPE).is_none());
        assert!(get(&h, HEADER_IDEMPOTENCY_KEY).is_none());
    }

    #[test]
    fn build_headers_post_with_no_idempotency_key_still_omits_it() {
        // Spec §5.3 — Idempotency-Key is auto-generated by the orchestrator
        // unless supplied. `build_headers` itself only sets it when the
        // caller passes Some(_), so a None call (orchestrator-internal use)
        // doesn't surprise-set anything.
        let h = build_headers(&Method::POST, "pp_test_x", None, UA);
        assert!(get(&h, HEADER_IDEMPOTENCY_KEY).is_none());
        assert_eq!(get(&h, header::CONTENT_TYPE).unwrap(), "application/json");
    }

    // --- build_url: Rust-specific (Node does inline string concat) ---
    //
    // Node's `${baseUrl}${path}` semantics: base is treated as a fixed prefix,
    // path as the suffix. We mirror that — `Url::join` would silently swallow
    // a base path prefix when the relative starts with `/` (RFC 3986), so we
    // can't use it.

    use url::Url;

    fn base(s: &str) -> Url {
        Url::parse(s).expect("test base url is valid")
    }

    #[test]
    fn build_url_joins_default_base_and_path() {
        let u = build_url(&base("https://api.poli.page"), "/v1/render").unwrap();
        assert_eq!(u.as_str(), "https://api.poli.page/v1/render");
    }

    #[test]
    fn build_url_strips_trailing_slash_on_base_to_avoid_double_slash() {
        let u = build_url(&base("https://api.poli.page/"), "/v1/render").unwrap();
        assert_eq!(u.as_str(), "https://api.poli.page/v1/render");
    }

    #[test]
    fn build_url_preserves_base_path_prefix() {
        // A custom base with a path prefix (e.g., behind a reverse proxy)
        // must keep that prefix — this is the contract Node ships and the
        // reason we don't use `Url::join` (which would strip the prefix).
        let u = build_url(&base("https://gw.example.com/poli"), "/v1/render").unwrap();
        assert_eq!(u.as_str(), "https://gw.example.com/poli/v1/render");
    }

    #[test]
    fn build_url_preserves_url_encoded_path_segments() {
        // Document IDs may contain characters that get percent-encoded.
        let u = build_url(
            &base("https://api.poli.page"),
            "/v1/documents/doc%20with%20space",
        )
        .unwrap();
        assert_eq!(
            u.as_str(),
            "https://api.poli.page/v1/documents/doc%20with%20space",
        );
    }

    #[test]
    fn build_url_handles_path_with_query_string() {
        let u = build_url(&base("https://api.poli.page"), "/v1/render?stream=1").unwrap();
        assert_eq!(u.as_str(), "https://api.poli.page/v1/render?stream=1");
    }
}
