//! `DocumentDescriptor` — the parsed wire shape returned by `render.document`
//! (and Phase 4's `documents.get`), plus a `download_pdf()` convenience.
//!
//! Spec §3.4.

use std::sync::Arc;

use bytes::Bytes;
use serde::Deserialize;

use crate::client::ClientInner;
use crate::types::page_format::{Orientation, PageFormat};
use crate::types::preview::Environment;
use crate::{Error, RenderMetadata};

/// A stored document returned by `client.render.document` and (in Phase 4)
/// `client.documents.get`.
///
/// Top-level fields are system-controlled by the API; `metadata` echoes any
/// caller-supplied key/value pairs from the render request.
/// [`DocumentDescriptor::download_pdf`] fetches the PDF bytes from
/// `presigned_pdf_url` on demand — the presigned URL has a ~15 minute TTL, so
/// expired URLs require a fresh `documents.get(id)` to re-mint.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentDescriptor {
    /// Stable document identifier (e.g. `"doc_abc123"`).
    pub document_id: String,
    /// Owning organization (e.g. `"org_xyz"`).
    pub organization_id: String,
    /// Project ID — `None` when the document was rendered in inline mode.
    pub project_id: Option<String>,
    /// Project slug — same `None` rule.
    pub project_slug: Option<String>,
    /// Template ID — `None` for inline-mode renders.
    pub template_id: Option<String>,
    /// Template slug — same `None` rule.
    pub template_slug: Option<String>,
    /// Resolved template version (e.g. `"1.0.0"` or `"draft"`).
    pub version: Option<String>,
    /// Sandbox vs live environment.
    pub environment: Environment,
    /// API key ID that issued the render — useful for audit trails.
    pub api_key_id: Option<String>,
    /// Resolved page format (e.g. `A4`).
    pub format: PageFormat,
    /// Resolved orientation, when explicit.
    pub orientation: Option<Orientation>,
    /// Resolved BCP-47 locale.
    pub locale: Option<String>,
    /// Number of pages in the rendered PDF.
    pub page_count: u32,
    /// Size of the stored PDF in bytes.
    pub size_bytes: u64,
    /// Creation timestamp (ISO 8601 string; not parsed — no `chrono` dep).
    pub created_at: String,
    /// Caller-supplied metadata, echoed back. Always non-`None` (defaults to
    /// an empty map when the server omits the field).
    #[serde(default)]
    pub metadata: RenderMetadata,
    /// Time-limited URL for the stored PDF. Use [`Self::download_pdf`] to fetch.
    pub presigned_pdf_url: String,
    /// Expiry timestamp for `presigned_pdf_url` (ISO 8601, not parsed).
    pub expires_at: String,

    /// Back-reference to the parent client so [`Self::download_pdf`] can reuse
    /// its `reqwest::Client` (and the connection pool). Skipped by serde so
    /// the wire shape stays unchanged; populated by [`attach_client`] after
    /// deserialization.
    #[serde(skip)]
    pub(crate) client: Option<Arc<ClientInner>>,
}

impl DocumentDescriptor {
    /// Fetch the PDF bytes from [`Self::presigned_pdf_url`].
    ///
    /// The fetch is **unauthenticated** (the presigned URL carries its own
    /// signature) and **not subject to the SDK's retry policy** — expired or
    /// already-downloaded URLs return immediately rather than waiting on
    /// retry backoff.
    ///
    /// # Errors
    ///
    /// - [`Error::Download`] on non-2xx response or network failure from the
    ///   presigned URL fetch.
    /// - [`Error::InvalidOptions`] when the descriptor lacks a client
    ///   back-reference (only happens if the caller manually constructed a
    ///   `DocumentDescriptor` via deserialization instead of obtaining it
    ///   from a client method).
    pub async fn download_pdf(&self) -> Result<Bytes, Error> {
        let client = self.client.as_ref().ok_or_else(|| Error::InvalidOptions {
            message: "DocumentDescriptor is missing its client back-reference \
                (was it deserialized outside an SDK method?)"
                .into(),
        })?;
        client.fetch_bytes(&self.presigned_pdf_url).await
    }
}

/// Attach a client back-reference to a freshly-deserialized descriptor.
/// Called by `render.document` (and Phase 4's `documents.*` methods) before
/// returning the descriptor to the caller.
pub(crate) fn attach_client(
    mut descriptor: DocumentDescriptor,
    client: Arc<ClientInner>,
) -> DocumentDescriptor {
    descriptor.client = Some(client);
    descriptor
}

