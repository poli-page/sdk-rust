//! Thin wrapper over the `uuid` crate so the call sites don't depend on
//! `uuid::Uuid` directly. Mirrors Node's `crypto.randomUUID()` usage.

// Phase 1 ships the wrapper; the orchestrator that calls it lands in Phase 2.
#![allow(dead_code)]

/// Generate a fresh v4 UUID as its 36-character lowercase string form.
pub(crate) fn new_v4_string() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_v4_string_returns_36_chars() {
        let id = new_v4_string();
        assert_eq!(id.len(), 36, "got {id}");
    }

    #[test]
    fn new_v4_string_uses_canonical_format() {
        // 8-4-4-4-12 hex with the version-4 nibble in position 14.
        let id = new_v4_string();
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(parts.len(), 5, "got {id}");
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
        assert!(
            parts[2].starts_with('4'),
            "v4 version nibble missing in {id}"
        );
    }

    #[test]
    fn new_v4_string_returns_distinct_values() {
        // 100 draws should produce 100 distinct strings (collision probability
        // ≈ 4e-36 — effectively zero).
        let mut set = std::collections::HashSet::new();
        for _ in 0..100 {
            assert!(set.insert(new_v4_string()), "v4 collision in 100 draws");
        }
    }
}
