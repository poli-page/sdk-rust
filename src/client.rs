//! Client construction, configuration, transport seam, and the retry loop.
//!
//! Ports the orchestration in Node `src/index.ts:69-260` (`PoliPage` class,
//! `#runWithRetry`, `#sendOnce`, `#fireHook`).

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::Bytes;
use futures_core::Stream;
use http::{HeaderMap, Method};
use tracing::{debug, warn};
use url::Url;

use crate::documents::Documents;
use crate::internal::constants::{
    DEFAULT_BASE_URL, DEFAULT_MAX_RETRIES, DEFAULT_RETRY_DELAY, DEFAULT_TIMEOUT, HEADER_REQUEST_ID,
};
use crate::internal::http::{
    build_headers, build_url, compute_backoff, parse_error_body, parse_retry_after,
};
use crate::internal::uuid::new_v4_string;
use crate::render::Render;
use crate::Error;
use crate::{RequestEvent, ResponseEvent, RetryEvent};

/// Resolved client options. Owned by `ClientInner` behind an `Arc` so cloning
/// the public `PoliPage` is just a refcount bump.
#[derive(Debug, Clone)]
pub(crate) struct ClientConfig {
    pub(crate) api_key: String,
    pub(crate) base_url: Url,
    pub(crate) max_retries: u32,
    pub(crate) retry_delay: Duration,
    pub(crate) timeout: Duration,
    pub(crate) user_agent: String,
}

type RetryHook = Arc<dyn Fn(&RetryEvent) + Send + Sync>;
type ErrorHook = Arc<dyn Fn(&Error) + Send + Sync>;
type RequestHook = Arc<dyn Fn(&RequestEvent) + Send + Sync>;
type ResponseHook = Arc<dyn Fn(&ResponseEvent) + Send + Sync>;

/// Shared client state. Held by `Arc` in every public handle so cloning
/// `PoliPage` (or any namespace handle) costs an atomic increment, not a
/// reqwest::Client + config copy.
pub(crate) struct ClientInner {
    pub(crate) config: ClientConfig,
    pub(crate) http: reqwest::Client,
    pub(crate) on_retry: Option<RetryHook>,
    pub(crate) on_error: Option<ErrorHook>,
    pub(crate) on_request: Option<RequestHook>,
    pub(crate) on_response: Option<ResponseHook>,
}

impl std::fmt::Debug for ClientInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientInner")
            .field("config", &self.config)
            // hooks are `dyn Fn` — not meaningfully Debug-formattable.
            .field("on_retry", &self.on_retry.as_ref().map(|_| "<fn>"))
            .field("on_error", &self.on_error.as_ref().map(|_| "<fn>"))
            .field("on_request", &self.on_request.as_ref().map(|_| "<fn>"))
            .field("on_response", &self.on_response.as_ref().map(|_| "<fn>"))
            .finish_non_exhaustive()
    }
}

/// Description of a single HTTP attempt as the namespace methods see it. The
/// orchestrator handles auth, retries, idempotency, hooks, and per-attempt
/// timeouts on top of this.
#[derive(Debug)]
pub(crate) struct HttpAttempt<'a> {
    pub method: Method,
    pub path: &'a str,
    pub body: Option<&'a serde_json::Value>,
    pub idempotency_key: Option<&'a str>,
    /// Per-call timeout override; `None` falls back to `ClientConfig::timeout`.
    pub timeout: Option<Duration>,
}

