//! The `documents` namespace — `client.documents.{get, preview, thumbnails, delete}`.
//!
//! Spec §6 / Node `src/documents.ts`.

use std::sync::Arc;

use http::Method;
use serde::Deserialize;

use crate::client::{auto_idempotency_key, ClientInner, HttpAttempt};
use crate::internal::constants::{HEADER_DOCUMENT_PAGE_COUNT, PATH_DOCUMENTS};
use crate::internal::url::encode_path_segment;
use crate::types::document::attach_client;
use crate::{DocumentDescriptor, DocumentPreviewResult, Error, Thumbnail, ThumbnailOptions};

/// The `client.documents` namespace. Cheap to clone — internally an
/// `Arc<ClientInner>`.
#[derive(Clone)]
#[must_use = "a Documents handle is only useful when one of its methods is called"]
pub struct Documents {
    inner: Arc<ClientInner>,
}

impl std::fmt::Debug for Documents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Documents").finish_non_exhaustive()
    }
}

impl Documents {
    pub(crate) fn new(inner: Arc<ClientInner>) -> Self {
        Self { inner }
    }

    /// Retrieve a stored document by ID, returning its descriptor and a fresh
    /// `presigned_pdf_url` (15-minute TTL — call this again to re-mint).
    ///
    /// Spec §6.1. GET `/v1/documents/:id`.
    ///
    /// # Errors
    ///
    /// - [`Error::NotFound`] when no document with this ID exists.
    /// - [`Error::Gone`] when the document was deleted.
    /// - [`Error::Auth`] / [`Error::PermissionDenied`] on credential issues.
    /// - [`Error::Internal`] when the response body fails to parse.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use poli_page::PoliPage;
    /// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = PoliPage::new("pp_sandbox_...")?;
    /// let doc = client.documents.get("doc_abc123").await?;
    /// let pdf = doc.download_pdf().await?;
    /// # let _ = pdf; Ok(()) }
    /// ```
    pub async fn get(&self, id: &str) -> Result<DocumentDescriptor, Error> {
        let path = format!("{PATH_DOCUMENTS}/{}", encode_path_segment(id));
        let raw: DocumentDescriptor = execute_get_json(&self.inner, &path).await?;
        Ok(attach_client(raw, Arc::clone(&self.inner)))
    }

    /// Retrieve a stored document's paginated HTML. Performs no rendering on
    /// the server (no quota counter increments). Spec §6.2.
    ///
    /// The deployed API responds with `text/html` directly and exposes the
    /// page count via the `X-Document-Page-Count` header — the SDK assembles
    /// the [`DocumentPreviewResult`] envelope here.
    ///
    /// # Errors
    ///
    /// Mirrors [`Self::get`].
    ///
    /// # Example
    ///
    /// ```no_run
    /// use poli_page::PoliPage;
    /// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = PoliPage::new("pp_sandbox_...")?;
    /// let preview = client.documents.preview("doc_abc123").await?;
    /// println!("{} pages: {}", preview.page_count, preview.html.len());
    /// # Ok(()) }
    /// ```
    pub async fn preview(&self, id: &str) -> Result<DocumentPreviewResult, Error> {
        let path = format!("{PATH_DOCUMENTS}/{}/preview", encode_path_segment(id));
        let attempt = HttpAttempt {
            method: Method::GET,
            path: &path,
            body: None,
            idempotency_key: None,
            timeout: None,
        };
        let resp = self.inner.execute(attempt).await?;
        let html = String::from_utf8(resp.body.to_vec()).map_err(|e| Error::Internal {
            message: format!("documents.preview body was not valid UTF-8: {e}"),
            status: Some(resp.status),
        })?;
        // NaN-tolerant: missing/unparseable header → 0 (Node behavior).
        let page_count = resp
            .headers
            .get(HEADER_DOCUMENT_PAGE_COUNT)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        Ok(DocumentPreviewResult { html, page_count })
    }

