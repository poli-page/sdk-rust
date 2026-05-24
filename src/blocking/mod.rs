//! Synchronous wrapper over the async [`crate::PoliPage`] surface.
//!
//! Available behind the `blocking` Cargo feature; each method here wraps the
//! async equivalent in `runtime.block_on(...)` against a single
//! current-thread `tokio::runtime::Runtime` owned by the client.
//!
//! Use this when you need a sync API — short scripts, CLI tools that don't
//! want to opt into the async ecosystem, callers from inside a sync host.
//! For everything else prefer the async surface; it's the same code path
//! without the block_on overhead.

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use tokio::runtime::Runtime;

use crate::Error;
use crate::RetryEvent;
use crate::{
    DocumentDescriptor, DocumentPreviewResult, PreviewResult, ProjectModeInput, RenderInput,
    Thumbnail, ThumbnailOptions,
};

pub mod fs;

pub use fs::{render_to_file, BlockingPdfReader};

/// Synchronous Poli Page client. Cheap to clone — internally an
/// `Arc<tokio::runtime::Runtime>` plus the same shared async state.
#[derive(Clone)]
pub struct PoliPage {
    // Held so cloning the client keeps the runtime alive; render and
    // documents hold their own Arc clones to use it.
    #[allow(dead_code)]
    runtime: Arc<Runtime>,
    /// The `render` namespace.
    pub render: Render,
    /// The `documents` namespace.
    pub documents: Documents,
}

impl std::fmt::Debug for PoliPage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PoliPage").finish_non_exhaustive()
    }
}

impl PoliPage {
    /// Build a sync client with the default configuration and the given API
    /// key. For non-default options use [`PoliPage::builder`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidOptions`] when `api_key` is empty or the
    /// tokio runtime fails to build.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use poli_page::{ProjectModeInput, blocking::PoliPage};
    /// use serde_json::json;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = PoliPage::new("pp_test_...")?;
    /// let pdf = client.render.pdf(ProjectModeInput {
    ///     project: "getting-started".into(),
    ///     template: "welcome".into(),
    ///     version: Some("1.0.0".into()),
    ///     data: json!({ "name": "World" }),
    ///     ..Default::default()
    /// })?;
    /// std::fs::write("welcome.pdf", &pdf)?;
    /// # Ok(()) }
    /// ```
    pub fn new(api_key: impl Into<String>) -> Result<Self, Error> {
        Self::builder().api_key(api_key).build()
    }

    /// Begin configuring a sync client with the builder pattern.
    pub fn builder() -> PoliPageBuilder {
        PoliPageBuilder::default()
    }
}

/// Builder for the synchronous [`PoliPage`].
///
/// Mirrors [`crate::PoliPageBuilder`] one-to-one — setters return `Self` for
/// chaining; only [`PoliPageBuilder::build`] can fail.
#[derive(Default)]
pub struct PoliPageBuilder {
    inner: crate::PoliPageBuilder,
}

impl std::fmt::Debug for PoliPageBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PoliPageBuilder")
            .field("inner", &self.inner)
            .finish()
    }
}

impl PoliPageBuilder {
    /// Set the API key. Required — `build()` fails without it.
    #[must_use]
    pub fn api_key(mut self, api_key: impl Into<String>) -> Self {
        self.inner = self.inner.api_key(api_key);
        self
    }

