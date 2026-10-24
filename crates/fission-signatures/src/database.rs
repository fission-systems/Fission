//! Signature Database
//!
//! Core database structure with indexing and matching logic

use super::msvc_sigs;
use super::relation::CallGraphView;
use super::signature::FunctionSignature;
use std::collections::HashMap;

/// Result of signature identification with relation validation
#[derive(Debug, Clone)]
pub struct IdentifyResult {
    /// The matched signature
    pub signature: FunctionSignature,
    /// Confidence score after relation validation (0-100)
    pub confidence: u8,
    /// Expected callees that were found in the call graph
    pub matched_callees: Vec<String>,
    /// Expected callers that were found in the call graph
    pub matched_callers: Vec<String>,
}

/// CRT Signature Database
///
/// Uses a first-byte index for faster signature matching. Most signatures
/// start with a unique or semi-unique first byte, so indexing by this byte
/// reduces the number of signatures to check from ~150 to typically 1-10.
pub struct SignatureDatabase {
    signatures: Vec<FunctionSignature>,
    /// Index of signatures by their first non-wildcard byte for O(1) initial filtering.
    /// Key: first byte value, Value: indices into signatures vec
    first_byte_index: HashMap<u8, Vec<usize>>,
    /// Indices of signatures that start with wildcards (must be checked for all inputs)
    wildcard_signatures: Vec<usize>,
}

impl SignatureDatabase {
    /// Create a new database with built-in signatures
    ///
    /// Performance: Pre-allocates vector capacity based on known signature count
    /// to avoid reallocations during loading (~150 signatures)
    pub fn new() -> Self {
        let mut db = Self {
            // Pre-allocate for ~150 known signatures to avoid reallocations
            signatures: Vec::with_capacity(160),
            first_byte_index: HashMap::with_capacity(64),
            wildcard_signatures: Vec::new(),
        };
        msvc_sigs::load_msvc_signatures(&mut db.signatures);
        db.build_index();
        db
    }

    /// Build the first-byte index for faster lookups
    fn build_index(&mut self) {
        self.first_byte_index.clear();
        self.wildcard_signatures.clear();
        for (idx, sig) in self.signatures.iter().enumerate() {
            if let Some(first_byte) = sig.pattern.first() {
                if first_byte.is_exact() {
                    self.first_byte_index
                        .entry(first_byte.value)
                        .or_insert_with(Vec::new)
                        .push(idx);
                } else {
                    self.wildcard_signatures.push(idx);
                }
            } else {
                // Empty pattern must also be indexed as a wildcard candidate
                self.wildcard_signatures.push(idx);
            }
        }
    }

    /// Try to match a function's bytes against known signatures.
    ///
    /// When multiple signatures match, returns the most specific match
    /// (highest constrained bit/byte count, then highest confidence).
    pub fn identify(&self, bytes: &[u8]) -> Option<&FunctionSignature> {
        self.find_all_matches(bytes).into_iter().next()
    }

    /// Try to match a function's bytes with call graph relation validation
    ///
    /// This provides Ghidra FID-style matching by:
    /// 1. First matching byte patterns
    /// 2. Then validating call graph relations if signature has constraints
    /// 3. Rejecting matches that don't pass relation checks (if force_relation is set)
    pub fn identify_with_relation<G: CallGraphView + ?Sized>(
        &self,
        bytes: &[u8],
        func_addr: u64,
        call_graph: &G,
    ) -> Option<IdentifyResult> {
        // First, find all byte-pattern matches
        let candidates = self.find_all_matches(bytes);

        for sig in candidates {
            let validation = super::relation::validate_relation(sig, func_addr, call_graph);

            if validation.passed {
                return Some(IdentifyResult {
                    signature: sig.clone(),
                    confidence: validation.confidence,
                    matched_callees: validation.matched_callees,
                    matched_callers: validation.matched_callers,
                });
            }
        }

        None
    }