/// The parsed wire response — bytes are buffered into `bytes::Bytes` once so
/// downstream parsing (JSON, text/html, future streaming) can pick whichever
/// view it needs.
#[derive(Debug)]
pub(crate) struct ParsedResponse {
    pub status: u16,
    /// Response headers. Phase 4's `documents.preview` reads
    /// `X-Document-Page-Count` from here.
    #[allow(dead_code)]
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl ClientInner {
    /// Run an HTTP attempt through the retry loop.
    ///
    /// Mirrors Node `index.ts:128-170` `#runWithRetry` plus `#sendOnce`.
    pub(crate) async fn execute(&self, attempt: HttpAttempt<'_>) -> Result<ParsedResponse, Error> {
        let mut last_error: Option<Error> = None;
        let mut next_retry_after: Option<Duration> = None;
        let per_attempt_timeout = attempt.timeout.unwrap_or(self.config.timeout);

        // attempt 0 is the initial try; 1..=max_retries are retries.
        for attempt_idx in 0..=self.config.max_retries {
            if attempt_idx > 0 {
                let delay = compute_backoff(attempt_idx, self.config.retry_delay, next_retry_after);
                if let Some(err) = last_error.as_ref() {
                    fire_retry_hook(
                        self.on_retry.as_ref(),
                        &RetryEvent {
                            // Node fires the hook with `attempt + 1` (1-based "about to be").
                            attempt: attempt_idx + 1,
                            delay,
                            reason: err.clone(),
                        },
                    );
                }
                sleep_at_least(delay).await;
            }

            match self
                .send_once(&attempt, per_attempt_timeout, attempt_idx + 1)
                .await
            {
                SendResult::Ok(resp) => return Ok(resp),
                SendResult::Err { error, retry_after } => {
                    if !error.is_retryable() {
                        fire_error_hook(self.on_error.as_ref(), &error);
                        return Err(error);
                    }
                    last_error = Some(error);
                    next_retry_after = retry_after;
                }
            }
        }

        let err = last_error.expect("retry loop ran at least once");
        fire_error_hook(self.on_error.as_ref(), &err);
        Err(err)
    }

    /// Fetch the bytes at a presigned URL. Unauthenticated, single attempt,
    /// no retries — per spec §9.1 the second-hop storage fetch sits outside
    /// the SDK's retry policy.
    ///
    /// Errors map to [`Error::Download`]: `Failed to download PDF: HTTP
    /// <status>` for a non-2xx answer, `Failed to download PDF: network
    /// error` (the transport error as `source`) when the storage does not
    /// answer or the body fails while it is read.
    pub(crate) async fn fetch_bytes(&self, url: &str) -> Result<Bytes, Error> {
        let response = self.start_download(url).await?;
        response.bytes().await.map_err(download_network_error)
    }

    /// Stream the bytes at a presigned URL chunk-by-chunk. Same auth/retry
    /// posture as [`fetch_bytes`].
    ///
    /// Returns once the response headers arrive (so non-2xx is reported
    /// before any bytes are read); subsequent reads on the returned stream
    /// surface chunk-level errors as [`Error::Download`].
    pub(crate) async fn stream_bytes(&self, url: &str) -> Result<PdfByteStream, Error> {
        let response = self.start_download(url).await?;
        Ok(PdfByteStream {
            inner: Box::pin(response.bytes_stream()),
        })
    }

    async fn start_download(&self, url: &str) -> Result<reqwest::Response, Error> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(download_network_error)?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(Error::Download {
                message: format!("Failed to download PDF: HTTP {status}"),
                status: Some(status),
                source: None,
            });
        }
        Ok(response)
    }
}

/// A presigned-URL download that got no answer, or whose body failed while
/// it was read: `DOWNLOAD_FAILED` without a status, the transport error as
/// `source`.
fn download_network_error(err: reqwest::Error) -> Error {
    Error::Download {
        message: "Failed to download PDF: network error".to_owned(),
        status: None,
        source: Some(Box::new(err)),
    }
}

/// Adapter that maps a reqwest byte stream's errors into our
/// [`Error::Download`] variant — `futures_core::Stream` only, so consumers
/// pull in `futures::StreamExt` (or hand-implement) to consume it.
pub struct PdfByteStream {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
}

impl std::fmt::Debug for PdfByteStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfByteStream").finish_non_exhaustive()
    }
}

impl Stream for PdfByteStream {
    type Item = Result<Bytes, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.inner.as_mut().poll_next(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(Ok(bytes))),
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(download_network_error(e)))),
        }
    }
}