    /// Override the base URL (defaults to `https://api.poli.page`).
    #[must_use]
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.inner = self.inner.base_url(base_url);
        self
    }

    /// Override the maximum number of retries on retryable failures.
    #[must_use]
    pub fn max_retries(mut self, max_retries: u32) -> Self {
        self.inner = self.inner.max_retries(max_retries);
        self
    }

    /// Override the initial retry delay.
    #[must_use]
    pub fn retry_delay(mut self, retry_delay: Duration) -> Self {
        self.inner = self.inner.retry_delay(retry_delay);
        self
    }

    /// Override the per-attempt timeout.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.inner = self.inner.timeout(timeout);
        self
    }

    /// Register a callback fired before each retry sleep.
    #[must_use]
    pub fn on_retry<F>(mut self, f: F) -> Self
    where
        F: Fn(&RetryEvent) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_retry(f);
        self
    }

    /// Register a callback fired once per terminal failure.
    #[must_use]
    pub fn on_error<F>(mut self, f: F) -> Self
    where
        F: Fn(&Error) + Send + Sync + 'static,
    {
        self.inner = self.inner.on_error(f);
        self
    }

    /// Validate the configuration and construct a [`PoliPage`].
    ///
    /// # Errors
    ///
    /// - [`Error::InvalidOptions`] for invalid options (empty api_key,
    ///   un-parseable base_url) or if the tokio runtime fails to build.
    pub fn build(self) -> Result<PoliPage, Error> {
        let async_client = self.inner.build()?;
        // Single-threaded runtime: blocking callers are typically already
        // single-threaded, and current-thread avoids spawning a worker pool
        // we wouldn't otherwise need.
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| Error::InvalidOptions {
                message: format!("failed to build tokio runtime for blocking client: {e}"),
            })?;
        let runtime = Arc::new(runtime);
        let render = Render {
            runtime: Arc::clone(&runtime),
            inner: async_client.render.clone(),
        };
        let documents = Documents {
            runtime: Arc::clone(&runtime),
            inner: async_client.documents.clone(),
        };
        Ok(PoliPage {
            runtime,
            render,
            documents,
        })
    }
}

/// The sync `client.render` namespace.
#[derive(Clone)]
pub struct Render {
    pub(crate) runtime: Arc<Runtime>,
    pub(crate) inner: crate::Render,
}

impl std::fmt::Debug for Render {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("blocking::Render").finish_non_exhaustive()
    }
}

impl Render {
    /// Sync counterpart of [`crate::Render::preview`].
    pub fn preview(&self, input: impl Into<RenderInput>) -> Result<PreviewResult, Error> {
        let input = input.into();
        self.runtime.block_on(self.inner.preview(input))
    }

    /// Sync counterpart of [`crate::Render::pdf`].
    pub fn pdf(&self, input: ProjectModeInput) -> Result<Bytes, Error> {
        self.runtime.block_on(self.inner.pdf(input))
    }

    /// Sync counterpart of [`crate::Render::pdf_stream`] — returns a
    /// `std::io::Read` whose reads block on chunk arrival. Use with
    /// `std::io::copy` to pipe into any sink.
    pub fn pdf_stream(&self, input: ProjectModeInput) -> Result<BlockingPdfReader, Error> {
        let stream = self.runtime.block_on(self.inner.pdf_stream(input))?;
        Ok(BlockingPdfReader::new(Arc::clone(&self.runtime), stream))
    }

    /// Sync counterpart of [`crate::Render::document`]. The returned
    /// descriptor carries a back-reference to the **async** client; calls
    /// to [`DocumentDescriptor::download_pdf`] must happen inside a tokio
    /// runtime. For purely sync downloads, the descriptor's
    /// `presigned_pdf_url` can be passed to [`Render::pdf_stream`] or fetched
    /// with any sync HTTP client.
    pub fn document(&self, input: ProjectModeInput) -> Result<DocumentDescriptor, Error> {
        self.runtime.block_on(self.inner.document(input))
    }
}

/// The sync `client.documents` namespace.
#[derive(Clone)]
pub struct Documents {
    pub(crate) runtime: Arc<Runtime>,
    pub(crate) inner: crate::Documents,
}

impl std::fmt::Debug for Documents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("blocking::Documents")
            .finish_non_exhaustive()
    }
}

impl Documents {
    /// Sync counterpart of [`crate::Documents::get`].
    pub fn get(&self, id: &str) -> Result<DocumentDescriptor, Error> {
        self.runtime.block_on(self.inner.get(id))
    }

    /// Sync counterpart of [`crate::Documents::preview`].
    pub fn preview(&self, id: &str) -> Result<DocumentPreviewResult, Error> {
        self.runtime.block_on(self.inner.preview(id))
    }

    /// Sync counterpart of [`crate::Documents::thumbnails`].
    pub fn thumbnails(&self, id: &str, options: ThumbnailOptions) -> Result<Vec<Thumbnail>, Error> {
        self.runtime.block_on(self.inner.thumbnails(id, options))
    }

    /// Sync counterpart of [`crate::Documents::delete`].
    pub fn delete(&self, id: &str) -> Result<(), Error> {
        self.runtime.block_on(self.inner.delete(id))
    }
}
