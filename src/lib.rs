#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::pedantic, clippy::cargo)]
// Allow-list for pedantic/cargo lints: each entry is either a known
// false-positive class or a constraint the SDK can't satisfy.
#![allow(
    // SDK modules name types after themselves by convention (Render, Documents).
    clippy::module_name_repetitions,
    // Blocking-module methods delegate to async equivalents; duplicating
    // `# Errors` sections on every wrapper is busywork.
    clippy::missing_errors_doc,
    // Transitive dep graph (windows-sys, rand) routinely ships multiple
    // versions; not actionable from this crate.
    clippy::multiple_crate_versions,
    // Casts we deliberately accept at the boundary (e.g. usize → i64 for
    // serde-json numeric output bounded by realistic PDF sizes).
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    // `Duration::from_mins` / `from_hours` are 1.94+; MSRV is 1.93.
    clippy::duration_suboptimal_units,
    // Too many false positives on plain English ("API", "JSON", etc.).
    clippy::doc_markdown,
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Poli Page SDK for Rust — render PDFs from HTML templates via the Poli Page API.
//!
//! See [`PoliPage`] for the entry-point client.

pub mod client;
pub mod documents;
pub mod error;
pub mod fs;
pub mod render;
pub mod retry;
pub mod types;

#[cfg(feature = "blocking")]
#[cfg_attr(docsrs, doc(cfg(feature = "blocking")))]
pub mod blocking;

pub use client::{PoliPage, PoliPageBuilder};
pub use documents::Documents;
pub use error::{error_codes, Error};
pub use fs::render_to_file;
pub use render::Render;
pub use retry::RetryEvent;
pub use types::{
    DocumentDescriptor, DocumentPreviewResult, Environment, InlineModeInput, MetadataValue,
    Orientation, PageFormat, PreviewResult, ProjectModeInput, RenderInput, RenderMetadata,
    Thumbnail, ThumbnailFormat, ThumbnailOptions,
};

pub(crate) mod internal;