impl ClientInner {
    #[allow(clippy::too_many_lines)]
    async fn send_once(
        &self,
        attempt: &HttpAttempt<'_>,
        per_attempt_timeout: Duration,
        attempt_idx_one_based: u32,
    ) -> SendResult {
        let url = match build_url(&self.config.base_url, attempt.path) {
            Ok(u) => u,
            Err(e) => {
                return SendResult::Err {
                    error: Error::InvalidOptions {
                        message: format!("could not build url for {}: {e}", attempt.path),
                    },
                    retry_after: None,
                };
            }
        };
        let headers = build_headers(
            &attempt.method,
            &self.config.api_key,
            attempt.idempotency_key,
            &self.config.user_agent,
        );

        let span = tracing::debug_span!(
            "polipage.request",
            method = %attempt.method,
            url = %url,
            request_id = tracing::field::Empty,
        );
        let _enter = span.enter();
        debug!("sending request");

        let mut req = self.http.request(attempt.method.clone(), url.clone());
        req = req.headers(headers);
        if let Some(b) = attempt.body {
            req = req.json(b);
        }

        fire_request_hook(
            self.on_request.as_ref(),
            &RequestEvent {
                method: attempt.method.to_string(),
                url: url.to_string(),
                attempt: attempt_idx_one_based,
            },
        );

        // One deadline bounds the whole attempt, whatever client was
        // injected: connecting, sending, the answer's head and its whole
        // body, so a body that drips slowly cannot outlive the timeout.
        let start = tokio::time::Instant::now();
        let deadline = start + per_attempt_timeout;
        let response = match tokio::time::timeout_at(deadline, req.send()).await {
            Err(_elapsed) => {
                return SendResult::Err {
                    error: Error::Timeout {
                        timeout: per_attempt_timeout,
                    },
                    retry_after: None,
                };
            }
            Ok(Err(e)) => {
                return SendResult::Err {
                    error: classify_reqwest_error(e, per_attempt_timeout, start.elapsed()),
                    retry_after: None,
                };
            }
            Ok(Ok(r)) => r,
        };

        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let request_id = headers
            .get(HEADER_REQUEST_ID)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        if let Some(rid) = &request_id {
            span.record("request_id", rid.as_str());
        }

        // The body is read inside the attempt, under the same deadline.
        let body = match tokio::time::timeout_at(deadline, response.bytes()).await {
            Ok(Ok(b)) => Ok(b),
            Ok(Err(e)) => Err(BodyFailure::Read(e)),
            Err(elapsed) => Err(BodyFailure::Timeout(elapsed)),
        };

        if (200..300).contains(&status) {
            let body = match body {
                Ok(b) => b,
                // A 2xx answer cut off mid-body is this attempt's
                // `network_error`, a body that outlasts the deadline its
                // `timeout`: both retried (spec §9.1).
                Err(BodyFailure::Timeout(_)) => {
                    return SendResult::Err {
                        error: Error::Timeout {
                            timeout: per_attempt_timeout,
                        },
                        retry_after: None,
                    };
                }
                Err(BodyFailure::Read(e)) => {
                    return SendResult::Err {
                        error: classify_reqwest_error(e, per_attempt_timeout, start.elapsed()),
                        retry_after: None,
                    };
                }
            };
            let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
            fire_response_hook(
                self.on_response.as_ref(),
                &ResponseEvent {
                    status,
                    request_id: request_id.clone(),
                    duration_ms,
                },
            );
            debug!(status, "received response");
            return SendResult::Ok(ParsedResponse {
                status,
                headers,
                body,
            });
        }

        // Non-2xx. A body that failed while it was read (dropped, or past
        // the deadline) is a malformed error body (spec §8.2): never a code
        // taken from the partial bytes, the real status and request id kept,
        // the read error as `source`, retried by its status.
        let (code, message, source) = match body {
            Ok(b) => {
                let (code, message) = parse_error_body(&b, status);
                (code, message, None)
            }
            Err(failure) => (
                crate::error::error_codes::INTERNAL_ERROR.to_owned(),
                format!("HTTP {status}"),
                Some(failure.into_source()),
            ),
        };
        let error = classify_api_error(status, code, message, request_id, source);
        // One rule for the retry loop and for `Error::is_retryable`. Only
        // `Retry-After` is read; `Retry-After-Ms` is ignored.
        let retry_after = if error.is_retryable() {
            warn!(status, code = error.code(), "retryable api error");
            headers
                .get(http::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(parse_retry_after)
        } else {
            None
        };
        SendResult::Err { error, retry_after }
    }
}

/// Why an answer's body could not be read in full.
enum BodyFailure {
    Read(reqwest::Error),
    Timeout(tokio::time::error::Elapsed),
}

impl BodyFailure {
    fn into_source(self) -> Box<dyn std::error::Error + Send + Sync> {
        match self {
            BodyFailure::Read(e) => Box::new(e),
            BodyFailure::Timeout(e) => Box::new(e),
        }
    }
}

enum SendResult {
    Ok(ParsedResponse),
    Err {
        /// Retried when [`Error::is_retryable`] says so.
        error: Error,
        retry_after: Option<Duration>,
    },
}

/// Map a `reqwest::Error` to our `Error` enum: `network_error` with the
/// spec's message and the reqwest error as `source`, or `timeout` when a
/// timeout of an injected `reqwest::Client` fired before the SDK's own. The
/// timeout then names the bound that was hit: the time the attempt ran.
fn classify_reqwest_error(err: reqwest::Error, attempt_timeout: Duration, ran: Duration) -> Error {
    if err.is_timeout() {
        let ran = Duration::from_millis(u64::try_from(ran.as_millis()).unwrap_or(u64::MAX));
        return Error::Timeout {
            timeout: ran.min(attempt_timeout),
        };
    }
    Error::Connection {
        message: "Network request failed".to_owned(),
        source: Box::new(err),
    }
}

/// Map a non-2xx status + parsed body to the most specific `Error` variant.
/// Mirrors what Node's classifier does inline at `index.ts:240`.
fn classify_api_error(
    status: u16,
    code: String,
    message: String,
    request_id: Option<String>,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
) -> Error {
    match status {
        400 | 422 => Error::BadRequest {
            status,
            code,
            message,
            request_id,
            source,
        },
        401 => Error::Auth {
            status,
            code,
            message,
            request_id,
            source,
        },
        403 => Error::PermissionDenied {
            status,
            code,
            message,
            request_id,
            source,
        },
        404 => Error::NotFound {
            status,
            code,
            message,
            request_id,
            source,
        },
        410 => Error::Gone {
            status,
            code,
            message,
            request_id,
            source,
        },
        429 => Error::RateLimited {
            status,
            code,
            message,
            request_id,
            source,
        },
        _ => Error::Api {
            status,
            code,
            message,
            request_id,
            source,
        },
    }
}

/// Invoke a retry hook, swallowing panics so a buggy callback can't break the
/// request. Mirrors Node `index.ts:106-113` `#fireHook`.
fn fire_retry_hook(hook: Option<&RetryHook>, event: &RetryEvent) {
    if let Some(h) = hook {
        let h = Arc::clone(h);
        let event = event.clone();
        let _ = catch_unwind(AssertUnwindSafe(move || h(&event)));
    }
}

fn fire_error_hook(hook: Option<&ErrorHook>, error: &Error) {
    if let Some(h) = hook {
        let h = Arc::clone(h);
        let error = error.clone();
        let _ = catch_unwind(AssertUnwindSafe(move || h(&error)));
    }
}

fn fire_request_hook(hook: Option<&RequestHook>, event: &RequestEvent) {
    if let Some(h) = hook {
        let h = Arc::clone(h);
        let event = event.clone();
        let _ = catch_unwind(AssertUnwindSafe(move || h(&event)));
    }
}

fn fire_response_hook(hook: Option<&ResponseHook>, event: &ResponseEvent) {
    if let Some(h) = hook {
        let h = Arc::clone(h);
        let event = event.clone();
        let _ = catch_unwind(AssertUnwindSafe(move || h(&event)));
    }
}

// =============================================================================
// Public surface
// =============================================================================

/// Async client for the Poli Page API.
///
/// Cheap to clone — internally an `Arc<ClientInner>`. The recommended pattern
/// is to build one client at process startup and clone it freely into request
/// handlers and spawned tasks (the underlying `reqwest::Client` pools
/// connections across all clones).
///
/// # Quick start
///
/// ```no_run
/// use poli_page::PoliPage;
///
/// # async fn run() -> Result<(), poli_page::Error> {
/// let client = PoliPage::new(std::env::var("POLI_PAGE_API_KEY").unwrap())?;
/// let _ = &client.render;
/// # Ok(()) }
/// ```
#[derive(Debug, Clone)]
#[must_use = "a PoliPage client is only useful through its `render` or `documents` namespaces"]
pub struct PoliPage {
    /// The `render` namespace — `pdf`, `pdf_stream`, `document`.
    pub render: Render,
    /// The `documents` namespace — `get`, `preview`, `thumbnails`, `delete`.
    pub documents: Documents,
    // Held so future namespaces can spawn from the same `Arc` without
    // re-wiring the builder.
    inner: Arc<ClientInner>,
}

impl PoliPage {
    /// Build a client with the default configuration and the given API key.
    ///
    /// For non-default options (timeout, retries, base URL, hooks) use
    /// [`PoliPage::builder`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidOptions`] when `api_key` is empty.
    pub fn new(api_key: impl Into<String>) -> Result<Self, Error> {
        Self::builder().api_key(api_key).build()
    }

