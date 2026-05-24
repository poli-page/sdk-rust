#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Poli Page SDK for Rust — render PDFs from HTML templates via the Poli Page API.
//!
//! See [`PoliPage`] for the entry-point client.

pub mod client;
pub mod error;
pub mod render;
pub mod retry;
pub mod types;

pub use client::{PoliPage, PoliPageBuilder};
pub use error::{error_codes, Error};
pub use render::Render;
pub use retry::RetryEvent;
pub use types::{
    Environment, InlineModeInput, MetadataValue, Orientation, PageFormat, PreviewResult,
    ProjectModeInput, RenderInput, RenderMetadata,
};

pub(crate) mod internal;
