//! Signature Database
//!
//! Core database structure with indexing and matching logic

use super::msvc_sigs;
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
            // Find the first non-wildcard byte in the pattern
            if let Some(&Some(first_byte)) = sig.pattern.first() {
                self.first_byte_index
                    .entry(first_byte)
                    .or_insert_with(Vec::new)
                    .push(idx);
            } else if sig.pattern.first() == Some(&None) {
                // Signature starts with wildcard
                self.wildcard_signatures.push(idx);
            }
        }
    }

    /// Try to match a function's bytes against known signatures
    ///
    /// Performance: Uses first-byte index to reduce candidates from ~150 to typically 1-10,
    /// providing significant speedup for large binaries with many functions.
    pub fn identify(&self, bytes: &[u8]) -> Option<&FunctionSignature> {
        if bytes.is_empty() {
            return None;
        }

        let first_byte = bytes[0];

        // Use the index to only check signatures that start with the same first byte
        if let Some(indices) = self.first_byte_index.get(&first_byte) {
            for &idx in indices {
                if let Some(sig) = self.signatures.get(idx) {
                    if sig.matches(bytes) {
                        return Some(sig);
                    }
                }
            }
        }

        // Check signatures that start with wildcards (pre-indexed, no full scan needed)
        for &idx in &self.wildcard_signatures {
            if let Some(sig) = self.signatures.get(idx) {
                if sig.matches(bytes) {
                    return Some(sig);
                }
            }
        }

        None
    }

    /// Try to match a function's bytes with call graph relation validation
    ///
    /// This provides Ghidra FID-style matching by:
    /// 1. First matching byte patterns
    /// 2. Then validating call graph relations if signature has constraints
    /// 3. Rejecting matches that don't pass relation checks (if force_relation is set)
    pub fn identify_with_relation(
        &self,
        bytes: &[u8],
        func_addr: u64,
        call_graph: &super::relation::CallGraph,
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

    /// Find all byte-pattern matches (internal helper)
    fn find_all_matches(&self, bytes: &[u8]) -> Vec<&FunctionSignature> {
        let mut matches = Vec::new();

        if bytes.is_empty() {
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

        matches
    }

    /// Get all signatures
    pub fn signatures(&self) -> &[FunctionSignature] {
        &self.signatures
    }

    /// Add a custom signature
    pub fn add_signature(&mut self, sig: FunctionSignature) {
        let idx = self.signatures.len();
        // Update index if signature has a non-wildcard first byte
        if let Some(&Some(first_byte)) = sig.pattern.first() {
            self.first_byte_index
                .entry(first_byte)
                .or_insert_with(Vec::new)
                .push(idx);
        } else if sig.pattern.first() == Some(&None) {
            // Wildcard signature
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

    #[test]
    fn test_pattern_match() {
        let sig = FunctionSignature::from_hex("test", "55 8B EC ?? 6A");

        assert!(sig.matches(&[0x55, 0x8B, 0xEC, 0x00, 0x6A]));
        assert!(sig.matches(&[0x55, 0x8B, 0xEC, 0xFF, 0x6A])); // wildcard
        assert!(!sig.matches(&[0x55, 0x8B, 0xED, 0x00, 0x6A])); // wrong byte
        assert!(!sig.matches(&[0x55, 0x8B])); // too short
    }

    #[test]
    fn test_database_creation() {
        let db = SignatureDatabase::new();
        assert!(!db.signatures().is_empty());
    }
}