    /// Begin configuring a client with the builder pattern.
    ///
    /// ```no_run
    /// use poli_page::PoliPage;
    /// # async fn run() -> Result<(), poli_page::Error> {
    /// let client = PoliPage::builder()
    ///     .api_key("pp_sandbox_...")
    ///     .max_retries(3)
    ///     .timeout(std::time::Duration::from_secs(60))
    ///     .build()?;
    /// # let _ = client; Ok(()) }
    /// ```
    #[must_use]
    pub fn builder() -> PoliPageBuilder {
        PoliPageBuilder::default()
    }

    /// Internal accessor — child namespaces clone the shared `Arc` so the
    /// `reqwest::Client` and config are shared without re-wiring the builder.
    #[doc(hidden)]
    #[allow(dead_code)] // Used by Phase 4's documents namespace.
    pub(crate) fn inner(&self) -> &Arc<ClientInner> {
        &self.inner
    }
}

/// Builder for [`PoliPage`].
///
/// Created via [`PoliPage::builder`]. Configuration setters return `Self` for
/// chaining; only [`PoliPageBuilder::build`] can fail.
#[derive(Default)]
pub struct PoliPageBuilder {
    api_key: Option<String>,
    base_url: Option<String>,
    max_retries: Option<u32>,
    retry_delay: Option<Duration>,
    timeout: Option<Duration>,
    http_client: Option<reqwest::Client>,
    on_retry: Option<RetryHook>,
    on_error: Option<ErrorHook>,
    on_request: Option<RequestHook>,
    on_response: Option<ResponseHook>,
}

impl std::fmt::Debug for PoliPageBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PoliPageBuilder")
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("base_url", &self.base_url)
            .field("max_retries", &self.max_retries)
            .field("retry_delay", &self.retry_delay)
            .field("timeout", &self.timeout)
            .field(
                "http_client",
                &self.http_client.as_ref().map(|_| "<custom>"),
            )
            .field("on_retry", &self.on_retry.as_ref().map(|_| "<fn>"))
            .field("on_error", &self.on_error.as_ref().map(|_| "<fn>"))
            .field("on_request", &self.on_request.as_ref().map(|_| "<fn>"))
            .field("on_response", &self.on_response.as_ref().map(|_| "<fn>"))
            .finish()
    }
}