/// Result of [`crate::Documents::preview`].
///
/// Assembled from the `text/html` response body plus the `X-Document-Page-Count`
/// header — `documents.preview` is the only SDK endpoint that returns
/// `text/html` directly (every other endpoint returns JSON or PDF bytes).
///
/// **Note**: the field is `page_count` (singular), not `total_pages`. That
/// matches the deployed API's header name and differs from
/// [`crate::PreviewResult::total_pages`] — spec §5.5 / Node SDK commit
/// `8523e13`.
#[derive(Debug, Clone)]
pub struct DocumentPreviewResult {
    /// The stored paginated HTML, exactly as the API returned it.
    pub html: String,
    /// Page count from the `X-Document-Page-Count` header. Defaults to `0`
    /// when the header is absent or unparseable (NaN-tolerant — Node behavior).
    pub page_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_wire() -> serde_json::Value {
        json!({
            "documentId": "doc_abc123",
            "organizationId": "org_xyz",
            "projectId": "proj_42",
            "projectSlug": "billing",
            "templateId": "tpl_invoice_v1",
            "templateSlug": "invoice",
            "version": "1.0.0",
            "environment": "live",
            "apiKeyId": "key_live_abc",
            "format": "A4",
            "orientation": "portrait",
            "locale": "en-US",
            "pageCount": 2,
            "sizeBytes": 38421,
            "createdAt": "2026-04-30T19:45:22Z",
            "metadata": {},
            "presignedPdfUrl": "https://s3.example/x.pdf",
            "expiresAt": "2026-04-30T20:00:22Z"
        })
    }

    #[test]
    fn deserializes_full_descriptor_with_camel_case_renames() {
        let d: DocumentDescriptor = serde_json::from_value(sample_wire()).unwrap();
        assert_eq!(d.document_id, "doc_abc123");
        assert_eq!(d.organization_id, "org_xyz");
        assert_eq!(d.project_slug.as_deref(), Some("billing"));
        assert_eq!(d.template_slug.as_deref(), Some("invoice"));
        assert_eq!(d.version.as_deref(), Some("1.0.0"));
        assert_eq!(d.environment, Environment::Live);
        assert_eq!(d.format, PageFormat::A4);
        assert_eq!(d.orientation, Some(Orientation::Portrait));
        assert_eq!(d.locale.as_deref(), Some("en-US"));
        assert_eq!(d.page_count, 2);
        assert_eq!(d.size_bytes, 38421);
        assert_eq!(d.presigned_pdf_url, "https://s3.example/x.pdf");
        assert!(d.metadata.is_empty());
        // Back-ref not attached yet — that's `attach_client`'s job.
        assert!(d.client.is_none());
    }

    #[test]
    fn deserializes_nullable_wire_fields_as_none() {
        // Inline-mode renders produce a descriptor with project/template IDs
        // set to null on the wire.
        let mut wire = sample_wire();
        let obj = wire.as_object_mut().unwrap();
        obj.insert("projectId".into(), json!(null));
        obj.insert("projectSlug".into(), json!(null));
        obj.insert("templateId".into(), json!(null));
        obj.insert("orientation".into(), json!(null));
        let d: DocumentDescriptor = serde_json::from_value(wire).unwrap();
        assert!(d.project_id.is_none());
        assert!(d.project_slug.is_none());
        assert!(d.template_id.is_none());
        assert!(d.orientation.is_none());
    }

    #[test]
    fn metadata_defaults_to_empty_when_field_omitted() {
        // Defense-in-depth: the server always echoes `{}`, but our parser
        // tolerates a missing `metadata` key per spec §3.4.
        let mut wire = sample_wire();
        wire.as_object_mut().unwrap().remove("metadata");
        let d: DocumentDescriptor = serde_json::from_value(wire).unwrap();
        assert!(d.metadata.is_empty());
    }

    #[tokio::test]
    async fn download_pdf_without_back_reference_errors_invalid_options() {
        let d: DocumentDescriptor = serde_json::from_value(sample_wire()).unwrap();
        let err = d.download_pdf().await.expect_err("no client");
        assert!(matches!(err, Error::InvalidOptions { .. }));
        assert!(err.to_string().contains("back-reference"));
    }
}
