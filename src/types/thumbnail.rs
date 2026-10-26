//! Thumbnail input and response types for `documents.thumbnails`.
//!
//! Spec §6.3 / Node `types.ts:148-169`.

use serde::{Deserialize, Serialize};

/// Output image format for thumbnails. The API defaults to JPEG when the
/// caller omits the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThumbnailFormat {
    /// JPEG — lossy. Smaller files; `quality` (1-100) tunes the trade-off.
    Jpeg,
    /// WebP — compact, supported by every current browser.
    Webp,
    /// PNG — lossless. Larger files, suitable for crisp UI thumbnails.
    Png,
    /// Catch-all for forward-compat (spec §9.2).
    #[serde(other)]
    Unknown,
}

/// Options accepted by [`crate::Documents::thumbnails`]. Every option is
/// optional; an unset option is not sent and the API default applies. The
/// SDK does not check bounds: out-of-range values get `400 VALIDATION_ERROR`
/// from the API.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThumbnailOptions {
    /// Thumbnail width in pixels (50-2000). API default: 400.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,

    /// Output image format. API default: [`ThumbnailFormat::Jpeg`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<ThumbnailFormat>,

    /// JPEG quality (1-100). Only valid when `format` is [`ThumbnailFormat::Jpeg`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<u8>,

    /// Generate thumbnails only for these (1-based) pages. `None` produces
    /// thumbnails for every page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pages: Option<Vec<u32>>,
}

impl ThumbnailOptions {
    /// Options with nothing set — every API default applies.
    ///
    /// ```
    /// use poli_page::ThumbnailOptions;
    /// let opts = ThumbnailOptions::new().width(840);
    /// assert_eq!(opts.width, Some(840));
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the thumbnail width in pixels.
    #[must_use]
    pub fn width(mut self, width: u32) -> Self {
        self.width = Some(width);
        self
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
    fn options_serialize_to_an_empty_object_by_default() {
        let wire = serde_json::to_value(ThumbnailOptions::new()).unwrap();
        assert_eq!(wire, json!({}));
    }

    #[test]
    fn width_builder_sets_only_width() {
        let wire = serde_json::to_value(ThumbnailOptions::new().width(840)).unwrap();
        assert_eq!(wire, json!({ "width": 840 }));
    }

    #[test]
    fn options_serializes_all_fields_when_set() {
        let opts = ThumbnailOptions {
            width: Some(320),
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
            (ThumbnailFormat::Jpeg, r#""jpeg""#),
            (ThumbnailFormat::Webp, r#""webp""#),
            (ThumbnailFormat::Png, r#""png""#),
        ] {
            assert_eq!(serde_json::to_string(&input).unwrap(), json);
            let back: ThumbnailFormat = serde_json::from_str(json).unwrap();
            assert_eq!(back, input);
        }
    }

    #[test]
    fn format_deserializes_unknown_as_catch_all() {
        let v: ThumbnailFormat = serde_json::from_str(r#""avif""#).unwrap();
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
