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
    /// Indices of signatures that start with wildcards (or have no byte
    /// pattern and therefore cannot use the first-byte index).
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
            match sig.pattern.first() {
                Some(Some(first_byte)) => {
                    self.first_byte_index
                        .entry(*first_byte)
                        .or_default()
                        .push(idx);
                }
                Some(None) | None => self.wildcard_signatures.push(idx),
            }
        }
    }

    /// Try to match a function's bytes against known signatures.
    ///
    /// Candidates are ranked by constrained-byte specificity and then by the
    /// signature's base confidence, so the answer does not depend on JSON or
    /// insertion order when patterns overlap.
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
        // First, find all byte-pattern matches, already most-specific first.
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

    /// Find all byte-pattern matches, ranked most specific first.
    fn find_all_matches(&self, bytes: &[u8]) -> Vec<&FunctionSignature> {
        let mut matches = Vec::new();

        if bytes.is_empty() {
            return matches;
        }

        let first_byte = bytes[0];

        if let Some(indices) = self.first_byte_index.get(&first_byte) {
            for &idx in indices {
                if let Some(sig) = self.signatures.get(idx)
                    && sig.matches(bytes)
                {
                    matches.push(sig);
                }
            }
        }

        for &idx in &self.wildcard_signatures {
            if let Some(sig) = self.signatures.get(idx)
                && sig.matches(bytes)
            {
                matches.push(sig);
            }
        }

        matches.sort_by(|a, b| {
            b.specificity()
                .cmp(&a.specificity())
                .then_with(|| b.confidence.cmp(&a.confidence))
                .then_with(|| b.pattern.len().cmp(&a.pattern.len()))
                .then_with(|| a.name.cmp(&b.name))
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
        match sig.pattern.first() {
            Some(Some(first_byte)) => {
                self.first_byte_index
                    .entry(*first_byte)
                    .or_default()
                    .push(idx);
            }
            Some(None) | None => self.wildcard_signatures.push(idx),
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

    fn empty_database() -> SignatureDatabase {
        SignatureDatabase {
            signatures: Vec::new(),
            first_byte_index: HashMap::new(),
            wildcard_signatures: Vec::new(),
        }
    }

    #[test]
    fn test_pattern_match() {
        let mut sig = FunctionSignature::from_hex("test", "55 8B EC ?? 6A");
        sig.min_size = 5;

        assert!(sig.matches(&[0x55, 0x8B, 0xEC, 0x00, 0x6A]));
        assert!(sig.matches(&[0x55, 0x8B, 0xEC, 0xFF, 0x6A])); // wildcard
        assert!(!sig.matches(&[0x55, 0x8B, 0xED, 0x00, 0x6A])); // wrong byte
        assert!(!sig.matches(&[0x55, 0x8B])); // too short
    }

    #[test]
    fn identify_prefers_more_specific_pattern_then_confidence() {
        let mut db = empty_database();

        let mut broad = FunctionSignature::from_hex("broad", "AA ??");
        broad.min_size = 2;
        broad.confidence = 100;
        db.add_signature(broad);

        let mut specific_low_confidence = FunctionSignature::from_hex("specific", "AA BB");
        specific_low_confidence.min_size = 2;
        specific_low_confidence.confidence = 80;
        db.add_signature(specific_low_confidence);

        assert_eq!(
            db.identify(&[0xAA, 0xBB]).map(|sig| sig.name.as_str()),
            Some("specific")
        );

        let mut same_specificity_higher_confidence =
            FunctionSignature::from_hex("specific-hi", "AA BB");
        same_specificity_higher_confidence.min_size = 2;
        same_specificity_higher_confidence.confidence = 95;
        db.add_signature(same_specificity_higher_confidence);

        assert_eq!(
            db.identify(&[0xAA, 0xBB]).map(|sig| sig.name.as_str()),
            Some("specific-hi")
        );
    }

    #[test]
    fn empty_patterns_remain_matchable_when_min_size_is_satisfied() {
        let mut db = empty_database();
        let mut empty = FunctionSignature::from_hex("empty", "");
        empty.min_size = 1;
        db.add_signature(empty);

        assert_eq!(
            db.identify(&[0x90]).map(|sig| sig.name.as_str()),
            Some("empty")
        );
    }

    #[test]
    fn test_database_creation() {
        let db = SignatureDatabase::new();
        assert!(!db.signatures().is_empty());
    }
}
