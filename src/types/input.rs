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

/// Render against a stored project + template by slug.
///
/// Used by `render.pdf`, `render.pdf_stream`, `render.document`, and (via
/// `Into<RenderInput>`) `render.preview`.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectModeInput {
    /// Project slug (e.g. `"billing"`).
    pub project: String,
    /// Template slug within the project (e.g. `"invoice"`).
    pub template: String,
    /// Template data — variables, locale hints, etc. Required.
    pub data: serde_json::Value,

    /// Pin to a specific published version, or `"draft"` for the in-progress
    /// version. `None` is invalid in project mode (the API will reject it
    /// with `VERSION_REQUIRED`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

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

/// Render with raw HTML inline. No project / template resolution.
///
/// Accepted only by `render.preview`; the other render methods statically
/// require [`ProjectModeInput`].
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineModeInput {
    /// Raw HTML template string. Required.
    pub template: String,
    /// Template data — variables, locale hints, etc. Required.
    pub data: serde_json::Value,

    /// Override the template's default page format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<PageFormat>,

    /// Override the template's default orientation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<Orientation>,

    /// BCP 47 locale.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,

    /// Caller-supplied metadata — see [`RenderMetadata`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<RenderMetadata>,

    /// Override the auto-generated UUID v4 idempotency key.
    #[serde(skip)]
    pub idempotency_key: Option<String>,

    /// Per-call timeout override.
    #[serde(skip)]
    pub timeout: Option<Duration>,
}

/// Input accepted by [`crate::Render::preview`]. Either project mode (resolved
/// by slug) or inline mode (raw HTML in `template`). Most call sites use the
/// `From` impls below rather than constructing the enum directly:
/// `render.preview(project_input).await` and
/// `render.preview(inline_input).await` both compile.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum RenderInput {
    /// Project-mode wrapper.
    Project(ProjectModeInput),
    /// Inline-mode wrapper.
    Inline(InlineModeInput),
}

impl From<ProjectModeInput> for RenderInput {
    fn from(p: ProjectModeInput) -> Self {
        RenderInput::Project(p)
    }
}

impl From<InlineModeInput> for RenderInput {
    fn from(i: InlineModeInput) -> Self {
        RenderInput::Inline(i)
    }
}

impl RenderInput {
    /// Idempotency-key override carried on the input (the orchestrator reads
    /// this to populate the `Idempotency-Key` header). Stripped from the
    /// serialised wire body via `#[serde(skip)]` on the underlying field.
    pub(crate) fn idempotency_key(&self) -> Option<&str> {
        match self {
            RenderInput::Project(p) => p.idempotency_key.as_deref(),
            RenderInput::Inline(i) => i.idempotency_key.as_deref(),
        }
    }

    /// Per-call timeout override (orchestrator consumes this; the SDK's
    /// per-attempt timeout falls back to the client-level default when `None`).
    pub(crate) fn timeout(&self) -> Option<Duration> {
        match self {
            RenderInput::Project(p) => p.timeout,
            RenderInput::Inline(i) => i.timeout,
        }
    }
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

    // --- ProjectModeInput wire body ---

    #[test]
    fn project_mode_serializes_required_fields_camel_case() {
        let input = ProjectModeInput {
            project: "billing".into(),
            template: "invoice".into(),
            data: json!({ "amount": 1280 }),
            version: Some("1.0.0".into()),
            ..Default::default()
        };
        let wire = serde_json::to_value(&input).unwrap();
        assert_eq!(wire["project"], "billing");
        assert_eq!(wire["template"], "invoice");
        assert_eq!(wire["version"], "1.0.0");
        assert_eq!(wire["data"], json!({ "amount": 1280 }));
    }

    #[test]
    fn project_mode_omits_unset_optional_fields() {
        let input = ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            ..Default::default()
        };
        let wire = serde_json::to_value(&input).unwrap();
        let obj = wire.as_object().unwrap();
        assert!(!obj.contains_key("version"));
        assert!(!obj.contains_key("format"));
        assert!(!obj.contains_key("orientation"));
        assert!(!obj.contains_key("locale"));
        assert!(!obj.contains_key("metadata"));
    }

    #[test]
    fn project_mode_strips_idempotency_key_and_timeout_from_wire() {
        let input = ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            idempotency_key: Some("idem-1".into()),
            timeout: Some(Duration::from_secs(30)),
            ..Default::default()
        };
        let wire = serde_json::to_value(&input).unwrap();
        let obj = wire.as_object().unwrap();
        assert!(!obj.contains_key("idempotencyKey"));
        assert!(!obj.contains_key("idempotency_key"));
        assert!(!obj.contains_key("timeout"));
    }

    #[test]
    fn project_mode_serializes_metadata_when_set() {
        let mut m = RenderMetadata::new();
        m.insert("customerId".into(), "cust_1".into());
        m.insert("amount".into(), 1280i64.into());
        let input = ProjectModeInput {
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

    // --- InlineModeInput wire body ---

    #[test]
    fn inline_mode_has_no_project_or_version_fields() {
        let input = InlineModeInput {
            template: "<h1>hi</h1>".into(),
            data: json!({}),
            ..Default::default()
        };
        let wire = serde_json::to_value(&input).unwrap();
        let obj = wire.as_object().unwrap();
        assert!(!obj.contains_key("project"));
        assert!(!obj.contains_key("version"));
        assert_eq!(wire["template"], "<h1>hi</h1>");
    }

    // --- RenderInput dispatch ---

    #[test]
    fn render_input_serializes_project_transparently() {
        let input: RenderInput = ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            ..Default::default()
        }
        .into();
        let wire = serde_json::to_value(&input).unwrap();
        assert_eq!(wire["project"], "p");
        assert!(wire.as_object().unwrap().get("template").is_some());
    }

    #[test]
    fn render_input_serializes_inline_transparently() {
        let input: RenderInput = InlineModeInput {
            template: "<p>x</p>".into(),
            data: json!({}),
            ..Default::default()
        }
        .into();
        let wire = serde_json::to_value(&input).unwrap();
        assert_eq!(wire["template"], "<p>x</p>");
        assert!(wire.as_object().unwrap().get("project").is_none());
    }

    #[test]
    fn render_input_idempotency_key_propagates_through_enum() {
        let input: RenderInput = ProjectModeInput {
            project: "p".into(),
            template: "t".into(),
            data: json!({}),
            idempotency_key: Some("idem-x".into()),
            ..Default::default()
        }
        .into();
        assert_eq!(input.idempotency_key(), Some("idem-x"));
    }

    #[test]
    fn render_input_timeout_propagates_through_enum() {
        let input: RenderInput = InlineModeInput {
            template: "<p>x</p>".into(),
            data: json!({}),
            timeout: Some(Duration::from_secs(15)),
            ..Default::default()
        }
        .into();
        assert_eq!(input.timeout(), Some(Duration::from_secs(15)));
    }
}
