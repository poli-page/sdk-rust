//! `render_to_file` — render a PDF and stream it to disk in bounded memory.
//!
//! Behavior parity with Node SDK's `renderToFile`; spec §11.

use std::future::poll_fn;
use std::path::Path;
use std::pin::Pin;

use futures_core::Stream;
use tokio::io::AsyncWriteExt;

use crate::{Error, PoliPage, ProjectModeInput};

/// Render a PDF and stream the bytes to the given path.
///
/// Creates parent directories if missing. Overwrites existing files. Uses
/// bounded memory regardless of PDF size — chunks are written as they arrive
/// from the deployed API via [`crate::Render::pdf_stream`].
///
/// `path` accepts anything that implements [`AsRef<Path>`] — `&str`, `&Path`,
/// `PathBuf`, etc.
///
/// # Errors
///
/// - [`Error::InvalidOptions`] for filesystem failures (couldn't create the
///   parent directory, couldn't open the file for writing, write failed,
///   flush failed).
/// - Any error variant [`crate::Render::pdf_stream`] returns: the first-hop
///   `POST /v1/render` family, or a [`Error::Download`] on the second-hop
///   presigned-URL fetch.
///
/// # Example
///
/// ```no_run
/// use poli_page::{PoliPage, ProjectModeInput};
/// use serde_json::json;
///
/// # async fn run() -> Result<(), poli_page::Error> {
/// let client = PoliPage::new("pp_test_...")?;
/// poli_page::render_to_file(
///     &client,
///     ProjectModeInput {
///         project: "billing".into(),
///         template: "invoice".into(),
///         version: Some("1.0.0".into()),
///         data: json!({ "invoiceNumber": "INV-001" }),
///         ..Default::default()
///     },
///     "invoice.pdf",
/// ).await?;
/// # Ok(()) }
/// ```
pub async fn render_to_file(
    client: &PoliPage,
    input: ProjectModeInput,
    path: impl AsRef<Path>,
) -> Result<(), Error> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        // `parent` is `""` when the path has no directory component; skip the
        // create_dir_all call so we don't tell tokio to create `""`.
        if !parent.as_os_str().is_empty() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::InvalidOptions {
                    message: format!("failed to create directory {}: {}", parent.display(), e),
                })?;
        }
    }

    let stream = client.render.pdf_stream(input).await?;
    let mut stream = std::pin::pin!(stream);
    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(|e| Error::InvalidOptions {
            message: format!("failed to create file {}: {}", path.display(), e),
        })?;

    // Consume the stream chunk-by-chunk via std::future::poll_fn so we don't
    // need to depend on `futures_util::StreamExt` (it's dev-only per §5.1).
    while let Some(chunk) = poll_fn(|cx| poll_next_pin(&mut stream, cx)).await {
        let chunk = chunk?;
        file.write_all(&chunk)
            .await
            .map_err(|e| Error::InvalidOptions {
                message: format!("failed to write to {}: {}", path.display(), e),
            })?;
    }
    file.flush().await.map_err(|e| Error::InvalidOptions {
        message: format!("failed to flush {}: {}", path.display(), e),
    })?;
    Ok(())
}

/// Adapter so the `while let` above stays type-inference-friendly.
fn poll_next_pin<S: Stream + ?Sized>(
    stream: &mut Pin<&mut S>,
    cx: &mut std::task::Context<'_>,
) -> std::task::Poll<Option<S::Item>> {
    stream.as_mut().poll_next(cx)
}
