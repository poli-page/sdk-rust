//! End-to-end integration tests against the live API. **Not run in normal
//! CI** — triggered only by the dedicated `.github/workflows/integration.yml`
//! nightly + push-to-main job, with `POLI_PAGE_API_KEY` injected as a secret.
//!
//! Local invocation:
//! ```text
//! POLI_PAGE_API_KEY=pp_test_... \
//!   cargo test --features integration --test integration -- --ignored
//! ```
//!
//! Tests are `#[cfg(feature = "integration")]` so the file produces zero
//! tests in the default build, and `#[ignore]` so they don't run even with
//! the feature enabled unless `--ignored` is passed.

#[cfg(feature = "integration")]
mod render;