impl PoliPageBuilder {
    /// Set the API key. Required — `build()` fails without it.
    #[must_use]
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.api_key = Some(api_key.into());
        self
    }

    /// Override the base URL. Defaults to `https://api.poli.page`.
    #[must_use]
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = Some(base_url.into());
        self
    }

    /// Override the maximum number of retries on retryable failures (5xx, 429,
    /// network, timeout). Defaults to `2` (3 total attempts). `0` disables.
    #[must_use]
    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = Some(max_retries);
        self
    }

    /// Override the initial retry delay. Subsequent retries apply exponential
    /// backoff and jitter on top. Defaults to 500 ms.
    #[must_use]
    pub fn retry_delay(mut self, retry_delay: Duration) -> Self {
        self.retry_delay = Some(retry_delay);
        self
    }

    /// Override the per-attempt timeout. Defaults to 60 seconds. Note: this is
    /// PER ATTEMPT — `max_retries * timeout + sleeps` is the worst-case
    /// wall-clock for a single call.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Provide a pre-built [`reqwest::Client`] for all HTTP traffic.
    ///
    /// Use when you need to share a connection pool with the rest of your
    /// application, or layer middleware (custom TLS, proxies, tracing) at
    /// the `reqwest` level — the AWS / Stripe / Azure SDK pattern.
    ///
    /// **Interaction with other setters:** the SDK's [`Self::timeout`] is
    /// enforced at the SDK layer (one `tokio` deadline per attempt, covering
    /// the connection, the request, the answer and its whole body) and
    /// applies regardless of whether you supply your own client. Any timeout
    /// configured on the supplied [`reqwest::Client`] applies on top — the
    /// effective deadline is `min(sdk_timeout, reqwest_timeout)`.
    ///
    /// **Resends:** the SDK's own client never lets reqwest resend a request
    /// (its default retry policy resends some refused HTTP/2 requests).
    /// For a client you supply, build it with
    /// `.retry(reqwest::retry::never())` too, so that every resend is one
    /// of the SDK's counted retries: the API does not deduplicate on
    /// `Idempotency-Key`.
    ///
    /// ```no_run
    /// use poli_page::PoliPage;
    /// # fn run() -> Result<(), poli_page::Error> {
    /// let http = reqwest::Client::builder()
    ///     .pool_max_idle_per_host(32)
    ///     .retry(reqwest::retry::never())
    ///     .build()
    ///     .map_err(|e| poli_page::Error::InvalidOptions {
    ///         message: format!("custom http client: {e}"),
    ///     })?;
    /// let client = PoliPage::builder()
    ///     .api_key("pp_sandbox_...")
    ///     .http_client(http)
    ///     .build()?;
    /// # let _ = client; Ok(()) }
    /// ```
    #[must_use]
    pub fn http_client(mut self, http_client: reqwest::Client) -> Self {
        self.http_client = Some(http_client);
        self
    }

    /// Register a callback fired before each retry sleep. The hook receives
    /// the upcoming attempt number, the sleep duration, and the error that
    /// triggered the retry. Panics inside the hook are swallowed.
    #[must_use]
    pub fn on_retry<F>(mut self, f: F) -> Self
    where
        F: Fn(&RetryEvent) + Send + Sync + 'static,
    {
        self.on_retry = Some(Arc::new(f));
        self
    }

    /// Register a callback fired once per terminal failure (retries exhausted,
    /// non-retryable error, or aborted). Panics inside the hook are swallowed.
    #[must_use]
    pub fn on_error<F>(mut self, f: F) -> Self
    where
        F: Fn(&Error) + Send + Sync + 'static,
    {
        self.on_error = Some(Arc::new(f));
        self
    }

    /// Register a callback fired immediately before each HTTP attempt is
    /// dispatched (including retries). The hook receives the method, the
    /// fully-resolved URL, and the 1-based attempt counter. Panics inside
    /// the hook are swallowed.
    #[must_use]
    pub fn on_request<F>(mut self, f: F) -> Self
    where
        F: Fn(&RequestEvent) + Send + Sync + 'static,
    {
        self.on_request = Some(Arc::new(f));
        self
    }

    /// Register a callback fired once per successful (2xx) response, after
    /// the body has been fully read. The hook receives the status code, the
    /// `X-Request-Id` header (when present), and the wall-clock duration of
    /// the attempt in milliseconds. Panics inside the hook are swallowed.
    #[must_use]
    pub fn on_response<F>(mut self, f: F) -> Self
    where
        F: Fn(&ResponseEvent) + Send + Sync + 'static,
    {
        self.on_response = Some(Arc::new(f));
        self
    }

    /// Validate the configuration and construct a [`PoliPage`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidOptions`] when:
    /// - `api_key` is empty or wasn't set;
    /// - `base_url` was set to a non-parseable URL.
    pub fn build(self) -> Result<PoliPage, Error> {
        let api_key = self.api_key.unwrap_or_default();
        if api_key.is_empty() {
            return Err(Error::InvalidOptions {
                message: "apiKey is required".into(),
            });
        }
        let base_url_str = self
            .base_url
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let base_url = Url::parse(&base_url_str).map_err(|e| Error::InvalidOptions {
            message: format!("invalid base_url {base_url_str:?}: {e}"),
        })?;
        let config = ClientConfig {
            api_key,
            base_url,
            max_retries: self.max_retries.unwrap_or(DEFAULT_MAX_RETRIES),
            retry_delay: self.retry_delay.unwrap_or(DEFAULT_RETRY_DELAY),
            timeout: self.timeout.unwrap_or(DEFAULT_TIMEOUT),
            user_agent: format!("poli-page-sdk-rust/{}", env!("CARGO_PKG_VERSION")),
        };
        let http = match self.http_client {
            Some(client) => client,
            None => sdk_http_client_builder()
                .build()
                .map_err(|e| Error::InvalidOptions {
                    message: format!("could not build reqwest::Client: {e}"),
                })?,
        };
        let inner = Arc::new(ClientInner {
            config,
            http,
            on_retry: self.on_retry,
            on_error: self.on_error,
            on_request: self.on_request,
            on_response: self.on_response,
        });
        Ok(PoliPage {
            render: Render::new(Arc::clone(&inner)),
            documents: Documents::new(Arc::clone(&inner)),
            inner,
        })
    }
}

