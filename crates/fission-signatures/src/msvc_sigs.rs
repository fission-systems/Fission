//! MSVC and CRT Function Signatures
//!
//! Collection of binary patterns for identifying MSVC CRT functions,
//! standard library functions, and common patterns in Windows binaries.
//! Data is loaded from the resolved patterns directory (`ResourceProvider`).

use fission_core::resources::ResourceProvider;
use serde::Deserialize;
use std::fs;

use super::signature::{FunctionSignature, PatternByte, SignatureParseError};

/// CRT pattern format: `{ "name": "...", "library": "...", "pattern": "hex...", "mask": "hex..." }`.
///
/// Mask byte semantics:
/// - `FF` → fixed: use the pattern byte
/// - `00` or `??` → wildcard: any byte matches
/// - Partial mask (e.g. `F0`, `0F`) → bitwise mask: `(byte & mask) == (value & mask)`
#[derive(Deserialize)]
struct JsonCrtSignature {
    name: String,
    library: String,
    pattern: String,
    mask: String,
}

/// Convert a CRT-format `pattern` + `mask` pair into a `Vec<PatternByte>` pattern.
///
/// Rules:
/// - Pattern and mask must have matching token counts.
/// - Pattern token `??` → wildcard
/// - Mask token `00` or `??` → wildcard  
/// - Partial or full hex mask → `PatternByte::new(pattern_byte, mask_byte)`
pub fn try_crt_pattern_to_vec(
    pattern: &str,
    mask: &str,
) -> Result<Vec<PatternByte>, SignatureParseError> {
    let pat_tokens: Vec<&str> = pattern.split_whitespace().collect();
    let mask_tokens: Vec<&str> = mask.split_whitespace().collect();

    if pat_tokens.len() != mask_tokens.len() {
        return Err(SignatureParseError::LengthMismatch {
            pattern_tokens: pat_tokens.len(),
            mask_tokens: mask_tokens.len(),
        });
    }

    let mut result = Vec::with_capacity(pat_tokens.len());
    for (i, (&pat, &m)) in pat_tokens.iter().zip(mask_tokens.iter()).enumerate() {
        if pat == "??" {
            result.push(PatternByte::wildcard());
            continue;
        }

        let pat_byte =
            u8::from_str_radix(pat.trim(), 16).map_err(|_| SignatureParseError::InvalidToken {
                token: pat.to_string(),
                position: i,
            })?;

        let m_trim = m.trim();
        let mask_byte = if m_trim == "??" || m_trim == "00" {
            0x00
        } else {
            u8::from_str_radix(m_trim, 16).map_err(|_| SignatureParseError::InvalidMaskToken {
                token: m.to_string(),
                position: i,
            })?
        };

        result.push(PatternByte::new(pat_byte, mask_byte));
    }

    Ok(result)
}

/// Convert a CRT-format `pattern` + `mask` pair into a `Vec<PatternByte>` pattern, panicking on invalid syntax.
#[allow(dead_code)]
pub fn crt_pattern_to_vec(pattern: &str, mask: &str) -> Vec<PatternByte> {
    try_crt_pattern_to_vec(pattern, mask)
        .unwrap_or_else(|e| panic!("invalid CRT pattern/mask pair: {e}"))
}

