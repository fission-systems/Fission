//! CRT Function Signature Database
//!
//! FLIRT-style pattern matching for recognizing CRT and standard library functions.
//! This helps the decompiler identify known functions without debug symbols.

pub mod win_api;

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
        // ==================== x86 Patterns ====================
        
        // __security_check_cookie (x86)
        self.signatures.push(FunctionSignature::from_hex(
            "__security_check_cookie",
            "3B 0D ?? ?? ?? ?? 74 ?? C3"
        ));
        
        // __security_init_cookie (x86)
        self.signatures.push(FunctionSignature::from_hex(
            "__security_init_cookie",
            "8B FF 55 8B EC 83 EC 10 A1"
        ));
        
        // _initterm (x86)
        self.signatures.push(FunctionSignature::from_hex(
            "_initterm",
            "56 8B 74 24 08 57 8B 7C 24 10"
        ));
        
        // _CRT_INIT (x86)
        self.signatures.push(FunctionSignature::from_hex(
            "_CRT_INIT",
            "53 56 57 BB 01 00 00 00"
        ));
        
        // ==================== x64 Patterns ====================
        
        // __security_check_cookie (x64) - GS cookie check
        self.signatures.push(FunctionSignature::from_hex(
            "__security_check_cookie",
            "48 3B 0D ?? ?? ?? ?? 75 ?? C3"
        ));
        
        // __security_init_cookie (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "__security_init_cookie",
            "48 83 EC 28 48 8B 05"
        ));
        
        // _initterm (x64) - initializer list
        self.signatures.push(FunctionSignature::from_hex(
            "_initterm",
            "48 89 5C 24 08 57 48 83 EC 20 48 8B D9 48 8B FA"
        ));
        
        // _initterm_e (x64) - initializer with error
        self.signatures.push(FunctionSignature::from_hex(
            "_initterm_e",
            "48 89 5C 24 08 48 89 6C 24 10 48 89 74 24 18"
        ));
        
        // __GSHandlerCheck (x64) - exception handler GS check
        self.signatures.push(FunctionSignature::from_hex(
            "__GSHandlerCheck",
            "48 89 4C 24 08 48 89 54 24 10 4C 89 44 24 18"
        ));
        
        // __chkstk (x64) - stack probe
        self.signatures.push(FunctionSignature::from_hex(
            "__chkstk",
            "48 83 EC 10 4C 89 14 24 4C 89 5C 24 08"
        ));
        
        // __alloca_probe (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "__alloca_probe",
            "51 48 8D 4C 24 08 48 2B C8"
        ));
        
        // memset (x64) - common pattern
        self.signatures.push(FunctionSignature::from_hex(
            "memset",
            "40 53 48 83 EC 20 0F B6 C2 48 8B D9"
        ));
        
        // memcpy (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "memcpy",
            "48 8B C1 4C 8D 15 ?? ?? ?? ?? 49 83 F8 0F"
        ));
        
        // memmove (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "memmove",
            "48 8B C1 4C 8B D9 48 3B CA"
        ));
        
        // strlen (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "strlen",
            "48 8B C1 48 F7 D0 48 83 C0 01"
        ));
        
        // strcmp (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "strcmp",
            "48 89 5C 24 08 48 89 74 24 10 57 48 83 EC 20 48 8B F2"
        ));
        
        // wcslen (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "wcslen",
            "48 8B C1 66 83 39 00 74"
        ));
        
        // wcscpy (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "wcscpy",
            "48 8B C1 66 44 89 01 66 45 85 C0"
        ));
        
        // _purecall (x64) - pure virtual call error
        self.signatures.push(FunctionSignature::from_hex(
            "_purecall",
            "48 83 EC 28 E8 ?? ?? ?? ?? 33 C0"
        ));
        
        // _amsg_exit (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "_amsg_exit",
            "48 83 EC 28 8B C1 B9 ?? 00 00 00"
        ));
        
        // _cexit (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "_cexit",
            "48 83 EC 28 E8 ?? ?? ?? ?? 85 C0 75"
        ));
        
        // _c_exit (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "_c_exit",
            "48 83 EC 28 E8 ?? ?? ?? ?? E8"
        ));
        
        // ~~ PyInstaller specific (observed in user binary) ~~
        
        // Python main entry stub
        self.signatures.push(FunctionSignature::from_hex(
            "_pyi_main",
            "48 89 5C 24 ?? 48 89 74 24 ?? 57 48 83 EC 20"
        ));
        
        // Common function prologue patterns (x64)
        self.signatures.push(FunctionSignature::from_hex(
            "_crt_startup",
            "48 83 EC 28 48 8D 0D ?? ?? ?? ?? E8"
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
