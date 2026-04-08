//! MSVC and CRT Function Signatures
//!
//! Collection of binary patterns for identifying MSVC CRT functions,
//! standard library functions, and common patterns in Windows binaries.
//! Data is loaded from JSON at compile time via `include_str!`.

use serde::Deserialize;

use super::signature::FunctionSignature;

/// Legacy format: `{ "name": "...", "pattern": "55 8B EC ?? 6A" }` (no mask field).
#[derive(Deserialize)]
struct JsonMsvcSignature {
    name: String,
    pattern: String,
}

/// CRT pattern format: `{ "name": "...", "library": "...", "pattern": "hex...", "mask": "hex..." }`.
///
/// Mask byte semantics:
/// - `FF` → fixed: use the pattern byte
/// - `00` or `??` → wildcard: any byte matches
#[derive(Deserialize)]
struct JsonCrtSignature {
    name: String,
    #[allow(dead_code)]
    library: String,
    pattern: String,
    mask: String,
}

/// Parse a single hex token (`"4C"`, `"??"`) into a byte or `None`.
fn parse_hex_token(s: &str) -> Option<Option<u8>> {
    let s = s.trim();
    if s == "??" || s == "00" || s.is_empty() {
        Some(None)
    } else {
        u8::from_str_radix(s, 16).ok().map(Some)
    }
}

/// Convert a CRT-format `pattern` + `mask` pair into a `Vec<Option<u8>>` pattern.
///
/// Rules:
/// - Pattern token `??` → wildcard
/// - Mask token `00` or `??` → wildcard  
/// - Otherwise → `Some(pattern_byte)`
fn crt_pattern_to_vec(pattern: &str, mask: &str) -> Vec<Option<u8>> {
    let pat_tokens: Vec<&str> = pattern.split_whitespace().collect();
    let mask_tokens: Vec<&str> = mask.split_whitespace().collect();

    pat_tokens
        .iter()
        .enumerate()
        .map(|(i, pat)| {
            // If pattern itself is wildcard, always wildcard.
            if *pat == "??" {
                return None;
            }
            // Check mask: if mask token is wildcard/zero, treat as wildcard.
            if let Some(m) = mask_tokens.get(i) {
                let m = m.trim();
                if m == "??" || m == "00" {
                    return None;
                }
            }
            // Otherwise parse as fixed byte.
            u8::from_str_radix(pat.trim(), 16).ok()
        })
        .collect()
}

/// Load all MSVC/CRT signatures into the provided vector.
///
/// Two sources are merged:
/// 1. `data/signatures/msvc.json` — legacy `from_hex` format (may be empty)
/// 2. `data/signatures/msvc_x64_crt.json` — CRT pattern+mask format
pub fn load_msvc_signatures(signatures: &mut Vec<FunctionSignature>) {
    // Legacy source (kept for backwards compatibility; currently empty).
    let legacy_json = include_str!("../data/signatures/msvc.json");
    let legacy_items: Vec<JsonMsvcSignature> = serde_json::from_str(legacy_json)
        .unwrap_or_else(|e| panic!(
            "Failed to parse msvc.json — check data/signatures/msvc.json syntax: {e}"
        ));
    for item in legacy_items {
        signatures.push(FunctionSignature::from_hex(&item.name, &item.pattern));
    }

    // CRT pattern+mask source.
    let crt_json = include_str!("../data/signatures/msvc_x64_crt.json");
    let crt_items: Vec<JsonCrtSignature> = serde_json::from_str(crt_json)
        .unwrap_or_else(|e| panic!(
            "Failed to parse msvc_x64_crt.json — check data/signatures/msvc_x64_crt.json syntax: {e}"
        ));
    for item in crt_items {
        let pattern = crt_pattern_to_vec(&item.pattern, &item.mask);
        let sig = FunctionSignature {
            name: item.name,
            pattern,
            min_size: 8,
            params: Vec::new(),
            ret_type: String::new(),
            expected_callees: Vec::new(),
            expected_callers: Vec::new(),
            force_relation: false,
            confidence: 90,
        };
        signatures.push(sig);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crt_pattern_all_fixed() {
        let result = crt_pattern_to_vec("4C 8B DC 48 83 EC", "FF FF FF FF FF FF");
        assert_eq!(
            result,
            vec![
                Some(0x4C),
                Some(0x8B),
                Some(0xDC),
                Some(0x48),
                Some(0x83),
                Some(0xEC)
            ]
        );
    }

    #[test]
    fn crt_pattern_with_mask_wildcard() {
        // mask "??" means wildcard
        let result = crt_pattern_to_vec("48 83 EC 28 48 85 C9", "FF FF FF ?? FF FF FF");
        assert_eq!(result[3], None, "mask wildcard should be None");
        assert_eq!(result[0], Some(0x48));
    }

    #[test]
    fn crt_pattern_with_pat_wildcard() {
        // pattern "??" means wildcard regardless of mask
        let result = crt_pattern_to_vec("48 85 C9 74 ?? 48 83 EC", "FF FF FF FF ?? FF FF FF");
        assert_eq!(result[4], None, "pattern wildcard should be None");
    }

    #[test]
    fn load_msvc_signatures_loads_crt() {
        let mut sigs = Vec::new();
        load_msvc_signatures(&mut sigs);
        // CRT JSON has 10 entries; there should be at least that many.
        assert!(sigs.len() >= 10, "expected at least 10 CRT signatures, got {}", sigs.len());
        let names: Vec<&str> = sigs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"memcpy"), "memcpy should be present");
        assert!(names.contains(&"malloc"), "malloc should be present");
        assert!(names.contains(&"strlen"), "strlen should be present");
    }

    #[test]
    fn memcpy_matches_known_prologue() {
        let mut sigs = Vec::new();
        load_msvc_signatures(&mut sigs);
        let memcpy_sig = sigs.iter().find(|s| s.name == "memcpy").expect("memcpy missing");
        // UCRT memcpy x64 prologue: 4C 8B DC 48 83 EC ...
        let bytes: &[u8] = &[0x4C, 0x8B, 0xDC, 0x48, 0x83, 0xEC, 0x48, 0x00];
        assert!(memcpy_sig.matches(bytes), "memcpy pattern should match known bytes");
    }

    #[test]
    fn database_contains_crt_entries() {
        use crate::SignatureDatabase;
        let db = SignatureDatabase::new();
        assert!(db.signatures().len() >= 10, "SignatureDatabase should have CRT patterns");
    }
}
