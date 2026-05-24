//! URL path-segment percent-encoding.
//!
//! Matches the JavaScript `encodeURIComponent` unreserved + sub-delims-minus-/
//! set so `documents/{id}` paths interpolate byte-for-byte the same way as
//! the Node SDK. Hand-rolled to avoid adding `percent-encoding` as a direct
//! dep — the implementation is ~20 lines of pure stdlib.
//!
//! Spec §5.4: "Document ID path interpolation MUST be URL-encoded".

use std::fmt::Write;

/// Percent-encode `s` for use as a single URL path segment.
///
/// Preserves the same character set as JS `encodeURIComponent`:
/// `A-Z a-z 0-9 - _ . ~ ! * ' ( )`. Everything else (including `/`, which
/// would otherwise split the segment) becomes `%XX`.
pub(crate) fn encode_path_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &byte in s.as_bytes() {
        if is_safe_for_encode_uri_component(byte) {
            out.push(byte as char);
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

#[inline]
fn is_safe_for_encode_uri_component(byte: u8) -> bool {
    matches!(byte,
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
        | b'-' | b'_' | b'.' | b'~'
        | b'!' | b'*' | b'\'' | b'(' | b')'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alphanumeric_and_unreserved_pass_through_unchanged() {
        assert_eq!(encode_path_segment("doc_abc123"), "doc_abc123");
        assert_eq!(encode_path_segment("AZaz09"), "AZaz09");
        assert_eq!(encode_path_segment("-_.~!*'()"), "-_.~!*'()");
    }

    #[test]
    fn slash_becomes_percent_2f() {
        // Mirrors Node test `tests/documents.test.ts:206-208` —
        // 'doc/with/slashes' → 'doc%2Fwith%2Fslashes'.
        assert_eq!(
            encode_path_segment("doc/with/slashes"),
            "doc%2Fwith%2Fslashes",
        );
    }

    #[test]
    fn space_becomes_percent_20() {
        assert_eq!(encode_path_segment("doc id"), "doc%20id");
    }

    #[test]
    fn ampersand_question_mark_hash_get_encoded() {
        assert_eq!(encode_path_segment("a&b?c#d"), "a%26b%3Fc%23d",);
    }

    #[test]
    fn unicode_encoded_byte_by_byte_as_utf8() {
        // 'é' is U+00E9, two bytes in UTF-8: 0xC3 0xA9
        assert_eq!(encode_path_segment("café"), "caf%C3%A9");
    }

    #[test]
    fn empty_string_returns_empty_string() {
        assert_eq!(encode_path_segment(""), "");
    }
}
