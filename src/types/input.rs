//! Input types accepted by the render namespace (spec §5.4 / §9.1).

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::page_format::{Orientation, PageFormat};

/// Free-form caller metadata. Forwarded to the API as-is and echoed back on
/// responses that support it. Not interpreted, indexed, or validated by the
/// SDK. Values are limited to primitives — nested objects and arrays are
/// rejected by the wire format.
pub type RenderMetadata = HashMap<String, MetadataValue>;

/// One value in a [`RenderMetadata`] map. Primitives only — nested
/// objects/arrays aren't part of the wire shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MetadataValue {
    /// UTF-8 string value.
    String(String),
    /// 64-bit signed integer value.
    Int(i64),
    /// 64-bit floating-point value.
    Float(f64),
    /// Boolean value.
    Bool(bool),
}

impl From<&str> for MetadataValue {
    fn from(s: &str) -> Self {
        MetadataValue::String(s.to_string())
    }
}
impl From<String> for MetadataValue {
    fn from(s: String) -> Self {
        MetadataValue::String(s)
    }
}
impl From<i64> for MetadataValue {
    fn from(n: i64) -> Self {
        MetadataValue::Int(n)
    }
}
impl From<i32> for MetadataValue {
    fn from(n: i32) -> Self {
        MetadataValue::Int(n.into())
    }
}
impl From<f64> for MetadataValue {
    fn from(f: f64) -> Self {
        MetadataValue::Float(f)
    }
}
impl From<bool> for MetadataValue {
    fn from(b: bool) -> Self {
        MetadataValue::Bool(b)
    }
}

/// Render a template of a stored project, resolved by slug.
///
/// Used by `render.pdf`, `render.pdf_stream`, `render.document` and
/// `render_to_file`. The API renders the project's copy published in the
/// API key's environment; there is no version to pick.
///
/// The wire body holds only `project`, `template`, `data`, `metadata`,
/// `format`, `orientation` and `locale`; unset optional fields are omitted.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderInput {
    /// Project slug (e.g. `"billing"`).
    pub project: String,
    /// Template slug within the project (e.g. `"invoice"`).
    pub template: String,
    /// Template data — variables, locale hints, etc. Required.
    pub data: serde_json::Value,

    /// Override the template's default page format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<PageFormat>,

    /// Override the template's default orientation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<Orientation>,

    /// BCP 47 locale (e.g. `"en-US"`, `"fr-FR"`) for page numbers and
    /// number/date formatting.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,

    /// Caller-supplied metadata — see [`RenderMetadata`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<RenderMetadata>,

    /// Override the auto-generated UUID v4 idempotency key. Stripped from the
    /// wire body and sent as the `Idempotency-Key` header instead.
    #[serde(skip)]
    pub idempotency_key: Option<String>,

    /// Per-call timeout override. Stripped from the wire body and consumed by
    /// the SDK's transport layer.
    #[serde(skip)]
    pub timeout: Option<Duration>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // --- MetadataValue round-trip ---

    #[test]
    fn metadata_value_serializes_each_variant_transparently() {
        assert_eq!(
            serde_json::to_value(MetadataValue::String("x".into())).unwrap(),
            json!("x"),
        );
        assert_eq!(
            serde_json::to_value(MetadataValue::Int(42)).unwrap(),
            json!(42),
        );
        assert_eq!(
            serde_json::to_value(MetadataValue::Float(1.5)).unwrap(),
            json!(1.5),
        );
        assert_eq!(
            serde_json::to_value(MetadataValue::Bool(true)).unwrap(),
            json!(true),
        );
    }

    #[test]
    fn metadata_value_deserializes_from_json_primitives() {
        let s: MetadataValue = serde_json::from_value(json!("x")).unwrap();
        assert_eq!(s, MetadataValue::String("x".into()));
        // Integers come back as Int (untagged tries variants in source order).
        let n: MetadataValue = serde_json::from_value(json!(42)).unwrap();
        assert_eq!(n, MetadataValue::Int(42));
        let b: MetadataValue = serde_json::from_value(json!(false)).unwrap();
        assert_eq!(b, MetadataValue::Bool(false));
    }

    // --- RenderInput wire body ---

    fn wire_keys(input: &RenderInput) -> Vec<String> {
        let wire = serde_json::to_value(input).unwrap();
        let mut keys: Vec<String> = wire.as_object().unwrap().keys().cloned().collect();
        keys.sort_unstable();
        keys
    }

    #[test]
    fn render_input_serializes_required_fields_camel_case() {
        let input = RenderInput {
            project: "billing".into(),
            template: "invoice".into(),
            data: json!({ "amount": 1280 }),
            ..Default::default()
        };
        let wire = serde_json::to_value(&input).unwrap();
        assert_eq!(wire["project"], "billing");
        assert_eq!(wire["template"], "invoice");
        assert_eq!(wire["data"], json!({ "amount": 1280 }));
    }

    #[test]
    fn render_input_omits_unset_optional_fields() {
        let input = RenderInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            ..Default::default()
        };
        assert_eq!(wire_keys(&input), ["data", "project", "template"]);
    }

    #[test]
    fn render_input_wire_body_is_exactly_the_seven_documented_fields() {
        let mut m = RenderMetadata::new();
        m.insert("k".into(), "v".into());
        let input = RenderInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            format: Some(PageFormat::A4),
            orientation: Some(Orientation::Portrait),
            locale: Some("en-US".into()),
            metadata: Some(m),
            idempotency_key: Some("idem-1".into()),
            timeout: Some(Duration::from_secs(30)),
        };
        assert_eq!(
            wire_keys(&input),
            [
                "data",
                "format",
                "locale",
                "metadata",
                "orientation",
                "project",
                "template"
            ],
        );
    }

    #[test]
    fn render_input_serializes_metadata_when_set() {
        let mut m = RenderMetadata::new();
        m.insert("customerId".into(), "cust_1".into());
        m.insert("amount".into(), 1280i64.into());
        let input = RenderInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            metadata: Some(m),
            ..Default::default()
        };
        let wire = serde_json::to_value(&input).unwrap();
        assert_eq!(wire["metadata"]["customerId"], "cust_1");
        assert_eq!(wire["metadata"]["amount"], 1280);
    }
}
