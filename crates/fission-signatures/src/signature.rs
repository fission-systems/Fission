//! Function Signature
//!
//! Represents a pattern for matching known functions

/// A function signature pattern for matching
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    /// Short name of the function
    pub name: String,
    /// Byte pattern (None = wildcard)
    pub pattern: Vec<Option<u8>>,
    /// Minimum function size
    pub min_size: usize,
    /// Parameter names (for annotation)
    pub params: Vec<String>,
    /// Return type description
    pub ret_type: String,

    // === Call Graph Relation Matching (Ghidra FID parity) ===
    /// Names of functions this function is expected to call (children)
    /// If non-empty, at least one of these must be present in the call graph
    pub expected_callees: Vec<String>,
    /// Names of functions that are expected to call this function (parents)
    pub expected_callers: Vec<String>,
    /// If true, force relation check - reject match if no expected callees found
    pub force_relation: bool,
    /// Base confidence score (0-100). Reduced if relations don't match.
    pub confidence: u8,
}

impl FunctionSignature {
    /// Create a new signature from a hex pattern string
    /// Use ?? for wildcards, e.g., "55 8B EC ?? ?? 6A"
    pub fn from_hex(name: &str, hex_pattern: &str) -> Self {
        let pattern: Vec<Option<u8>> = hex_pattern
            .split_whitespace()
            .map(|s| {
                if s == "??" {
                    None
                } else {
                    u8::from_str_radix(s, 16).ok()
                }
            })
            .collect();

        Self {
            name: name.to_string(),
            pattern,
            min_size: 16,
            params: Vec::new(),
            ret_type: String::new(),
            expected_callees: Vec::new(),
            expected_callers: Vec::new(),
            force_relation: false,
            confidence: 100,
        }
    }

    /// Create a signature with expected callees for relation validation
    pub fn with_callees(mut self, callees: &[&str]) -> Self {
        self.expected_callees = callees.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Create a signature with expected callers for relation validation
    pub fn with_callers(mut self, callers: &[&str]) -> Self {
        self.expected_callers = callers.iter().map(|s| s.to_string()).collect();
        self
    }

    /// Set force_relation flag - require at least one expected callee to be found
    pub fn force_relation(mut self) -> Self {
        self.force_relation = true;
        self
    }

    /// Set confidence score
    pub fn with_confidence(mut self, score: u8) -> Self {
        self.confidence = score;
        self
    }

    /// Match pattern against bytes
    pub fn matches(&self, bytes: &[u8]) -> bool {
        if bytes.len() < self.pattern.len() {
            return false;
        }

        for (i, &pat_byte) in self.pattern.iter().enumerate() {
            if let Some(expected) = pat_byte {
                if bytes[i] != expected {
                    return false;
                }
            }
        }
        true
    }
}
