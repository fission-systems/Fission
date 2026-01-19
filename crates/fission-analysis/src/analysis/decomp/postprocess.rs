//! Decompiler Post-Processor
//!
//! Provides IDA-style code cleaning and boilerplate removal.
//!
//! This module processes raw C code from the decompiler to make it more
//! readable by hiding language-specific overhead like safety checks and panics.

use fission_loader::loader::types::InferredTypeInfo;
use once_cell::sync::Lazy;
use regex::Regex;

/// Pattern for Rust overflow checks
static RUST_OVERFLOW_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?s)if\s*\([^\{]*overflow[^\{]*\)\s*\{\s*panic_const_(add|sub|mul)_overflow\(\);?\s*\}",
    )
    .unwrap()
});

/// Pattern for Rust bounds checks
static RUST_BOUNDS_CHECK_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)if\s*\([^\{]*(?:index|len)[^\{]*\)\s*\{\s*panic_bounds_check\([^\{]*\);?\s*\}")
        .unwrap()
});

/// Pattern for Go panic checks
static GO_PANIC_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)if\s*\([^\{]*\)\s*\{\s*runtime\.gopanic\([^\{]*\);?\s*\}").unwrap()
});

/// Decompiler output post-processor
pub struct PostProcessor {
    clean_rust: bool,
    clean_go: bool,
    inferred_types: Vec<InferredTypeInfo>,
}

impl PostProcessor {
    pub fn new() -> Self {
        Self {
            clean_rust: true,
            clean_go: true,
            inferred_types: Vec::new(),
        }
    }

    /// Set inferred types for field name resolution
    pub fn with_inferred_types(mut self, types: Vec<InferredTypeInfo>) -> Self {
        self.inferred_types = types;
        self
    }

    /// Process the decompiler output to remove boilerplate
    pub fn process(&self, code: &str) -> String {
        let mut processed = code.to_string();

        if self.clean_rust {
            processed = self.remove_rust_boilerplate(&processed);
        }

        if self.clean_go {
            processed = self.remove_go_boilerplate(&processed);
        }

        // Always attempt to demangle Swift symbols
        processed = self.demangle_swift_symbols(&processed);

        // Apply field offset replacement if we have type info
        if !self.inferred_types.is_empty() {
            processed = self.replace_field_offsets(&processed);
        }

        processed
    }

    /// Replace pointer offset accesses with field names
    /// e.g., *(ptr + 0x18) -> this->counter (if offset 24 maps to 'counter')
    fn replace_field_offsets(&self, code: &str) -> String {
        let mut result = code.to_string();

        // Build offset -> field name mapping
        let mut offset_map: std::collections::HashMap<u32, String> =
            std::collections::HashMap::new();
        for ty in &self.inferred_types {
            for field in &ty.fields {
                offset_map.insert(field.offset, field.name.clone());
            }
        }

        if offset_map.is_empty() {
            return result;
        }

        // Pattern: *(something + 0xNN) or *(something + NN)
        // We look for hex offsets like 0x10, 0x18, 0x20, etc.
        static OFFSET_PATTERN: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"\*\s*\(\s*(\w+)\s*\+\s*(0x[0-9a-fA-F]+|\d+)\s*\)").unwrap());

        result = OFFSET_PATTERN
            .replace_all(&result, |caps: &regex::Captures| {
                let base = &caps[1];
                let offset_str = &caps[2];

                // Parse offset (handle both hex and decimal)
                let offset: u32 = if offset_str.starts_with("0x") || offset_str.starts_with("0X") {
                    u32::from_str_radix(&offset_str[2..], 16).unwrap_or(0)
                } else {
                    offset_str.parse().unwrap_or(0)
                };

                // Look up field name
                if let Some(field_name) = offset_map.get(&offset) {
                    format!("{}->{}/* @{} */", base, field_name, offset_str)
                } else {
                    // No match, keep original
                    caps[0].to_string()
                }
            })
            .to_string();

        // Also try to match array-style access: something[offset]
        // Pattern: baseVar._N_N_ (Ghidra's internal offset notation like local_38._8_8_)
        static GHIDRA_OFFSET_PATTERN: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"(\w+)\._(\d+)_(\d+)_").unwrap());

        result = GHIDRA_OFFSET_PATTERN
            .replace_all(&result, |caps: &regex::Captures| {
                let base = &caps[1];
                let offset: u32 = caps[2].parse().unwrap_or(0);
                let _size: u32 = caps[3].parse().unwrap_or(0);

                if let Some(field_name) = offset_map.get(&offset) {
                    format!("{}.{}/* @{} */", base, field_name, offset)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        result
    }

    fn demangle_swift_symbols(&self, code: &str) -> String {
        // Simple regex to find potential Swift symbols
        // Matches _$s... up to non-identifier char
        static SWIFT_REGEX: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(_\$s[a-zA-Z0-9_\$]+|__T[a-zA-Z0-9_\$]+|_T[a-zA-Z0-9_\$]+)").unwrap()
        });

        SWIFT_REGEX
            .replace_all(code, |caps: &regex::Captures| {
                let symbol = &caps[0];
                fission_loader::loader::demangle::demangle(symbol)
            })
            .to_string()
    }

    fn remove_rust_boilerplate(&self, code: &str) -> String {
        let mut result = code.to_string();

        // Replace overflow checks with comments
        result = RUST_OVERFLOW_PATTERN
            .replace_all(&result, "/* [Safety Check: Overflow] */")
            .to_string();

        // Replace bounds checks
        result = RUST_BOUNDS_CHECK_PATTERN
            .replace_all(&result, "/* [Safety Check: Bounds] */")
            .to_string();

        result
    }

    fn remove_go_boilerplate(&self, code: &str) -> String {
        let mut result = code.to_string();

        // Replace Go gopanic checks
        result = GO_PANIC_PATTERN
            .replace_all(&result, "/* [Go Panic Check] */")
            .to_string();

        result
    }
}

impl Default for PostProcessor {
    fn default() -> Self {
        Self::new()
    }
}
