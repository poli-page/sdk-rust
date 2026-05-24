#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Poli Page SDK for Rust — render PDFs from HTML templates via the Poli Page API.
//!
//! This is the Phase 1 transport-core scaffold; the orchestrating `PoliPage`
//! client lands in Phase 2. See the implementation plan at the repository
//! root for the build order.

pub mod error;

pub use error::{error_codes, Error};

pub(crate) mod internal;

/// Async client for the Poli Page API.
///
/// Phase 0 stub — replaced with the real client in Phase 2.
#[derive(Debug, Clone)]
pub struct PoliPage {
    _private: (),
}

/// Builder for [`PoliPage`]. Returned by [`PoliPage::builder`].
///
/// Phase 0 stub — replaced with the real builder in Phase 2.
#[derive(Debug, Default)]
pub struct PoliPageBuilder {
    _private: (),
}

impl PoliPage {
    /// Returns a new [`PoliPageBuilder`] for configuring the client.
    pub fn builder() -> PoliPageBuilder {
        PoliPageBuilder { _private: () }
    }
}
