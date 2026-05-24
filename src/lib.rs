#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Poli Page SDK for Rust — render PDFs from HTML templates via the Poli Page API.
//!
//! See [`PoliPage`] for the entry-point client.

pub mod client;
pub mod documents;
pub mod error;
pub mod render;
pub mod retry;
pub mod types;

pub use client::{PoliPage, PoliPageBuilder};
pub use documents::Documents;
pub use error::{error_codes, Error};
pub use render::Render;
pub use retry::RetryEvent;
pub use types::{
    DocumentDescriptor, DocumentPreviewResult, Environment, InlineModeInput, MetadataValue,
    Orientation, PageFormat, PreviewResult, ProjectModeInput, RenderInput, RenderMetadata,
    Thumbnail, ThumbnailFormat, ThumbnailOptions,
};

pub(crate) mod internal;
