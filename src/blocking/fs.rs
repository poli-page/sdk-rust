//! Sync `render_to_file` plus the [`BlockingPdfReader`] adapter.

use std::future::poll_fn;
use std::io::{self, Read, Write};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use bytes::Bytes;
use futures_core::Stream;
use tokio::runtime::Runtime;

use crate::client::PdfByteStream;
use crate::internal::part_file::{
    io_error, keep_permissions, parent_dir, part_path, prepare_target, PartGuard,
};
use crate::Error;
use crate::RenderInput;

use super::PoliPage;

/// `std::io::Read` adapter over a [`PdfByteStream`]. Each call to
/// [`Read::read`] block_ons enough of the stream to satisfy the request.
///
/// A download failure (`DOWNLOAD_FAILED`, an idle timeout included) comes
/// back as an [`io::Error`] whose inner error is the SDK [`Error`].
pub struct BlockingPdfReader {
    runtime: Arc<Runtime>,
    stream: Pin<Box<PdfByteStream>>,
    leftover: Bytes,
    finished: bool,
}

impl std::fmt::Debug for BlockingPdfReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlockingPdfReader")
            .field("buffered", &self.leftover.len())
            .field("finished", &self.finished)
            .finish_non_exhaustive()
    }
}

impl BlockingPdfReader {
    pub(crate) fn new(runtime: Arc<Runtime>, stream: PdfByteStream) -> Self {
        Self {
            runtime,
            stream: Box::pin(stream),
            leftover: Bytes::new(),
            finished: false,
        }
    }

    /// The next chunk of the download, `None` at its end.
    fn next_chunk(&mut self) -> Option<Result<Bytes, Error>> {
        if self.finished {
            return None;
        }
        // Pull the next chunk synchronously by block_on'ing the underlying
        // Stream's poll_next. `poll_fn` adapts the raw poll into a future
        // we can hand to `block_on`.
        let stream = &mut self.stream;
        let next = self
            .runtime
            .block_on(poll_fn(|cx| Pin::new(&mut **stream).poll_next(cx)));
        if next.is_none() {
            self.finished = true;
        }
        next
    }
}

impl Read for BlockingPdfReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        if self.leftover.is_empty() {
            match self.next_chunk() {
                None => return Ok(0),
                Some(Err(e)) => return Err(io::Error::other(e)),
                Some(Ok(bytes)) => self.leftover = bytes,
            }
        }

        let n = buf.len().min(self.leftover.len());
        buf[..n].copy_from_slice(&self.leftover[..n]);
        self.leftover = self.leftover.slice(n..);
        Ok(n)
    }
}

/// Sync counterpart of [`crate::render_to_file`].
///
/// Creates parent directories if missing. Streams PDF bytes chunk by chunk —
/// bounded memory regardless of PDF size — into a temporary file next to the
/// target, renamed over it once the PDF is complete: a failed call leaves
/// the target as it was. A symbolic link is resolved first and stays a link;
/// an existing file keeps its permissions.
///
/// # Errors
///
/// Same shape as [`crate::render_to_file`]: [`Error::Io`] (`io_error`) when
/// the file cannot be written, the [`crate::Render::pdf_stream`] errors
/// otherwise.
pub fn render_to_file(
    client: &PoliPage,
    input: RenderInput,
    path: impl AsRef<Path>,
) -> Result<(), Error> {
    let target = prepare_target(path.as_ref()).map_err(io_error)?;
    if let Some(parent) = parent_dir(&target) {
        std::fs::create_dir_all(parent).map_err(io_error)?;
    }
    let part_path = part_path(&target).map_err(io_error)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&part_path)
        .map_err(io_error)?;
    // Removes the temporary file on every early return.
    let part = PartGuard::new(part_path);

    let mut reader = client.render.pdf_stream(input)?;
    while let Some(chunk) = reader.next_chunk() {
        file.write_all(&chunk?).map_err(io_error)?;
    }
    file.flush().map_err(io_error)?;
    drop(file);
    keep_permissions(&target, part.path()).map_err(io_error)?;
    std::fs::rename(part.path(), &target).map_err(io_error)?;
    part.disarm();
    Ok(())
}
