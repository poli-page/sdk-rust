//! Page-format and orientation enums (spec §3.4, Node `types.ts:7-21`).
//!
//! Both are open sets: a value the API adds lands in `Unknown(String)`,
//! verbatim, and serializes back unchanged (spec §10).

open_string_enum! {
    /// Canonical Poli Page page formats. The full list mirrors the Node SDK.
    pub enum PageFormat {
        /// ISO A3 (297 × 420 mm).
        A3 = "A3",
        /// ISO A4 (210 × 297 mm) — the default for most templates.
        A4 = "A4",
        /// ISO A5 (148 × 210 mm).
        A5 = "A5",
        /// ISO A6 (105 × 148 mm).
        A6 = "A6",
        /// ISO B4 (250 × 353 mm).
        B4 = "B4",
        /// ISO B5 (176 × 250 mm).
        B5 = "B5",
        /// US Letter (8.5 × 11 in).
        Letter = "Letter",
        /// US Legal (8.5 × 14 in).
        Legal = "Legal",
        /// US Tabloid / Ledger (11 × 17 in).
        Tabloid = "Tabloid",
        /// US Executive (7.25 × 10.5 in).
        Executive = "Executive",
        /// US Statement (5.5 × 8.5 in).
        Statement = "Statement",
        /// US Folio (8.5 × 13 in).
        Folio = "Folio",
    }
}

open_string_enum! {
    /// Page orientation override.
    pub enum Orientation {
        /// Portrait (taller than wide) — typically the default for most formats.
        Portrait = "portrait",
        /// Landscape (wider than tall).
        Landscape = "landscape",
    }
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
    fn page_format_keeps_an_unknown_value_verbatim() {
        let v: PageFormat = serde_json::from_str(r#""FUTURE_FORMAT""#).unwrap();
        assert_eq!(v, PageFormat::Unknown("FUTURE_FORMAT".into()));
        assert_eq!(serde_json::to_string(&v).unwrap(), r#""FUTURE_FORMAT""#);
        assert_eq!(v.as_str(), "FUTURE_FORMAT");
    }

    #[test]
    fn from_str_maps_known_values_to_their_variant() {
        assert_eq!(PageFormat::from("Letter"), PageFormat::Letter);
        assert_eq!(Orientation::from("landscape"), Orientation::Landscape);
        // The wire values are case-sensitive, as the API's.
        assert_eq!(PageFormat::from("a4"), PageFormat::Unknown("a4".into()));
        assert_eq!(PageFormat::A4.to_string(), "A4");
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
    fn orientation_keeps_an_unknown_value_verbatim() {
        let v: Orientation = serde_json::from_str(r#""diagonal""#).unwrap();
        assert_eq!(v, Orientation::Unknown("diagonal".into()));
        assert_eq!(serde_json::to_string(&v).unwrap(), r#""diagonal""#);
    }
}
