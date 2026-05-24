//! Sync `render_to_file` plus the [`BlockingPdfReader`] adapter.

use std::future::poll_fn;
use std::io::{self, Read};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;

use bytes::Bytes;
use futures_core::Stream;
use tokio::runtime::Runtime;

use crate::client::PdfByteStream;
use crate::Error;
use crate::ProjectModeInput;

use super::PoliPage;

/// `std::io::Read` adapter over a [`PdfByteStream`]. Each call to
/// [`Read::read`] block_ons enough of the stream to satisfy the request.
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
}

impl Read for BlockingPdfReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        if self.leftover.is_empty() && !self.finished {
            // Pull the next chunk synchronously by block_on'ing the underlying
            // Stream's poll_next. `poll_fn` adapts the raw poll into a future
            // we can hand to `block_on`.
            let stream = &mut self.stream;
            let next = self
                .runtime
                .block_on(poll_fn(|cx| Pin::new(&mut **stream).poll_next(cx)));
            match next {
                None => {
                    self.finished = true;
                    return Ok(0);
                }
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
/// Creates parent directories if missing. Overwrites existing files. Streams
/// PDF bytes into the target via `std::io::copy` — bounded memory regardless
/// of PDF size.
///
/// # Errors
///
/// Same shape as [`crate::render_to_file`].
pub fn render_to_file(
    client: &PoliPage,
    input: ProjectModeInput,
    path: impl AsRef<Path>,
) -> Result<(), Error> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| Error::InvalidOptions {
                message: format!("failed to create directory {}: {}", parent.display(), e),
            })?;
        }
    }
    let mut reader = client.render.pdf_stream(input)?;
    let mut file = std::fs::File::create(path).map_err(|e| Error::InvalidOptions {
        message: format!("failed to create file {}: {}", path.display(), e),
    })?;
    std::io::copy(&mut reader, &mut file).map_err(|e| Error::InvalidOptions {
        message: format!("failed to copy to {}: {}", path.display(), e),
    })?;
    Ok(())
}
