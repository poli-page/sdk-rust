//! `PreviewResult` — the parsed response from `render.preview`.

use serde::{Deserialize, Serialize};

/// The API environment a request was served from.
///
/// Carries a `#[serde(other)]` catch-all for forward-compat (spec §9.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    /// Test / sandbox environment — billed but isolated from production.
    Sandbox,
    /// Live / production environment.
    Live,
    /// Catch-all for any future environment value.
    #[serde(other)]
    Unknown,
}

/// The result of `client.render.preview(input)`.
///
/// Returned by both project-mode and inline-mode renders; the API echoes the
/// same shape in either case.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewResult {
    /// Paginated HTML output ready for display.
    pub html: String,
    /// Total number of pages in the rendered preview.
    pub total_pages: u32,
    /// Which API environment served the request.
    pub environment: Environment,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_canonical_wire_shape() {
        let body = r#"{
            "html": "<p>preview</p>",
            "totalPages": 3,
            "environment": "sandbox"
        }"#;
        let r: PreviewResult = serde_json::from_str(body).unwrap();
        assert_eq!(r.html, "<p>preview</p>");
        assert_eq!(r.total_pages, 3);
        assert_eq!(r.environment, Environment::Sandbox);
    }

    #[test]
    fn deserializes_live_environment() {
        let body = r#"{ "html": "x", "totalPages": 1, "environment": "live" }"#;
        let r: PreviewResult = serde_json::from_str(body).unwrap();
        assert_eq!(r.environment, Environment::Live);
    }

    #[test]
    fn deserializes_unknown_environment_via_catch_all() {
        let body = r#"{ "html": "x", "totalPages": 1, "environment": "preview" }"#;
        let r: PreviewResult = serde_json::from_str(body).unwrap();
        assert_eq!(r.environment, Environment::Unknown);
    }
}
