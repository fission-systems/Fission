//! Function Signature
//!
//! Represents a pattern for matching known functions

/// Errors encountered when parsing function signatures or byte patterns.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SignatureParseError {
    #[error("invalid pattern token '{token}' at position {position}")]
    InvalidToken { token: String, position: usize },
    #[error(
        "token count mismatch: pattern has {pattern_tokens} tokens but mask has {mask_tokens} tokens"
    )]
    LengthMismatch {
        pattern_tokens: usize,
        mask_tokens: usize,
    },
    #[error("invalid mask token '{token}' at position {position}")]
    InvalidMaskToken { token: String, position: usize },
}

/// A single byte in a signature pattern, supporting exact values, wildcards, and partial bitmasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PatternByte {
    /// Expected byte value (masked: `value & mask == value`)
    pub value: u8,
    /// Bitmask for matching (0xFF = exact byte, 0x00 = wildcard, intermediate = partial mask)
    pub mask: u8,
}

impl PatternByte {
    /// Create an exact expected byte pattern.
    pub const fn exact(value: u8) -> Self {
        Self { value, mask: 0xFF }
    }

    /// Create a wildcard pattern byte (matches any byte).
    pub const fn wildcard() -> Self {
        Self {
            value: 0,
            mask: 0x00,
        }
    }

    /// Create a pattern byte with a partial or full bitmask.
    pub const fn new(value: u8, mask: u8) -> Self {
        Self {
            value: value & mask,
            mask,
        }
    }

    /// Returns true if the given byte matches this pattern byte.
    #[inline]
    pub fn matches(&self, b: u8) -> bool {
        (b & self.mask) == self.value
    }

    /// Returns true if this byte is a wildcard (mask == 0).
    #[inline]
    pub fn is_wildcard(&self) -> bool {
        self.mask == 0
    }

    /// Returns true if this byte is an exact match (mask == 0xFF).
    #[inline]
    pub fn is_exact(&self) -> bool {
        self.mask == 0xFF
    }

    /// Number of constrained bits in this pattern byte.
    #[inline]
    pub fn constrained_bits(&self) -> u32 {
        self.mask.count_ones()
    }
}

impl PartialEq<Option<u8>> for PatternByte {
    fn eq(&self, other: &Option<u8>) -> bool {
        match other {
            None => self.is_wildcard(),
            Some(b) => self.is_exact() && self.value == *b,
        }
    }
}

impl PartialEq<PatternByte> for Option<u8> {
    fn eq(&self, other: &PatternByte) -> bool {
        other == self
    }
}

/// A function signature pattern for matching
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    /// Short name of the function
    pub name: String,
    /// Byte pattern
    pub pattern: Vec<PatternByte>,
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
    /// Attempt to parse a signature from a hex pattern string.
    ///
    /// Valid tokens are two-digit hex bytes (e.g. `55`, `8b`, `EC`) or `??` for wildcards.
    /// Returns `Err(SignatureParseError)` if any token is malformed.
    pub fn try_from_hex(name: &str, hex_pattern: &str) -> Result<Self, SignatureParseError> {
        let mut pattern = Vec::new();
        for (pos, s) in hex_pattern.split_whitespace().enumerate() {
            if s == "??" {
                pattern.push(PatternByte::wildcard());
            } else if s.len() == 2 && !s.starts_with("0x") && !s.starts_with("0X") {
                let val =
                    u8::from_str_radix(s, 16).map_err(|_| SignatureParseError::InvalidToken {
                        token: s.to_string(),
                        position: pos,
                    })?;
                pattern.push(PatternByte::exact(val));
            } else {
                return Err(SignatureParseError::InvalidToken {
                    token: s.to_string(),
                    position: pos,
                });
            }
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

    /// Create a new signature from a hex pattern string.
    /// Use ?? for wildcards, e.g., "55 8B EC ?? ?? 6A".
    /// Panics if the hex pattern contains invalid tokens.
    pub fn from_hex(name: &str, hex_pattern: &str) -> Self {
        Self::try_from_hex(name, hex_pattern)
            .unwrap_or_else(|e| panic!("invalid hex pattern for signature '{name}': {e}"))
    }

    /// Set minimum function size
    pub fn with_min_size(mut self, min_size: usize) -> Self {
        self.min_size = min_size;
        self
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

    /// Calculate pattern specificity (total number of constrained bits)
    pub fn specificity(&self) -> usize {
        self.pattern
            .iter()
            .map(|b| b.constrained_bits() as usize)
            .sum()
    }

    /// Match pattern against bytes
    pub fn matches(&self, bytes: &[u8]) -> bool {
        if bytes.len() < self.pattern.len() || bytes.len() < self.min_size {
            return false;
        }

        for (i, pat_byte) in self.pattern.iter().enumerate() {
            if !pat_byte.matches(bytes[i]) {
                return false;
            }
        }
        true
    }
}
