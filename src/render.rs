//! The `render` namespace — `client.render.preview(input).await`.
//!
//! Phase 2 ships only `preview`; `pdf`, `pdf_stream`, and `document` land
//! in Phase 3 as stubs that return `Error::Internal` for now.

use std::sync::Arc;
use std::time::Duration;

use http::Method;

use crate::client::{auto_idempotency_key, ClientInner, HttpAttempt};
use crate::{Error, PreviewResult, ProjectModeInput, RenderInput};

/// The `client.render` namespace. Cheap to clone — internally an
/// `Arc<ClientInner>`.
#[derive(Clone)]
pub struct Render {
    inner: Arc<ClientInner>,
}

impl std::fmt::Debug for Render {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Render").finish_non_exhaustive()
    }
}

impl Render {
    pub(crate) fn new(inner: Arc<ClientInner>) -> Self {
        Self { inner }
    }

    /// Generate paginated HTML preview output for a render.
    ///
    /// Accepts both [`ProjectModeInput`] (resolved by project + template slug)
    /// and [`crate::InlineModeInput`] (raw HTML in `template`) via
    /// `impl Into<RenderInput>` — `render.preview(project_input).await` and
    /// `render.preview(inline_input).await` both compile.
    ///
    /// POSTs to `/v1/render/preview`. Returns the parsed
    /// [`PreviewResult`] (`html`, `total_pages`, `environment`).
    ///
    /// # Errors
    ///
    /// - [`Error::BadRequest`] when the API rejects the input shape
    ///   (e.g. missing `data`).
    /// - [`Error::Auth`] / [`Error::PermissionDenied`] on credential issues.
    /// - [`Error::RateLimited`] when the tenant exceeds its quota.
    /// - [`Error::Api`] for any other 4xx/5xx after retries are exhausted.
    /// - [`Error::Connection`] / [`Error::Timeout`] for network failures.
    pub async fn preview(&self, input: impl Into<RenderInput>) -> Result<PreviewResult, Error> {
        let input = input.into();
        let idempotency_key = input
            .idempotency_key()
            .map(str::to_owned)
            .unwrap_or_else(auto_idempotency_key);
        let timeout_override = input.timeout();
        execute_post_json(
            &self.inner,
            "/v1/render/preview",
            &input,
            Some(&idempotency_key),
            timeout_override,
        )
        .await
    }

    /// Phase 3 stub — returns `Error::Internal` until the real impl lands.
    pub async fn pdf(&self, _input: ProjectModeInput) -> Result<bytes::Bytes, Error> {
        Err(Error::Internal {
            message: "render.pdf is not implemented yet (Phase 3)".into(),
            status: None,
        })
    }

    /// Phase 3 stub — returns `Error::Internal` until the real impl lands.
    pub async fn document(&self, _input: ProjectModeInput) -> Result<serde_json::Value, Error> {
        Err(Error::Internal {
            message: "render.document is not implemented yet (Phase 3)".into(),
            status: None,
        })
    }
}

/// Crate-internal helper — POSTs a JSON body and deserialises the response.
///
/// Lives here (rather than as a method on `ClientInner`) so the generic over
/// the response type `T` doesn't pollute every call site of `ClientInner`.
async fn execute_post_json<B, T>(
    inner: &Arc<ClientInner>,
    path: &str,
    body: &B,
    idempotency_key: Option<&str>,
    timeout_override: Option<Duration>,
) -> Result<T, Error>
where
    B: serde::Serialize,
    T: serde::de::DeserializeOwned,
{
    let body_value = serde_json::to_value(body).map_err(|e| Error::InvalidOptions {
        message: format!("could not serialize request body: {e}"),
    })?;
    let attempt = HttpAttempt {
        method: Method::POST,
        path,
        body: Some(&body_value),
        idempotency_key,
        timeout: timeout_override,
    };
    let resp = inner.execute(attempt).await?;
    serde_json::from_slice::<T>(&resp.body).map_err(|e| Error::Internal {
        message: format!("could not parse response body as JSON: {e}"),
        status: Some(resp.status),
    })
}
