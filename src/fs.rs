//! `render_to_file` — render a PDF and stream it to disk in bounded memory.

use std::future::poll_fn;
use std::path::Path;
use std::pin::Pin;

use futures_core::Stream;
use tokio::io::AsyncWriteExt;

use crate::internal::part_file::{
    blocking, io_error, keep_permissions, parent_dir, part_path, prepare_target, PartGuard,
};
use crate::{Error, PoliPage, RenderInput};

/// Render a PDF and stream the bytes to the given path.
///
/// Creates parent directories if missing. Uses bounded memory regardless of
/// PDF size — chunks are written as they arrive from the deployed API via
/// [`crate::Render::pdf_stream`], whose idle timeout applies.
///
/// The PDF is written to a temporary file next to the target
/// (`.<name>.<random>.part`), which is renamed over the target only once the
/// PDF is complete: a call that fails leaves the target as it was (an
/// existing file keeps its content, a missing one is not created) and
/// removes the temporary file. A symbolic link at `path` is resolved first:
/// the file it points to is replaced and the link stays a link. An existing
/// file keeps its permissions; its owner and hard links do not survive the
/// rename, which makes a new file. An existing path that is not a regular
/// file (a directory, a FIFO, a device, a socket), or a file this process
/// may not write (read-only), is refused with `io_error` and left as it is.
/// These checks, made from the file's metadata, and the temporary file come
/// before the render, so a path that cannot be written costs no render.
///
/// `path` accepts anything that implements [`AsRef<Path>`] — `&str`, `&Path`,
/// `PathBuf`, etc.
///
/// # Errors
///
/// - [`Error::Io`] (`io_error`, `Failed to write PDF: <path>: <reason>`,
///   `<path>` as given, no status, never retried) when the file cannot be
///   written: a target that is not a regular file (`not a regular file`) or
///   is read-only, or creating a parent directory, creating or writing the
///   temporary file, or renaming it over the target failed. The I/O error is
///   the `source()`.
/// - Any error variant [`crate::Render::pdf_stream`] returns: the first-hop
///   `POST /v1/render` family, or a [`Error::Download`] on the second-hop
///   presigned-URL fetch.
///
/// # Example
///
/// ```no_run
/// use poli_page::{PoliPage, RenderInput};
/// use serde_json::json;
///
/// # async fn run() -> Result<(), poli_page::Error> {
/// let client = PoliPage::new("pp_sandbox_...")?;
/// poli_page::render_to_file(
///     &client,
///     RenderInput {
///         project: "billing".into(),
///         template: "invoice".into(),
///         data: json!({ "invoiceNumber": "INV-001" }),
///         ..Default::default()
///     },
///     "invoice.pdf",
/// ).await?;
/// # Ok(()) }
/// ```
pub async fn render_to_file(
    client: &PoliPage,
    input: RenderInput,
    path: impl AsRef<Path>,
) -> Result<(), Error> {
    let requested = path.as_ref();
    // Every write failure names the path the caller gave.
    let fail = |e| io_error(requested, e);
    let given = requested.to_path_buf();
    let target = blocking(move || prepare_target(&given))
        .await
        .map_err(fail)?;
    if let Some(parent) = parent_dir(&target) {
        tokio::fs::create_dir_all(parent).await.map_err(fail)?;
    }
    let part_path = part_path(&target).map_err(fail)?;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&part_path)
        .await
        .map_err(fail)?;
    // Removes the temporary file on every early return (and if this future
    // is dropped). Rust opens files with delete sharing on Windows, so the
    // removal works while `file` is still open.
    let part = PartGuard::new(part_path);

    let stream = client.render.pdf_stream(input).await?;
    let mut stream = std::pin::pin!(stream);
    // Consume the stream chunk-by-chunk via std::future::poll_fn so we don't
    // need to depend on `futures_util::StreamExt` (it's a dev-dependency).
    while let Some(chunk) = poll_fn(|cx| poll_next_pin(&mut stream, cx)).await {
        let chunk = chunk?;
        file.write_all(&chunk).await.map_err(fail)?;
    }
    file.flush().await.map_err(fail)?;
    // Wait for every write and close the file before the rename.
    drop(file.into_std().await);
    let (from, to) = (part.path().to_path_buf(), target.clone());
    blocking(move || keep_permissions(&to, &from))
        .await
        .map_err(fail)?;
    tokio::fs::rename(part.path(), &target)
        .await
        .map_err(fail)?;
    part.disarm();
    Ok(())
}

/// Adapter so the `while let` above stays type-inference-friendly.
fn poll_next_pin<S: Stream + ?Sized>(
    stream: &mut Pin<&mut S>,
    cx: &mut std::task::Context<'_>,
) -> std::task::Poll<Option<S::Item>> {
    stream.as_mut().poll_next(cx)
}