    /// Generate page thumbnails for a stored document. Spec §6.3.
    /// POSTs `/v1/documents/:id/thumbnails` with the options wrapped under a
    /// `thumbnails` key (a deployed-API quirk), or `{}` when no option is set
    /// so every API default applies (width 400, JPEG). Options are not
    /// range-checked here: the API answers `400 VALIDATION_ERROR`. Unwraps
    /// the server's `{ thumbnails: [...] }` envelope and returns the array.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use poli_page::{PoliPage, ThumbnailFormat, ThumbnailOptions};
    /// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = PoliPage::new("pp_sandbox_...")?;
    /// // API defaults: 400 px wide JPEG.
    /// let thumbs = client.documents.thumbnails("doc_abc123", None).await?;
    /// // Or pick the options you need.
    /// let thumbs = client.documents.thumbnails(
    ///     "doc_abc123",
    ///     Some(ThumbnailOptions { format: Some(ThumbnailFormat::Webp), ..ThumbnailOptions::new().width(320) }),
    /// ).await?;
    /// for t in &thumbs {
    ///     println!("page {} — {}×{}", t.page, t.width, t.height);
    /// }
    /// # Ok(()) }
    /// ```
    pub async fn thumbnails(
        &self,
        id: &str,
        options: Option<ThumbnailOptions>,
    ) -> Result<Vec<Thumbnail>, Error> {
        let path = format!("{PATH_DOCUMENTS}/{}/thumbnails", encode_path_segment(id));
        let options = serde_json::to_value(options.unwrap_or_default()).map_err(|e| {
            Error::InvalidOptions {
                message: format!("could not serialize thumbnail options: {e}"),
            }
        })?;
        // Wire-body wrap: { "thumbnails": <options> }, or {} with no option set.
        let body = if options == serde_json::json!({}) {
            options
        } else {
            let mut body = serde_json::Map::new();
            body.insert("thumbnails".to_owned(), options);
            serde_json::Value::Object(body)
        };
        let idempotency_key = auto_idempotency_key();
        let attempt = HttpAttempt {
            method: Method::POST,
            path: &path,
            body: Some(&body),
            idempotency_key: Some(&idempotency_key),
            timeout: None,
        };
        let resp = self.inner.execute(attempt).await?;
        let envelope: ThumbnailsResponse =
            serde_json::from_slice(&resp.body).map_err(|e| Error::Internal {
                message: format!("could not parse thumbnails response: {e}"),
                status: Some(resp.status),
            })?;
        Ok(envelope.thumbnails)
    }

    /// Soft-delete a stored document. The PDF is purged from storage;
    /// metadata is retained for audit. Spec §6.4. DELETE `/v1/documents/:id`.
    ///
    /// Idempotent: the API answers `204` for an already-deleted document
    /// too, so a re-delete resolves with `Ok(())`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use poli_page::PoliPage;
    /// # async fn run() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = PoliPage::new("pp_sandbox_...")?;
    /// client.documents.delete("doc_abc123").await?;
    /// # Ok(()) }
    /// ```
    pub async fn delete(&self, id: &str) -> Result<(), Error> {
        let path = format!("{PATH_DOCUMENTS}/{}", encode_path_segment(id));
        let attempt = HttpAttempt {
            method: Method::DELETE,
            path: &path,
            body: None,
            idempotency_key: None,
            timeout: None,
        };
        let _ = self.inner.execute(attempt).await?;
        Ok(())
    }
}

/// Server response envelope for `thumbnails` — the SDK unwraps it before
/// returning to the caller.
#[derive(Debug, Deserialize)]
struct ThumbnailsResponse {
    thumbnails: Vec<Thumbnail>,
}

/// GET a path and deserialise the JSON response. Crate-private — namespace
/// methods use this for the standard "GET, then parse" pattern.
async fn execute_get_json<T>(inner: &Arc<ClientInner>, path: &str) -> Result<T, Error>
where
    T: serde::de::DeserializeOwned,
{
    let attempt = HttpAttempt {
        method: Method::GET,
        path,
        body: None,
        idempotency_key: None,
        timeout: None,
    };
    let resp = inner.execute(attempt).await?;
    serde_json::from_slice::<T>(&resp.body).map_err(|e| Error::Internal {
        message: format!("could not parse response body as JSON: {e}"),
        status: Some(resp.status),
    })
}
