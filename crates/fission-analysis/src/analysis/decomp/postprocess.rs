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

        // Apply arithmetic idiom recovery
        processed = self.apply_arithmetic_idioms(&processed);

        processed
    }

    /// Try to recover a divisor from a magic number multiplication and shift
    /// Based on algorithms from RetDec and angr
    fn recover_divisor(&self, magic: u64, shift: u32, is_64bit_mul: bool) -> Option<u64> {
        let base_bits = if is_64bit_mul { 64 } else { 32 };

        // Dists approx 2^(base_bits + shift) / magic
        let pow_val = (base_bits as u64) + (shift as u64);
        if pow_val >= 128 {
            return None;
        }

        // We use 128-bit math to check
        let dividend = if pow_val < 64 {
            1u128 << pow_val
        } else {
            1u128 << pow_val
        };

        let divisor = (dividend / (magic as u128)) as u64;

        // Validate with a few test cases
        // (X * magic) >> base_bits >> shift == X / divisor
        let test_cases = [100u64, 1000, 10000];
        for &x in &test_cases {
            let expected = x / (divisor.max(1));
            let actual = (((x as u128 * magic as u128) >> base_bits) >> shift) as u64;
            if expected != actual {
                // Try divisor + 1 (for ceil cases)
                if x / (divisor + 1) == actual {
                    return Some(divisor + 1);
                }
                return None;
            }
        }

        Some(divisor)
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
            Lazy::new(|| Regex::new(r"(\w+)\._([\d]+)_([\d]+)_").unwrap());

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

        // Apply Swift accessor pattern recognition
        result = self.recognize_swift_accessors(&result);

        result
    }

    /// Recognize Swift accessor patterns and convert to field access
    /// Swift uses VTable calls for property access:
    /// getter: (**(ptr + 0x88))(buffer) -> ptr->get_fieldName()
    /// setter: (**(ptr + 0x90))(value, buffer) -> ptr->set_fieldName(value)
    fn recognize_swift_accessors(&self, code: &str) -> String {
        let mut result = code.to_string();

        // Pattern: (**(something*)(*base + 0xNN))(...) - Swift VTable accessor call
        // The VTable offset doesn't directly correspond to field offset,
        // but we can annotate it for clarity
        static SWIFT_VTABLE_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\(\*\*\([\w\s\*]+\)\(\*(\w+)\s*\+\s*(0x[0-9a-fA-F]+)\)\)").unwrap()
        });

        result = SWIFT_VTABLE_PATTERN
            .replace_all(&result, |caps: &regex::Captures| {
                let base = &caps[1];
                let vtable_offset_str = &caps[2];

                // Parse VTable offset
                let vtable_offset: u32 = if vtable_offset_str.starts_with("0x") {
                    u32::from_str_radix(&vtable_offset_str[2..], 16).unwrap_or(0)
                } else {
                    vtable_offset_str.parse().unwrap_or(0)
                };

                // Swift property access via VTable:
                // Typically getters are at lower offsets (0x78, 0x80, 0x88...)
                // The actual field accessed depends on the class layout
                // We try to infer the property based on known patterns
                let accessor_type = match vtable_offset & 0x0f {
                    0x8 => "get",
                    0x0 => "set",
                    _ => "access",
                };

                // Try to find a corresponding field from our type info
                // VTable slots typically start at offset 0x50 for the first property
                // Each property has ~2 slots (getter, setter), each 8 bytes on 64-bit
                let estimated_field_index = vtable_offset.saturating_sub(0x50) / 0x10;

                // Look for field at similar position
                let field_hint: String = self
                    .inferred_types
                    .iter()
                    .flat_map(|t| t.fields.iter())
                    .nth(estimated_field_index as usize)
                    .map(|f| f.name.clone())
                    .unwrap_or_else(|| format!("property_{}", estimated_field_index));

                format!(
                    "/* Swift {} {} via VTable@{} */(**(void**)(*{} + {}))",
                    accessor_type, field_hint, vtable_offset_str, base, vtable_offset_str
                )
            })
            .to_string();

        // Pattern for Swift accessor return values: axVar3._8_8_
        // These are often the actual value returned by the accessor
        static SWIFT_ACCESSOR_RESULT: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"(axVar\d+)\._(8)_(8)_").unwrap());

        result = SWIFT_ACCESSOR_RESULT
            .replace_all(&result, |caps: &regex::Captures| {
                let var = &caps[1];
                // This is the returned value from a Swift accessor
                format!("{}->value/* Swift property value */", var)
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

    /// Apply arithmetic idiom recovery
    /// Simplifies common compiler-generated bit-twiddling patterns
    fn apply_arithmetic_idioms(&self, code: &str) -> String {
        let mut result = code.to_string();

        // 1. Signed Modulo 2 (64-bit split implementation on 32-bit x86)
        // Pattern:
        // signVar = hiVar >> 0x1f;
        // modVar = (loVar ^ signVar) - signVar & 1 ^ signVar;
        // return CONCAT44(-(uint)(modVar < signVar), modVar - signVar);
        static SIGNED_MOD2_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?s)(?P<s1>\w+)\s*=\s*(?P<high>\w+)\s*>>\s*0x1[fF];\s*(?P<m1>\w+)\s*=\s*\((?P<low>[\w\->\.\*]+)\s*\^\s*(?P<s2>\w+)\)\s*-\s*(?P<s3>\w+)\s*&\s*1\s*\^\s*(?P<s4>\w+);\s*return\s*CONCAT44\s*\(-\s*\(uint\)\s*\((?P<m2>\w+)\s*<\s*(?P<s5>\w+)\),\s*(?P<m3>\w+)\s*-\s*(?P<s6>\w+)\);").unwrap()
        });

        result = SIGNED_MOD2_PATTERN
            .replace_all(&result, |caps: &regex::Captures| {
                let sign = &caps["s1"];
                let md = &caps["m1"];
                let low = &caps["low"];

                // Verify all back-references match
                if sign == &caps["s2"]
                    && sign == &caps["s3"]
                    && sign == &caps["s4"]
                    && sign == &caps["s5"]
                    && sign == &caps["s6"]
                    && md == &caps["m2"]
                    && md == &caps["m3"]
                {
                    format!("return (longlong){} % 2;", low)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        // 2. Absolute Value (64-bit split)
        // Similar pattern but without the '& 1'
        static ABS64_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?s)(?P<s1>\w+)\s*=\s*(?P<high>\w+)\s*>>\s*0x1[fF];\s*(?P<out>\w+)\s*=\s*\((?P<val>[\w\->\.\*]+)\s*\^\s*(?P<s2>\w+)\)\s*-\s*(?P<s3>\w+);").unwrap()
        });

        result = ABS64_PATTERN
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                let sign = &caps["s1"];
                let out = &caps["out"];

                if sign == &caps["s2"] && sign == &caps["s3"] {
                    format!("{} = abs({}); // sign: {}", out, val, sign)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        // 3. Generic Signed Modulo Power-of-Two (64-bit split)
        // Pattern: (val + (sign >> shift) & mask) - (sign >> shift)
        static MOD_POW2_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?s)\(\s*(?P<val>[\w\->\.\*]+)\s*\+\s*\((?P<s1>\w+)\s*>>\s*(?P<sh1>0x[0-9a-fA-F]+)\)\s*&\s*(?P<mask>0x[0-9a-fA-F]+|[\d]+)\s*\)\s*-\s*\(\s*(?P<s2>\w+)\s*>>\s*(?P<sh2>0x[0-9a-fA-F]+|[\d]+)\s*\)").unwrap()
        });

        result = MOD_POW2_PATTERN
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                let sign = &caps["s1"];
                let shift = &caps["sh1"];

                if sign == &caps["s2"] && shift == &caps["sh2"] {
                    let mask: u64 = if caps["mask"].starts_with("0x") {
                        u64::from_str_radix(&caps["mask"][2..], 16).unwrap_or(0)
                    } else {
                        caps["mask"].parse().unwrap_or(0)
                    };
                    format!("({} % {})", val, mask + 1)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        // 4. Sign Extraction Cleanup (Simplified)
        // (int)var >> 0x1f is just a sign mask
        static SIGN_MASK_PATTERN: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"\(int\)\s*(\w+)\s*>>\s*0x1[fF]").unwrap());

        result = SIGN_MASK_PATTERN
            .replace_all(&result, |caps: &regex::Captures| {
                format!("SIGN_EXTRACT({})", &caps[1])
            })
            .to_string();

        // 5. Magic Division (32-bit and 64-bit)
        // Pattern: (uint)((ulonglong)var * 0xMAGIC >> 0x20) >> 0xSHIFT
        // This is very common in 32-bit code for division by non-powers-of-two
        static MAGIC_DIV_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?s)\(uint\)\s*\(\s*\(ulonglong\)\s*(?P<val>[\w\->\.\*]+)\s*\*\s*(?P<magic>0x[0-9a-fA-F]+)\s*>>\s*0x20\s*\)\s*(?:>>\s*(?P<shift>0x[0-9a-fA-F]+|[\d]+))?").unwrap()
        });

        result = MAGIC_DIV_PATTERN
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                let magic_str = &caps["magic"];
                let magic: u64 = u64::from_str_radix(&magic_str[2..], 16).unwrap_or(0);
                let shift: u32 = caps
                    .name("shift")
                    .map(|s| {
                        if s.as_str().starts_with("0x") {
                            u32::from_str_radix(&s.as_str()[2..], 16).unwrap_or(0)
                        } else {
                            s.as_str().parse().unwrap_or(0)
                        }
                    })
                    .unwrap_or(0);

                if let Some(divisor) = self.recover_divisor(magic, shift, false) {
                    format!("({} / {})", val, divisor)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        // 6. Common 32-bit to 64-bit extension patterns in arithmetic
        // Pattern: CONCAT44(var >> 0x1f, var) -> (longlong)var
        static CONCAT_SEXT_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"CONCAT44\s*\(\s*(?P<hi>[\w\->\.\*]+)\s*>>\s*0x1[fF]\s*,\s*(?P<lo>[\w\->\.\*]+)\s*\)").unwrap()
        });

        result = CONCAT_SEXT_PATTERN
            .replace_all(&result, |caps: &regex::Captures| {
                let hi = &caps["hi"];
                let lo = &caps["lo"];
                if hi == lo {
                    format!("(longlong){}", lo)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        // 7. Modulo from optimized division subtraction
        // Pattern: (var - ((var / D) * (D-1) + var / D)) -> (var % D)
        // Simplified pattern: (var - (uint)(... + var / D))
        static MOD_DIV_SUB_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?s)\(\s*(?P<val>[\w\->\.\*]+)\s*-\s*\(\s*(?:[\w\s\(\)\*>>&\^|~]+)\s*\+\s*(?P<v2>[\w\->\.\*]+)\s*/\s*(?P<divisor>\d+)\s*\)\s*\)").unwrap()
        });

        result = MOD_DIV_SUB_PATTERN
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                let v2 = &caps["v2"];
                if val == v2 {
                    let divisor = &caps["divisor"];
                    format!("({} % {})", val, divisor)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        result
    }
}

impl Default for PostProcessor {
    fn default() -> Self {
        Self::new()
    }
}