    /// Find all byte-pattern matches (ranked by specificity and confidence descending)
    fn find_all_matches(&self, bytes: &[u8]) -> Vec<&FunctionSignature> {
        let mut matches = Vec::new();

        if bytes.is_empty() {
            for &idx in &self.wildcard_signatures {
                if let Some(sig) = self.signatures.get(idx) {
                    if sig.matches(bytes) {
                        matches.push(sig);
                    }
                }
            }
            matches.sort_by(|a, b| {
                b.specificity()
                    .cmp(&a.specificity())
                    .then_with(|| b.confidence.cmp(&a.confidence))
            });
            return matches;
        }

        let first_byte = bytes[0];

        if let Some(indices) = self.first_byte_index.get(&first_byte) {
            for &idx in indices {
                if let Some(sig) = self.signatures.get(idx) {
                    if sig.matches(bytes) {
                        matches.push(sig);
                    }
                }
            }
        }

        for &idx in &self.wildcard_signatures {
            if let Some(sig) = self.signatures.get(idx) {
                if sig.matches(bytes) {
                    matches.push(sig);
                }
            }
        }

        // Rank by specificity (constrained bits) descending, then by confidence descending
        matches.sort_by(|a, b| {
            b.specificity()
                .cmp(&a.specificity())
                .then_with(|| b.confidence.cmp(&a.confidence))
        });

        matches
    }

    /// Get all signatures
    pub fn signatures(&self) -> &[FunctionSignature] {
        &self.signatures
    }

    /// Add a custom signature
    pub fn add_signature(&mut self, sig: FunctionSignature) {
        let idx = self.signatures.len();
        if let Some(first_byte) = sig.pattern.first() {
            if first_byte.is_exact() {
                self.first_byte_index
                    .entry(first_byte.value)
                    .or_insert_with(Vec::new)
                    .push(idx);
            } else {
                self.wildcard_signatures.push(idx);
            }
        } else {
            self.wildcard_signatures.push(idx);
        }
        self.signatures.push(sig);
    }

    /// Scan binary bytes and identify known functions at given addresses
    /// Returns a map of address -> function name for matched signatures
    pub fn identify_functions_in_binary(
        &self,
        binary_data: &[u8],
        function_addresses: &[(u64, String)], // (address, current_name)
        image_base: u64,
    ) -> HashMap<u64, String> {
        let mut identified = HashMap::new();

        for (addr, _current_name) in function_addresses {
            // Calculate file offset from virtual address
            // For memory-mapped data, the address should be usable directly
            let offset = if *addr >= image_base {
                (*addr - image_base) as usize
            } else {
                continue;
            };

            // Skip if offset is out of bounds
            if offset >= binary_data.len() {
                continue;
            }

            // Get function bytes (first 32 bytes should be enough for matching)
            let end_offset = (offset + 32).min(binary_data.len());
            let func_bytes = &binary_data[offset..end_offset];

            // Try to identify
            if let Some(sig) = self.identify(func_bytes) {
                identified.insert(*addr, sig.name.clone());
            }
        }

        identified
    }
}

