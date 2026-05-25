//! Thumbnail input and response types for `documents.thumbnails`.
//!
//! Spec §6.3 / Node `types.ts:148-169`.

use serde::{Deserialize, Serialize};

/// Output image format for thumbnails. Defaults to PNG on the server when the
/// caller omits the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThumbnailFormat {
    /// PNG — lossless. Larger files, suitable for crisp UI thumbnails.
    Png,
    /// JPEG — lossy. Smaller files; `quality` (1-100) tunes the trade-off.
    Jpeg,
    /// Catch-all for forward-compat (spec §9.2).
    #[serde(other)]
    Unknown,
}

/// Options accepted by [`crate::Documents::thumbnails`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbnailOptions {
    /// Thumbnail width in pixels. Required by the API.
    pub width: u32,

    /// Output image format. Server default: PNG.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<ThumbnailFormat>,

    /// JPEG quality (1-100). Only valid when `format` is [`ThumbnailFormat::Jpeg`];
    /// ignored otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<u8>,

    /// Generate thumbnails only for these (1-based) pages. `None` produces
    /// thumbnails for every page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<Vec<u32>>,
}

impl ThumbnailOptions {
    /// Minimal constructor — most callers only set `width`.
    ///
    /// ```
    /// use poli_page::ThumbnailOptions;
    /// let opts = ThumbnailOptions::new(840);
    /// assert_eq!(opts.width, 840);
    /// ```
    #[must_use]
    pub fn new(width: u32) -> Self {
        Self {
            width,
            format: None,
            quality: None,
            pages: None,
        }
    }
}

/// A single thumbnail returned by [`crate::Documents::thumbnails`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thumbnail {
    /// 1-based page number this thumbnail corresponds to.
    pub page: u32,
    /// Rendered width in pixels.
    pub width: u32,
    /// Rendered height in pixels (derived from the page aspect ratio).
    pub height: u32,
    /// Image MIME type (e.g., `"image/png"`, `"image/jpeg"`).
    pub content_type: String,
    /// Base64-encoded image bytes.
    pub data: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn options_serializes_only_required_field_by_default() {
        let opts = ThumbnailOptions::new(840);
        let wire = serde_json::to_value(&opts).unwrap();
        let obj = wire.as_object().unwrap();
        assert_eq!(wire["width"], 840);
        assert_eq!(obj.len(), 1, "unexpected fields: {wire}");
    }

    #[test]
    fn options_serializes_all_fields_when_set() {
        let opts = ThumbnailOptions {
            width: 320,
            format: Some(ThumbnailFormat::Jpeg),
            quality: Some(85),
            pages: Some(vec![1, 2, 3]),
        };
        let wire = serde_json::to_value(&opts).unwrap();
        assert_eq!(wire["width"], 320);
        assert_eq!(wire["format"], "jpeg");
        assert_eq!(wire["quality"], 85);
        assert_eq!(wire["pages"], json!([1, 2, 3]));
    }

    #[test]
    fn format_round_trips_through_serde() {
        for (input, json) in [
            (ThumbnailFormat::Png, r#""png""#),
            (ThumbnailFormat::Jpeg, r#""jpeg""#),
        ] {
            assert_eq!(serde_json::to_string(&input).unwrap(), json);
            let back: ThumbnailFormat = serde_json::from_str(json).unwrap();
            assert_eq!(back, input);
        }
    }

    #[test]
    fn format_deserializes_unknown_as_catch_all() {
        let v: ThumbnailFormat = serde_json::from_str(r#""webp""#).unwrap();
        assert_eq!(v, ThumbnailFormat::Unknown);
    }

    #[test]
    fn thumbnail_deserializes_from_wire_shape() {
        let body = json!({
            "page": 1,
            "width": 840,
            "height": 1188,
            "contentType": "image/png",
            "data": "iVBORw0KGgoAAAA="
        });
        let t: Thumbnail = serde_json::from_value(body).unwrap();
        assert_eq!(t.page, 1);
        assert_eq!(t.width, 840);
        assert_eq!(t.height, 1188);
        assert_eq!(t.content_type, "image/png");
        assert_eq!(t.data, "iVBORw0KGgoAAAA=");
    }
}
