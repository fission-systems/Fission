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
    /// Owning library/module of the matched function (e.g. `"ucrtbase"`),
    /// when the pattern source records it.
    pub library: Option<String>,

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
    /// Create a new signature from a hex pattern string.
    ///
    /// Use `??` for wildcards, e.g. `"55 8B EC ?? ?? 6A"`. Malformed
    /// tokens are programmer/configuration errors and fail loudly rather than
    /// silently becoming wildcards.
    pub fn from_hex(name: &str, hex_pattern: &str) -> Self {
        Self::try_from_hex(name, hex_pattern)
            .unwrap_or_else(|error| panic!("invalid signature pattern for {name}: {error}"))
    }

    /// Fallible form of [`Self::from_hex`] for data-driven callers that need
    /// to report a bad pattern instead of panicking.
    pub fn try_from_hex(name: &str, hex_pattern: &str) -> Result<Self, String> {
        let mut pattern = Vec::new();
        for (index, token) in hex_pattern.split_whitespace().enumerate() {
            if token == "??" {
                pattern.push(None);
                continue;
            }
            if token.len() != 2 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(format!("invalid token {token:?} at byte-pattern index {index}"));
            }
            let byte = u8::from_str_radix(token, 16)
                .map_err(|_| format!("invalid hex byte {token:?} at index {index}"))?;
            pattern.push(Some(byte));
        }

        Ok(Self {
            name: name.to_string(),
            pattern,
            min_size: 16,
            params: Vec::new(),
            ret_type: String::new(),
            library: None,
            expected_callees: Vec::new(),
            expected_callers: Vec::new(),
            force_relation: false,
            confidence: 100,
        })
    }

    /// Number of pattern bytes that actually constrain the input.
    pub fn specificity(&self) -> usize {
        self.pattern.iter().filter(|byte| byte.is_some()).count()
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
        if bytes.len() < self.pattern.len() || bytes.len() < self.min_size {
            return false;
        }

        for (i, &pat_byte) in self.pattern.iter().enumerate() {
            if let Some(expected) = pat_byte
                && bytes[i] != expected
            {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_hex_tokens_are_rejected_not_wildcarded() {
        for pattern in ["55 ? EC", "55 0x8B EC", "55 ZZ EC", "55 ??? EC"] {
            assert!(
                FunctionSignature::try_from_hex("bad", pattern).is_err(),
                "{pattern:?} should be rejected"
            );
        }
        assert!(FunctionSignature::try_from_hex("ok", "55 ?? EC").is_ok());
    }

    #[test]
    fn minimum_size_is_part_of_matching_contract() {
        let mut signature = FunctionSignature::from_hex("short-prefix", "55 8B EC");
        signature.min_size = 8;
        assert!(!signature.matches(&[0x55, 0x8B, 0xEC, 0, 0, 0, 0]));
        assert!(signature.matches(&[0x55, 0x8B, 0xEC, 0, 0, 0, 0, 0]));
    }
}
