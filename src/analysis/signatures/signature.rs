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
        }
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