/// Load MSVC/CRT pattern signatures from the resolved patterns corpus.
pub fn load_msvc_signatures(signatures: &mut Vec<FunctionSignature>) {
    let pattern_file = ["msvc", "_x64", "_crt", ".json"].concat();
    let Some(path) = ResourceProvider::global()
        .paths()
        .get_pattern_file(&pattern_file)
    else {
        return;
    };
    let crt_json = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {e}", path.display()));
    let crt_items: Vec<JsonCrtSignature> = serde_json::from_str(&crt_json)
        .unwrap_or_else(|e| panic!("Failed to parse {}: {e}", path.display()));
    for item in crt_items {
        let pattern = match try_crt_pattern_to_vec(&item.pattern, &item.mask) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("Skipping invalid CRT signature {}: {e}", item.name);
                continue;
            }
        };
        let library = (!item.library.is_empty()).then_some(item.library);
        let sig = FunctionSignature {
            name: item.name,
            pattern,
            min_size: 8,
            params: Vec::new(),
            ret_type: String::new(),
            library,
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
                PatternByte::exact(0x4C),
                PatternByte::exact(0x8B),
                PatternByte::exact(0xDC),
                PatternByte::exact(0x48),
                PatternByte::exact(0x83),
                PatternByte::exact(0xEC)
            ]
        );
    }

    #[test]
    fn crt_pattern_with_mask_wildcard() {
        // mask "??" means wildcard
        let result = crt_pattern_to_vec("48 83 EC 28 48 85 C9", "FF FF FF ?? FF FF FF");
        assert!(result[3].is_wildcard(), "mask wildcard should be wildcard");
        assert_eq!(result[0], PatternByte::exact(0x48));
    }

    #[test]
    fn crt_pattern_with_pat_wildcard() {
        // pattern "??" means wildcard regardless of mask
        let result = crt_pattern_to_vec("48 85 C9 74 ?? 48 83 EC", "FF FF FF FF ?? FF FF FF");
        assert!(
            result[4].is_wildcard(),
            "pattern wildcard should be wildcard"
        );
    }

    #[test]
    fn crt_pattern_preserves_partial_bitmasks() {
        let result = crt_pattern_to_vec("48 89 C0", "FF F0 0F");
        assert_eq!(result[0], PatternByte::exact(0x48));
        assert_eq!(result[1], PatternByte::new(0x89, 0xF0));
        assert_eq!(result[2], PatternByte::new(0xC0, 0x0F));

        // Test matching
        assert!(result[1].matches(0x89));
        assert!(result[1].matches(0x80));
        assert!(result[1].matches(0x8F));
        assert!(!result[1].matches(0x90));
    }

    #[test]
    fn crt_pattern_length_mismatch_fails() {
        let err = try_crt_pattern_to_vec("48 89 C0", "FF FF").unwrap_err();
        match err {
            SignatureParseError::LengthMismatch {
                pattern_tokens,
                mask_tokens,
            } => {
                assert_eq!(pattern_tokens, 3);
                assert_eq!(mask_tokens, 2);
            }
            _ => panic!("unexpected error: {err:?}"),
        }
    }

    #[test]
    fn load_msvc_signatures_loads_crt() {
        let mut sigs = Vec::new();
        load_msvc_signatures(&mut sigs);
        // CRT JSON has 10 entries; there should be at least that many.
        assert!(
            sigs.len() >= 10,
            "expected at least 10 CRT signatures, got {}",
            sigs.len()
        );
        let names: Vec<&str> = sigs.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"memcpy"), "memcpy should be present");
        assert!(names.contains(&"malloc"), "malloc should be present");
        assert!(names.contains(&"strlen"), "strlen should be present");
    }

    #[test]
    fn load_msvc_signatures_populates_library() {
        let mut sigs = Vec::new();
        load_msvc_signatures(&mut sigs);
        let memcpy = sigs
            .iter()
            .find(|s| s.name == "memcpy")
            .expect("memcpy signature should be present");
        assert_eq!(
            memcpy.library.as_deref(),
            Some("ucrtbase"),
            "library from the pattern JSON should carry through onto FunctionSignature"
        );
    }

    #[test]
    fn memcpy_matches_known_prologue() {
        let mut sigs = Vec::new();
        load_msvc_signatures(&mut sigs);
        let memcpy_sig = sigs
            .iter()
            .find(|s| s.name == "memcpy")
            .expect("memcpy missing");
        // UCRT memcpy x64 prologue: 4C 8B DC 48 83 EC ...
        let bytes: &[u8] = &[0x4C, 0x8B, 0xDC, 0x48, 0x83, 0xEC, 0x48, 0x00];
        assert!(
            memcpy_sig.matches(bytes),
            "memcpy pattern should match known bytes"
        );
    }

    #[test]
    fn database_contains_crt_entries() {
        use crate::SignatureDatabase;
        let db = SignatureDatabase::new();
        assert!(
            db.signatures().len() >= 10,
            "SignatureDatabase should have CRT patterns"
        );
    }
}