impl Default for SignatureDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signature::SignatureParseError;

    #[test]
    fn test_pattern_match() {
        let sig = FunctionSignature::from_hex("test", "55 8B EC ?? 6A").with_min_size(5);

        assert!(sig.matches(&[0x55, 0x8B, 0xEC, 0x00, 0x6A]));
        assert!(sig.matches(&[0x55, 0x8B, 0xEC, 0xFF, 0x6A])); // wildcard
        assert!(!sig.matches(&[0x55, 0x8B, 0xED, 0x00, 0x6A])); // wrong byte
        assert!(!sig.matches(&[0x55, 0x8B])); // too short (< 5)
    }

    #[test]
    fn test_matches_enforces_min_size() {
        let sig = FunctionSignature::from_hex("prologue", "55 8B EC").with_min_size(16);
        // 8 bytes matches pattern prefix, but fails min_size (8 < 16)
        let short_thunk = [0x55, 0x8B, 0xEC, 0x90, 0x90, 0x90, 0x90, 0x90];
        assert_eq!(short_thunk.len(), 8);
        assert!(!sig.matches(&short_thunk));

        // 16 bytes matches min_size and pattern
        let mut full_func = [0x90u8; 16];
        full_func[..3].copy_from_slice(&[0x55, 0x8B, 0xEC]);
        assert!(sig.matches(&full_func));
    }

    #[test]
    fn test_identify_ranks_by_specificity_and_confidence() {
        let mut db = SignatureDatabase {
            signatures: Vec::new(),
            first_byte_index: HashMap::new(),
            wildcard_signatures: Vec::new(),
        };

        // Short loose signature
        let sig_generic = FunctionSignature::from_hex("loose_func", "55 8B EC")
            .with_min_size(3)
            .with_confidence(100);

        // Specific signature with more constrained bytes
        let sig_specific = FunctionSignature::from_hex("specific_func", "55 8B EC 83 EC 20")
            .with_min_size(6)
            .with_confidence(90);

        db.add_signature(sig_generic);
        db.add_signature(sig_specific);

        let input = [0x55, 0x8B, 0xEC, 0x83, 0xEC, 0x20, 0x48, 0x89];
        let matched = db.identify(&input).expect("expected match");
        assert_eq!(matched.name, "specific_func");

        // When specificities are equal, confidence breaks tie
        let mut db2 = SignatureDatabase {
            signatures: Vec::new(),
            first_byte_index: HashMap::new(),
            wildcard_signatures: Vec::new(),
        };
        let sig_low_conf = FunctionSignature::from_hex("low_conf", "55 8B EC")
            .with_min_size(3)
            .with_confidence(80);
        let sig_high_conf = FunctionSignature::from_hex("high_conf", "55 8B EC")
            .with_min_size(3)
            .with_confidence(95);

        db2.add_signature(sig_low_conf);
        db2.add_signature(sig_high_conf);

        let matched2 = db2.identify(&input).expect("expected match");
        assert_eq!(matched2.name, "high_conf");
    }

    #[test]
    fn test_empty_pattern_indexed_and_matched() {
        let mut db = SignatureDatabase {
            signatures: Vec::new(),
            first_byte_index: HashMap::new(),
            wildcard_signatures: Vec::new(),
        };

        let empty_sig = FunctionSignature {
            name: "empty_pattern".to_string(),
            pattern: Vec::new(),
            min_size: 4,
            params: Vec::new(),
            ret_type: String::new(),
            library: None,
            expected_callees: Vec::new(),
            expected_callers: Vec::new(),
            force_relation: false,
            confidence: 50,
        };

        db.add_signature(empty_sig);
        assert_eq!(db.wildcard_signatures.len(), 1);

        assert!(!db.identify(&[0x90, 0x90]).is_some()); // too short (< 4)
        let matched = db
            .identify(&[0x90, 0x90, 0x90, 0x90])
            .expect("should match empty pattern");
        assert_eq!(matched.name, "empty_pattern");
    }

    #[test]
    fn test_try_from_hex_rejects_malformed_tokens() {
        assert!(matches!(
            FunctionSignature::try_from_hex("bad", "55 8B ? 00"),
            Err(SignatureParseError::InvalidToken { token, position: 2 }) if token == "?"
        ));

        assert!(matches!(
            FunctionSignature::try_from_hex("bad", "0x55 8B EC"),
            Err(SignatureParseError::InvalidToken { token, position: 0 }) if token == "0x55"
        ));

        assert!(matches!(
            FunctionSignature::try_from_hex("bad", "55 ZZ EC"),
            Err(SignatureParseError::InvalidToken { token, position: 1 }) if token == "ZZ"
        ));

        assert!(matches!(
            FunctionSignature::try_from_hex("bad", "55 ??? EC"),
            Err(SignatureParseError::InvalidToken { token, position: 1 }) if token == "???"
        ));

        assert!(FunctionSignature::try_from_hex("good", "55 8b EC ?? 00").is_ok());
    }

    #[test]
    fn test_database_creation() {
        let db = SignatureDatabase::new();
        assert!(!db.signatures().is_empty());
    }
}
