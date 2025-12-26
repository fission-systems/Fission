//! CRT Function Signature Database
//!
//! FLIRT-style pattern matching for recognizing CRT and standard library functions.
//! This helps the decompiler identify known functions without debug symbols.

use std::collections::HashMap;

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

/// CRT Signature Database
pub struct SignatureDatabase {
    signatures: Vec<FunctionSignature>,
}

impl SignatureDatabase {
    /// Create a new database with built-in signatures
    pub fn new() -> Self {
        let mut db = Self {
            signatures: Vec::new(),
        };
        db.load_msvc_signatures();
        db
    }
    
    /// Load MSVC CRT signatures
    fn load_msvc_signatures(&mut self) {
        // Common CRT function entry patterns
        // These are simplified examples - real FLIRT signatures are much more complex
        
        // __security_check_cookie (x86)
        self.signatures.push(FunctionSignature::from_hex(
            "__security_check_cookie",
            "3B 0D ?? ?? ?? ?? 74 ?? C3"
        ));
        
        // __security_cookie_init (x86)
        self.signatures.push(FunctionSignature::from_hex(
            "__security_init_cookie",
            "8B FF 55 8B EC 83 EC 10 A1"
        ));
        
        // _initterm (x86) - CRT initialization
        self.signatures.push(FunctionSignature::from_hex(
            "_initterm",
            "56 8B 74 24 08 57 8B 7C 24 10"
        ));
        
        // _CRT_INIT (x86)
        self.signatures.push(FunctionSignature::from_hex(
            "_CRT_INIT",
            "53 56 57 BB 01 00 00 00"
        ));
        
        // malloc wrapper (common pattern)
        self.signatures.push(FunctionSignature::from_hex(
            "malloc",
            "FF 25 ?? ?? ?? ??" // jmp [IAT]
        ));
        
        // memcpy common prologue
        self.signatures.push(FunctionSignature::from_hex(
            "memcpy",
            "8B 44 24 04 8B 4C 24 0C"
        ));
        
        // strlen common pattern
        self.signatures.push(FunctionSignature::from_hex(
            "strlen",
            "8B 4C 24 04 F7 C1 03 00 00 00"
        ));
        
        // strcmp pattern
        self.signatures.push(FunctionSignature::from_hex(
            "strcmp",
            "8B 4C 24 04 8B 54 24 08 0F B6 01"
        ));
        
        // printf/sprintf wrapper
        self.signatures.push(FunctionSignature::from_hex(
            "_printf",
            "6A ?? 68 ?? ?? ?? ?? E8"
        ));
        
        // x64 patterns
        self.signatures.push(FunctionSignature::from_hex(
            "__security_check_cookie_x64",
            "48 3B 0D ?? ?? ?? ?? 75 ?? C3"
        ));
    }
    
    /// Try to match a function's bytes against known signatures
    pub fn identify(&self, bytes: &[u8]) -> Option<&FunctionSignature> {
        for sig in &self.signatures {
            if sig.matches(bytes) {
                return Some(sig);
            }
        }
        None
    }
    
    /// Get all signatures
    pub fn signatures(&self) -> &[FunctionSignature] {
        &self.signatures
    }
    
    /// Add a custom signature
    pub fn add_signature(&mut self, sig: FunctionSignature) {
        self.signatures.push(sig);
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
