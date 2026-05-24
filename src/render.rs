//! The `render` namespace — `client.render.{preview, pdf, pdf_stream, document}`.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use http::Method;

use crate::client::{auto_idempotency_key, ClientInner, HttpAttempt, PdfByteStream};
use crate::types::document::attach_client;
use crate::{DocumentDescriptor, Error, PreviewResult, ProjectModeInput, RenderInput};

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
    /// POSTs to `/v1/render/preview`. Returns the parsed [`PreviewResult`]
    /// (`html`, `total_pages`, `environment`).
    ///
    /// # Errors
    ///
    /// - [`Error::BadRequest`] when the API rejects the input shape
    ///   (e.g. missing `data`).
    /// - [`Error::Auth`] / [`Error::PermissionDenied`] on credential issues.
    /// - [`Error::RateLimited`] when the tenant exceeds its quota.
    /// - [`Error::Api`] for any other 4xx/5xx after retries are exhausted.
    /// - [`Error::Connection`] / [`Error::Timeout`] for network failures.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use poli_page::{InlineModeInput, PoliPage};
    /// use serde_json::json;
    ///
    /// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = PoliPage::new("pp_test_...")?;
    /// let result = client.render.preview(InlineModeInput {
    ///     template: "<h1>Hello {{ name }}</h1>".into(),
    ///     data: json!({ "name": "World" }),
    ///     ..Default::default()
    /// }).await?;
    /// println!("{} page(s)", result.total_pages);
    /// # Ok(()) }
    /// ```
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

    /// Render a PDF and return it as raw bytes. Two HTTP calls under the
    /// hood: `POST /v1/render` to produce a stored document, then `GET`
    /// against the descriptor's `presigned_pdf_url` to fetch the bytes.
    /// The second call is unauthenticated and not retried (spec §5.5).
    ///
    /// # Errors
    ///
    /// Same `Error::*` shape as [`Self::document`] for the first hop;
    /// second-hop failures (presigned URL expired, S3 unreachable, etc.)
    /// surface as [`Error::Download`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// use poli_page::{PoliPage, ProjectModeInput};
    /// use serde_json::json;
    ///
    /// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = PoliPage::new("pp_test_...")?;
    /// let pdf = client.render.pdf(ProjectModeInput {
    ///     project: "billing".into(),
    ///     template: "invoice".into(),
    ///     version: Some("1.0.0".into()),
    ///     data: json!({ "invoiceNumber": "INV-001" }),
    ///     ..Default::default()
    /// }).await?;
    /// std::fs::write("invoice.pdf", &pdf)?;
    /// # Ok(()) }
    /// ```
    pub async fn pdf(&self, input: ProjectModeInput) -> Result<Bytes, Error> {
        let descriptor = self.document(input).await?;
        descriptor.download_pdf().await
    }

    /// Render a PDF and return a streaming view of its bytes. Identical wire
    /// behaviour to [`Self::pdf`] — two hops with the same auth/retry rules
    /// — but the second hop's body is streamed chunk-by-chunk instead of
    /// buffered into `Bytes`. Use this when piping directly into an outgoing
    /// HTTP response, an S3 upload, or a file without wanting the whole PDF
    /// in memory at once.
    ///
    /// The returned [`PdfByteStream`] implements
    /// `futures_core::Stream<Item = Result<Bytes, Error>>`. Consume it with
    /// `futures::StreamExt` (or a `while let Some(chunk) = stream.next()`
    /// loop where `next()` is provided by `StreamExt`).
    ///
    /// # Errors
    ///
    /// First-hop errors mirror [`Self::document`]. Second-hop *header* errors
    /// (non-2xx, network) surface synchronously here as [`Error::Download`];
    /// chunk-level errors mid-stream surface as `Err` items inside the
    /// stream itself.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use poli_page::{PoliPage, ProjectModeInput};
    /// use serde_json::json;
    /// use std::future::poll_fn;
    /// use std::pin::Pin;
    /// use futures_core::Stream;
    ///
    /// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = PoliPage::new("pp_test_...")?;
    /// let mut stream = std::pin::pin!(client.render.pdf_stream(ProjectModeInput {
    ///     project: "billing".into(),
    ///     template: "invoice".into(),
    ///     version: Some("1.0.0".into()),
    ///     data: json!({}),
    ///     ..Default::default()
    /// }).await?);
    /// while let Some(chunk) = poll_fn(|cx| Pin::new(&mut stream).poll_next(cx)).await {
    ///     let chunk = chunk?;
    ///     # let _ = chunk;
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn pdf_stream(&self, input: ProjectModeInput) -> Result<PdfByteStream, Error> {
        let descriptor = self.document(input).await?;
        self.inner.stream_bytes(&descriptor.presigned_pdf_url).await
    }

    /// Render a PDF, store it server-side, and return a flat document
    /// descriptor. Like [`Self::pdf`] but skips the auto-download —
    /// the caller fetches the PDF via [`DocumentDescriptor::download_pdf`]
    /// (or stores `document_id` for later via Phase 4's `documents.get`).
    ///
    /// POSTs to `/v1/render`. Same wire endpoint as `pdf` — the difference is
    /// that `pdf` chains a second fetch.
    ///
    /// # Errors
    ///
    /// - [`Error::BadRequest`] when the input shape is invalid.
    /// - [`Error::Auth`] / [`Error::PermissionDenied`] on credential issues.
    /// - [`Error::RateLimited`] when the tenant exceeds its quota.
    /// - [`Error::Api`] for any other 4xx/5xx after retries are exhausted.
    /// - [`Error::Internal`] when the response body fails to parse as a
    ///   `DocumentDescriptor`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use poli_page::{PoliPage, ProjectModeInput};
    /// use serde_json::json;
    ///
    /// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = PoliPage::new("pp_test_...")?;
    /// let doc = client.render.document(ProjectModeInput {
    ///     project: "billing".into(),
    ///     template: "invoice".into(),
    ///     version: Some("1.0.0".into()),
    ///     data: json!({}),
    ///     ..Default::default()
    /// }).await?;
    /// println!("stored as {} ({} pages)", doc.document_id, doc.page_count);
    /// let pdf = doc.download_pdf().await?;
    /// # let _ = pdf; Ok(()) }
    /// ```
    pub async fn document(&self, input: ProjectModeInput) -> Result<DocumentDescriptor, Error> {
        let idempotency_key = input
            .idempotency_key
            .clone()
            .unwrap_or_else(auto_idempotency_key);
        let timeout_override = input.timeout;
        let raw: DocumentDescriptor = execute_post_json(
            &self.inner,
            "/v1/render",
            &input,
            Some(&idempotency_key),
            timeout_override,
        )
        .await?;
        Ok(attach_client(raw, Arc::clone(&self.inner)))
    }
}

/// Crate-internal helper — POSTs a JSON body and deserialises the response.
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