/// The `reqwest` client the SDK builds when none is injected.
///
/// Every resend must be the SDK's own retry, counted by `max_retries` and
/// reported to `on_request` / `on_retry`: the API does not deduplicate on
/// `Idempotency-Key`, so a render sent twice can be stored twice. reqwest's
/// default retry policy resends a request the server refused over HTTP/2
/// (`REFUSED_STREAM`, a graceful `GOAWAY`), so it is turned off. HTTP/2 is
/// not one of the SDK's reqwest features, but Cargo feature unification
/// turns it on when the application's own reqwest has it.
///
/// What stays below reqwest: hyper's pooled client sends a request again on
/// a new connection only when none of it was written to the reused one
/// (hyper-util's `retry_canceled_requests`). The server never saw that
/// request, so it is not a resend. A request that was written and then lost
/// its connection always comes back as an error.
pub(crate) fn sdk_http_client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().retry(reqwest::retry::never())
}

/// Sleep for at least `delay`: if the timer wakes early on the monotonic
/// clock, sleep again for the rest (spec §9.2). `tokio::time::Instant`
/// follows a paused test clock, so this never spins under
/// `tokio::time::pause`.
async fn sleep_at_least(delay: Duration) {
    let end = tokio::time::Instant::now() + delay;
    while tokio::time::Instant::now() < end {
        tokio::time::sleep_until(end).await;
    }
}

