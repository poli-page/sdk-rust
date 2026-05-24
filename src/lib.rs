#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Poli Page SDK for Rust — render PDFs from HTML templates via the Poli Page API.
//!
//! This is the Phase 0 scaffold; the real client surface lands in later phases.
//! See the implementation plan at the repository root for the build order.

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
