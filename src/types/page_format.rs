//! Page-format and orientation enums (spec §3.4, Node `types.ts:7-21`).
//!
//! Both enums carry a `#[serde(other)]` catch-all so a server-side enum
//! extension is a silent no-op for old SDK versions — per the forward-compat
//! posture in spec §9.2.

/// Canonical Poli Page page formats. The full list mirrors the Node SDK.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PageFormat {
    /// ISO A3 (297 × 420 mm).
    A3,
    /// ISO A4 (210 × 297 mm) — the default for most templates.
    A4,
    /// ISO A5 (148 × 210 mm).
    A5,
    /// ISO A6 (105 × 148 mm).
    A6,
    /// ISO B4 (250 × 353 mm).
    B4,
    /// ISO B5 (176 × 250 mm).
    B5,
    /// US Letter (8.5 × 11 in).
    Letter,
    /// US Legal (8.5 × 14 in).
    Legal,
    /// US Tabloid / Ledger (11 × 17 in).
    Tabloid,
    /// US Executive (7.25 × 10.5 in).
    Executive,
    /// US Statement (5.5 × 8.5 in).
    Statement,
    /// US Folio (8.5 × 13 in).
    Folio,
    /// Catch-all for values the SDK doesn't yet recognise (server added a new
    /// format). The raw string is not preserved at v1.0 — see spec §9.2.
    #[serde(other)]
    Unknown,
}

/// Page orientation override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    /// Portrait (taller than wide) — typically the default for most formats.
    Portrait,
    /// Landscape (wider than tall).
    Landscape,
    /// Catch-all for forward-compat (see spec §9.2).
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_format_serializes_to_canonical_string() {
        assert_eq!(serde_json::to_string(&PageFormat::A4).unwrap(), r#""A4""#);
        assert_eq!(
            serde_json::to_string(&PageFormat::Letter).unwrap(),
            r#""Letter""#,
        );
        assert_eq!(
            serde_json::to_string(&PageFormat::Tabloid).unwrap(),
            r#""Tabloid""#,
        );
    }

    #[test]
    fn page_format_deserializes_from_canonical_string() {
        let v: PageFormat = serde_json::from_str(r#""A4""#).unwrap();
        assert_eq!(v, PageFormat::A4);
    }

    #[test]
    fn page_format_unknown_variant_catches_server_additions() {
        let v: PageFormat = serde_json::from_str(r#""FUTURE_FORMAT""#).unwrap();
        assert_eq!(v, PageFormat::Unknown);
    }

    #[test]
    fn orientation_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&Orientation::Portrait).unwrap(),
            r#""portrait""#,
        );
        assert_eq!(
            serde_json::to_string(&Orientation::Landscape).unwrap(),
            r#""landscape""#,
        );
    }

    #[test]
    fn orientation_deserializes_lowercase() {
        let v: Orientation = serde_json::from_str(r#""portrait""#).unwrap();
        assert_eq!(v, Orientation::Portrait);
    }

    #[test]
    fn orientation_unknown_catches_server_additions() {
        let v: Orientation = serde_json::from_str(r#""diagonal""#).unwrap();
        assert_eq!(v, Orientation::Unknown);
    }
}