// `new_v4_string` is the auto-generation site for the Idempotency-Key header
// on POSTs that the caller didn't override. Centralised in the orchestrator so
// every POST path picks it up uniformly.
pub(crate) fn auto_idempotency_key() -> String {
    new_v4_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_requires_api_key() {
        let err = PoliPage::builder().build().expect_err("empty build");
        assert!(matches!(err, Error::InvalidOptions { .. }));
        // Spec §8.3: the exact text, the same in every SDK.
        assert_eq!(err.message(), "apiKey is required");
        assert_eq!(err.to_string(), "apiKey is required");
    }

    #[test]
    fn builder_rejects_empty_api_key() {
        let err = PoliPage::builder()
            .api_key("")
            .build()
            .expect_err("empty key");
        assert!(matches!(err, Error::InvalidOptions { .. }));
        assert_eq!(err.message(), "apiKey is required");
    }

    #[test]
    fn builder_rejects_invalid_base_url() {
        let err = PoliPage::builder()
            .api_key("pp_sandbox_x")
            .base_url("not a url")
            .build()
            .expect_err("invalid base url");
        assert!(matches!(err, Error::InvalidOptions { .. }));
    }

    #[test]
    fn builder_accepts_minimum_config() {
        let client = PoliPage::new("pp_sandbox_x").expect("default build");
        assert_eq!(client.inner.config.max_retries, DEFAULT_MAX_RETRIES);
        assert_eq!(client.inner.config.retry_delay, DEFAULT_RETRY_DELAY);
        assert_eq!(client.inner.config.timeout, DEFAULT_TIMEOUT);
        assert_eq!(
            client.inner.config.base_url.as_str(),
            "https://api.poli.page/"
        );
    }

    #[test]
    fn builder_overrides_take_effect() {
        let client = PoliPage::builder()
            .api_key("pp_sandbox_x")
            .base_url("https://api.example.com")
            .max_retries(5)
            .retry_delay(Duration::from_millis(100))
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();
        assert_eq!(client.inner.config.max_retries, 5);
        assert_eq!(client.inner.config.retry_delay, Duration::from_millis(100));
        assert_eq!(client.inner.config.timeout, Duration::from_secs(10));
        assert_eq!(
            client.inner.config.base_url.as_str(),
            "https://api.example.com/",
        );
    }

    #[test]
    fn poli_page_clone_is_cheap_arc_bump() {
        // Just verify the API compiles and the Arc count goes up — the actual
        // perf is structural (Arc<ClientInner>).
        let client = PoliPage::new("pp_sandbox_x").unwrap();
        let before = Arc::strong_count(&client.inner);
        let _c2 = client.clone();
        let after = Arc::strong_count(&client.inner);
        assert!(after > before);
    }

    #[test]
    fn auto_idempotency_key_returns_uuid_v4() {
        let k = auto_idempotency_key();
        assert_eq!(k.len(), 36);
    }

    // --- no hidden resend -------------------------------------------------
    //
    // reqwest's default retry policy resends a request the server refused
    // over HTTP/2 (`REFUSED_STREAM`), on the same connection, without the
    // SDK seeing it: `on_request` fires once, `max_retries` does not count
    // it. `sdk_http_client_builder` turns that policy off. HTTP/2 needs
    // reqwest's `http2` feature, on here through the dev-dependencies (an
    // application's own reqwest can turn it on the same way).

    /// An h2c server that refuses its first `refused` streams and answers
    /// the others `200 {}`. Returns its origin and counters of the TCP
    /// connections and the streams it saw.
    async fn refusing_h2_server(
        refused: usize,
    ) -> (
        String,
        Arc<std::sync::atomic::AtomicUsize>,
        Arc<std::sync::atomic::AtomicUsize>,
    ) {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let origin = format!("http://{}", listener.local_addr().expect("addr"));
        let connections = Arc::new(AtomicUsize::new(0));
        let streams = Arc::new(AtomicUsize::new(0));
        let (conns, strs) = (Arc::clone(&connections), Arc::clone(&streams));
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                conns.fetch_add(1, Ordering::SeqCst);
                let strs = Arc::clone(&strs);
                tokio::spawn(async move {
                    let Ok(mut conn) = h2::server::handshake(socket).await else {
                        return;
                    };
                    while let Some(Ok((_request, mut respond))) = conn.accept().await {
                        if strs.fetch_add(1, Ordering::SeqCst) < refused {
                            respond.send_reset(h2::Reason::REFUSED_STREAM);
                            continue;
                        }
                        let head = http::Response::builder()
                            .status(200)
                            .header("content-type", "application/json")
                            .body(())
                            .expect("response head");
                        if let Ok(mut body) = respond.send_response(head, false) {
                            let _ = body.send_data(Bytes::from_static(b"{}"), true);
                        }
                    }
                });
            }
        });
        (origin, connections, streams)
    }

    fn h2_client(
        origin: &str,
        http: reqwest::Client,
        max_retries: u32,
    ) -> (PoliPage, Arc<std::sync::atomic::AtomicUsize>) {
        let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = Arc::clone(&attempts);
        let client = PoliPage::builder()
            .api_key("pp_sandbox_x")
            .base_url(origin)
            .max_retries(max_retries)
            .retry_delay(Duration::from_millis(1))
            .timeout(Duration::from_secs(2))
            .http_client(http)
            .on_request(move |_| {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            })
            .build()
            .expect("client");
        (client, attempts)
    }

    fn thumbnails(
        client: &PoliPage,
    ) -> impl std::future::Future<Output = Result<Vec<crate::Thumbnail>, Error>> + '_ {
        // Answers `{}`: a POST whose 200 body is not a thumbnails envelope
        // still shows whether the request went through.
        client.documents.thumbnails("doc_abc123", None)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn no_hidden_resend_of_a_refused_http2_post() {
        use std::sync::atomic::Ordering;
        let (origin, connections, streams) = refusing_h2_server(1).await;
        let http = sdk_http_client_builder()
            .http2_prior_knowledge()
            .build()
            .expect("reqwest client");
        let (client, attempts) = h2_client(&origin, http, 0);
        let err = thumbnails(&client).await.expect_err("refused");
        assert_eq!(err.code(), "network_error", "{err:?}");
        assert_eq!(streams.load(Ordering::SeqCst), 1, "the POST went out once");
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert_eq!(connections.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_refused_http2_post_is_resent_only_as_a_counted_retry() {
        use std::sync::atomic::Ordering;
        let (origin, _, streams) = refusing_h2_server(1).await;
        let http = sdk_http_client_builder()
            .http2_prior_knowledge()
            .build()
            .expect("reqwest client");
        let (client, attempts) = h2_client(&origin, http, 1);
        // The retry reaches the 200, whose `{}` is not a thumbnails body.
        let err = thumbnails(&client).await.expect_err("not an envelope");
        assert_eq!(err.code(), "INTERNAL_ERROR", "{err:?}");
        assert_eq!(err.status(), Some(200));
        assert_eq!(streams.load(Ordering::SeqCst), 2);
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            2,
            "both sends are SDK attempts"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn control_reqwest_default_client_resends_a_refused_http2_post() {
        // Control: the same request through reqwest's default policy is
        // sent twice on one connection for a single SDK attempt. This is
        // what `sdk_http_client_builder` prevents.
        use std::sync::atomic::Ordering;
        let (origin, connections, streams) = refusing_h2_server(1).await;
        let http = reqwest::Client::builder()
            .http2_prior_knowledge()
            .build()
            .expect("reqwest client");
        let (client, attempts) = h2_client(&origin, http, 0);
        let _ = thumbnails(&client).await;
        assert_eq!(streams.load(Ordering::SeqCst), 2, "reqwest resent the POST");
        assert_eq!(
            connections.load(Ordering::SeqCst),
            1,
            "on the same connection"
        );
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            1,
            "the SDK saw one attempt"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn sleep_at_least_follows_a_paused_clock() {
        // Never spins: the paused clock jumps to the deadline.
        let start = tokio::time::Instant::now();
        sleep_at_least(Duration::from_secs(30)).await;
        assert!(tokio::time::Instant::now() - start >= Duration::from_secs(30));
    }

    #[tokio::test]
    async fn sleep_at_least_never_wakes_before_the_delay() {
        for ms in [1u64, 2, 5, 10, 15, 20] {
            let delay = Duration::from_millis(ms);
            let start = std::time::Instant::now();
            sleep_at_least(delay).await;
            let slept = start.elapsed();
            assert!(slept >= delay, "slept {slept:?} for a {delay:?} delay");
        }
    }

    #[test]
    fn builder_uses_injected_http_client() {
        // A unique User-Agent on the supplied client is observable later via
        // `ClientInner::http` only structurally — but the simpler invariant
        // is that the builder *accepts* a client and stores it. We assert
        // both: the builder field round-trips, and `build()` succeeds when
        // one is supplied.
        let custom = reqwest::Client::builder()
            .user_agent("test-injected/1.0")
            .build()
            .expect("custom client builds");
        let client = PoliPage::builder()
            .api_key("pp_sandbox_x")
            .http_client(custom)
            .build()
            .expect("build with injected client");
        // We can't peek at reqwest::Client internals, so the meaningful
        // assertion is structural: a client was constructed and the
        // namespaces are reachable. The wiremock test in
        // `tests/render.rs::render_uses_injected_http_client` proves the
        // injected client is the one that actually issues requests.
        let _ = &client.render;
        let _ = &client.documents;
    }
}
