//! Decompiler Post-Processor
//!
//! Provides IDA-style code cleaning and boilerplate removal.
//!
//! This module processes raw C code from the decompiler to make it more
//! readable by hiding language-specific overhead like safety checks and panics.

use fission_loader::loader::types::{DwarfFunctionInfo, DwarfLocation, InferredTypeInfo};
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

/// Pattern for sequential equality checks with return (flat or BST)
/// Matches: if (var == N) { return expr; }
static SEQ_EQ_RETURN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^(\s*)if\s*\(\s*(\w+)\s*==\s*(-?(?:0[xX][0-9a-fA-F]+|\d+))\s*\)\s*\{\s*(return\s+[^;]+;)\s*\}",
    )
    .unwrap()
});

/// Matches reverse form: if (N == var) { return expr; }
static SEQ_EQ_RETURN_REV: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^(\s*)if\s*\(\s*(-?(?:0[xX][0-9a-fA-F]+|\d+))\s*==\s*(\w+)\s*\)\s*\{\s*(return\s+[^;]+;)\s*\}",
    )
    .unwrap()
});

/// Matches: if (!var) { return expr; }  (equivalently var == 0)
static SEQ_NOT_RETURN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(\s*)if\s*\(\s*!(\w+)\s*\)\s*\{\s*(return\s+[^;]+;)\s*\}").unwrap()
});

/// Range guard: if (var < N) { or if (var > N) { — BST split node
static RANGE_GUARD_OPEN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*if\s*\(\s*(\w+)\s*[<>]=?\s*(?:0[xX][0-9a-fA-F]+|\d+)\s*\)\s*\{").unwrap()
});

/// Standalone return statement (potential default case)
static DEFAULT_RETURN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(return\s+[^;]+;)\s*$").unwrap()
});

/// Decompiler output post-processor
pub struct PostProcessor {
    clean_rust: bool,
    clean_go: bool,
    inferred_types: Vec<InferredTypeInfo>,
    dwarf_info: Option<DwarfFunctionInfo>,
}

impl PostProcessor {
    pub fn new() -> Self {
        Self {
            clean_rust: true,
            clean_go: true,
            inferred_types: Vec::new(),
            dwarf_info: None,
        }
    }

    /// Set inferred types for field name resolution
    pub fn with_inferred_types(mut self, types: Vec<InferredTypeInfo>) -> Self {
        self.inferred_types = types;
        self
    }

    /// Set DWARF function info for variable/parameter name substitution
    pub fn with_dwarf_info(mut self, info: Option<DwarfFunctionInfo>) -> Self {
        self.dwarf_info = info;
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

        // Insert missing casts for assignment type mismatches
        processed = Self::insert_missing_casts(&processed);

        // Apply arithmetic idiom recovery
        processed = self.apply_arithmetic_idioms(&processed);

        // =====================================================================
        // Phase A: RetDec-inspired post-processing passes
        // Order follows RetDec's optimizer_manager.cpp —
        //   expressions → structure → dead code → naming
        // =====================================================================

        // A-1: Deref → Array index: *(a + N) → a[N]
        processed = Self::deref_to_array_index(&processed);

        // A-2: Bit-op → Logical-op in conditions: (cmp1) & (cmp2) → cmp1 && cmp2
        processed = Self::bitop_to_logicop(&processed);

        // A-3: Constant condition / dead branch removal
        processed = Self::remove_constant_conditions(&processed);

        // A-4: Empty else removal + If-return early exit
        processed = Self::simplify_if_structure(&processed);

        // A-5: while(true) { if(c) break; S } → while(!c) { S }
        processed = Self::while_true_to_while_cond(&processed);

        // =====================================================================
        // Phase B: Advanced structural + naming passes
        // =====================================================================

        // B-1: while(true) → for loop (init + exit-cond + update detection)
        processed = Self::while_true_to_for_loop(&processed);

        // B-2: Dead local assignment removal (2 iterations for cascading)
        processed = Self::remove_dead_local_assigns(&processed);
        processed = Self::remove_dead_local_assigns(&processed);

        // B-3: Induction variable naming (i, j, k for loop counters)
        processed = Self::rename_induction_vars(&processed);

        // B-4: Semantic variable naming (main→argc/argv, return→result, API results)
        processed = Self::rename_semantic_vars(&processed);

        // B-5: Loop idiom recognition (strlen, popcount, memset)
        processed = Self::recognize_loop_idioms(&processed);

        // Reconstruct switch from BST / sequential equality-return patterns
        processed = Self::reconstruct_switch_from_bst(&processed);

        // Apply DWARF variable/parameter name substitution
        if self.dwarf_info.is_some() {
            processed = self.apply_dwarf_names(&processed);
        }

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

        // Pattern: *(type*)(base + 0xNN) — cast-included pointer dereference
        // e.g., *(int*)(param_1 + 0x10), *(char *)(local_38 + 0x20)
        static CAST_OFFSET_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\*\s*\(\s*[\w\s]+\*\s*\)\s*\(\s*(\w+)\s*\+\s*(0x[0-9a-fA-F]+|\d+)\s*\)")
                .unwrap()
        });

        result = CAST_OFFSET_PATTERN
            .replace_all(&result, |caps: &regex::Captures| {
                let base = &caps[1];
                let offset_str = &caps[2];

                let offset: u32 = if offset_str.starts_with("0x") || offset_str.starts_with("0X") {
                    u32::from_str_radix(&offset_str[2..], 16).unwrap_or(0)
                } else {
                    offset_str.parse().unwrap_or(0)
                };

                if let Some(field_name) = offset_map.get(&offset) {
                    format!("{}->{}/* @{} */", base, field_name, offset_str)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        // Pattern: base[0xNN] — array-style access with hex index
        // e.g., param_1[0xc], local_38[0x10]
        static ARRAY_OFFSET_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(\w+)\[\s*(0x[0-9a-fA-F]+)\s*\]").unwrap()
        });

        result = ARRAY_OFFSET_PATTERN
            .replace_all(&result, |caps: &regex::Captures| {
                let base = &caps[1];
                let offset_str = &caps[2];

                let offset: u32 =
                    u32::from_str_radix(&offset_str[2..], 16).unwrap_or(0);

                if let Some(field_name) = offset_map.get(&offset) {
                    format!("{}->{}/* @{} */", base, field_name, offset_str)
                } else {
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

    /// Insert missing casts for common patterns where the decompiler omits
    /// explicit type conversions that would improve readability.
    ///
    /// Patterns handled:
    /// 1. `malloc(N)` → `(type *)malloc(N)` when assigned to a typed pointer
    /// 2. `*(base + offset)` → `*(type *)(base + offset)` when type is inferrable
    /// 3. Void pointer arithmetic without explicit cast
    fn insert_missing_casts(code: &str) -> String {
        let mut result = code.to_string();

        // Pattern 1: malloc/calloc return without cast
        // `var = malloc(...)` → `var = (void *)malloc(...)`
        // Only when the LHS doesn't already have a cast on the RHS
        static MALLOC_NO_CAST: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(\w+)\s*=\s*((?:malloc|calloc|realloc)\s*\([^;]*\))\s*;").unwrap()
        });

        result = MALLOC_NO_CAST
            .replace_all(&result, |caps: &regex::Captures| {
                let lhs = &caps[1];
                let rhs = &caps[2];
                // Check if there's already a cast
                if rhs.contains("(void *)") || rhs.contains("(void*)") {
                    return caps[0].to_string();
                }
                format!("{} = (void *){};", lhs, rhs)
            })
            .to_string();

        // Pattern 2: Bare integer used as pointer in dereference
        // `*(ulong + 0xN)` → `*(void *)(ulong + 0xN)`
        // This fires when a numeric/undefined type is used with pointer arithmetic
        static BARE_PTR_ARITH: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\*\s*\(\s*((?:ulong|ulonglong|undefined\d*|long|longlong)\s+\w+)\s*\+\s*(0x[0-9a-fA-F]+|\d+)\s*\)")
                .unwrap()
        });

        result = BARE_PTR_ARITH
            .replace_all(&result, |caps: &regex::Captures| {
                let base_expr = &caps[1];
                let offset = &caps[2];
                format!("*(void *)({} + {})", base_expr, offset)
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

        // 6b. Zero-extension CONCAT: CONCAT44(0, var) -> (ulonglong)var
        // This occurs when a 32-bit value is zero-extended to 64-bit.
        static CONCAT_ZEXT_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"CONCAT44\s*\(\s*0\s*,\s*(?P<lo>[^)]+?)\s*\)").unwrap()
        });

        result = CONCAT_ZEXT_PATTERN
            .replace_all(&result, |caps: &regex::Captures| {
                let lo = caps["lo"].trim();
                format!("(ulonglong){}", lo)
            })
            .to_string();

        // 6c. CONCAT with in_ phantom parameters: CONCAT44(in_REG, var) or CONCAT44(var, in_REG)
        // When one operand is a phantom "in_" register, the CONCAT is an artifact of
        // failed parameter binding. Replace with the meaningful operand only.
        static CONCAT_PHANTOM_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"CONCAT\d+\s*\(\s*(?:(?:\([^)]*\)\s*)?in_\w+\s*,\s*(?P<real1>[^,)]+))\s*\)").unwrap()
        });
        static CONCAT_PHANTOM_PATTERN2: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"CONCAT\d+\s*\(\s*(?P<real2>[^,)]+?)\s*,\s*(?:\([^)]*\)\s*)?in_\w+\s*\)").unwrap()
        });

        result = CONCAT_PHANTOM_PATTERN
            .replace_all(&result, |caps: &regex::Captures| {
                caps["real1"].trim().to_string()
            })
            .to_string();

        result = CONCAT_PHANTOM_PATTERN2
            .replace_all(&result, |caps: &regex::Captures| {
                caps["real2"].trim().to_string()
            })
            .to_string();

        // 6d. CONCAT with two sub-register pieces of the same value
        // CONCAT71(in_RAX, bVar) or CONCAT71(var_hi, var_lo) where the variable name
        // matches — these are register join artifacts. Keep the lower piece.
        static CONCAT_REGISTER_JOIN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"CONCAT\d+\s*\(\s*(?:\([^)]*\)\s*)?(?:in_[A-Z]\w*)\s*,\s*(?P<lo_val>[^)]+?)\s*\)").unwrap()
        });

        result = CONCAT_REGISTER_JOIN
            .replace_all(&result, |caps: &regex::Captures| {
                caps["lo_val"].trim().to_string()
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

        // =====================================================================
        // 8. Bit-shift → arithmetic (RetDec idioms_common.cpp)
        // =====================================================================

        // 8a. Unsigned right-shift → division: (uint)X >> C  →  X / 2^C
        // Only match when there's an explicit (uint) cast so we don't confuse
        // arithmetic >> with signed shifts that carry semantics.
        static LSHR_TO_DIV: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\(uint\)\s*(?P<val>\w+)\s*>>\s*(?P<sh>\d+)\b").unwrap()
        });

        result = LSHR_TO_DIV
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                let sh: u32 = caps["sh"].parse().unwrap_or(0);
                if sh > 0 && sh < 32 {
                    let divisor = 1u64 << sh;
                    format!("({} / {})", val, divisor)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        // 8b. Left-shift → multiplication: X << C  →  X * 2^C
        // Only for simple variable << constant (not inside complex expressions).
        static SHL_TO_MUL: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\b(?P<val>\w+)\s*<<\s*(?P<sh>[1-9]\d*)\b").unwrap()
        });

        result = SHL_TO_MUL
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                let sh: u32 = caps["sh"].parse().unwrap_or(0);
                // Don't transform if val looks like a keyword or type cast
                if sh > 0 && sh < 32
                    && val != "if" && val != "while" && val != "for"
                    && val != "return" && val != "int" && val != "uint"
                {
                    let multiplier = 1u64 << sh;
                    format!("{} * {}", val, multiplier)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        // 8c. AND mask → modulo: X & (2^N - 1)  →  X % 2^N
        // Match both hex and decimal masks.
        static AND_MASK_TO_MOD: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\b(?P<val>\w+)\s*&\s*(?P<mask>0x[0-9a-fA-F]+)\b").unwrap()
        });

        result = AND_MASK_TO_MOD
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                let mask_str = &caps["mask"];
                let mask = u64::from_str_radix(&mask_str[2..], 16).unwrap_or(0);
                // Check if mask+1 is a power of 2 (i.e. mask = 2^N - 1)
                let modulus = mask.wrapping_add(1);
                if mask > 0 && modulus > 0 && modulus.is_power_of_two()
                    && val != "if" && val != "while" && val != "return"
                {
                    format!("({} % {})", val, modulus)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        // =====================================================================
        // 9. Signed magic division with round-toward-zero fixup
        // Pattern: (int)((longlong)X * 0xMAGIC >> 0x20) >> SHIFT + (X >> 0x1f)
        // The "+ (X >> 0x1f)" compensates for negative values.
        // =====================================================================
        static SIGNED_MAGIC_DIV: Lazy<Regex> = Lazy::new(|| {
            Regex::new(concat!(
                r"\(int\)\s*\(\s*\(longlong\)\s*(?P<val>[\w\->\.\*]+)\s*\*\s*",
                r"(?P<magic>0x[0-9a-fA-F]+)\s*>>\s*0x20\s*\)\s*",
                r"(?:>>\s*(?P<shift>0x[0-9a-fA-F]+|\d+)\s*)?",
                r"\+\s*\(\s*(?P<v2>[\w\->\.\*]+)\s*>>\s*0x1[fF]\s*\)",
            )).unwrap()
        });

        result = SIGNED_MAGIC_DIV
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                let v2 = &caps["v2"];
                if val != v2 {
                    return caps[0].to_string();
                }
                let magic_str = &caps["magic"];
                let magic = u64::from_str_radix(&magic_str[2..], 16).unwrap_or(0);
                let shift: u32 = caps
                    .name("shift")
                    .map(|s| {
                        let s = s.as_str();
                        if s.starts_with("0x") {
                            u32::from_str_radix(&s[2..], 16).unwrap_or(0)
                        } else {
                            s.parse().unwrap_or(0)
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

        // 9b. Signed magic div variant: ((longlong)X * MAGIC >> 0x20) + X >> SHIFT + (X >> 0x1f)
        // Some compilers emit a correction add of X before the final shift.
        static SIGNED_MAGIC_DIV_FIXUP: Lazy<Regex> = Lazy::new(|| {
            Regex::new(concat!(
                r"\(\s*\(longlong\)\s*(?P<val>[\w\->\.\*]+)\s*\*\s*",
                r"(?P<magic>0x[0-9a-fA-F]+)\s*>>\s*0x20\s*",
                r"\+\s*(?P<v2>[\w\->\.\*]+)\s*\)\s*",
                r">>\s*(?P<shift>0x[0-9a-fA-F]+|\d+)\s*",
                r"\+\s*\(\s*(?P<v3>[\w\->\.\*]+)\s*>>\s*0x1[fF]\s*\)",
            )).unwrap()
        });

        result = SIGNED_MAGIC_DIV_FIXUP
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                let v2 = &caps["v2"];
                let v3 = &caps["v3"];
                if val != v2 || val != v3 {
                    return caps[0].to_string();
                }
                let magic = u64::from_str_radix(&caps["magic"][2..], 16).unwrap_or(0);
                let shift: u32 = {
                    let s = &caps["shift"];
                    if s.starts_with("0x") {
                        u32::from_str_radix(&s[2..], 16).unwrap_or(0)
                    } else {
                        s.parse().unwrap_or(0)
                    }
                };

                // For fixup variant, the effective magic is (magic + 2^32) because
                // compiler subtracts X then adds it back (negative magic).
                let effective_magic = magic.wrapping_add(1u64 << 32);
                if let Some(divisor) = self.recover_divisor(effective_magic, shift, false) {
                    format!("({} / {})", val, divisor)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        // 10. GCC float negation: X ^ 0x80000000 → -X  (XOR sign bit)
        static FLOAT_NEG_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\b(?P<val>\w+)\s*\^\s*(?:0x80000000|-2147483648)\b").unwrap()
        });

        result = FLOAT_NEG_PATTERN
            .replace_all(&result, |caps: &regex::Captures| {
                let val = &caps["val"];
                format!("-{}", val)
            })
            .to_string();

        result
    }

    // =========================================================================
    // A-1: Deref → Array Index (RetDec deref_to_array_index_optimizer.cpp)
    //   *(a + N)  →  a[N]     *(N + a)  →  a[N]
    // =========================================================================
    fn deref_to_array_index(code: &str) -> String {
        // *(var + N)  →  var[N]
        static DEREF_ADD: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\*\s*\(\s*(?P<base>[\w\->\.]+)\s*\+\s*(?P<idx>[\w\->\.0-9]+)\s*\)").unwrap()
        });
        // *(N + var)  →  var[N]  (commutative)
        static DEREF_ADD_REV: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\*\s*\(\s*(?P<idx>\d+|0x[0-9a-fA-F]+)\s*\+\s*(?P<base>[\w\->\.]+)\s*\)").unwrap()
        });

        let mut result = DEREF_ADD
            .replace_all(code, |caps: &regex::Captures| {
                let base = &caps["base"];
                let idx = &caps["idx"];
                // Don't transform if base looks like a cast expression
                if base.starts_with('(') {
                    return caps[0].to_string();
                }
                format!("{}[{}]", base, idx)
            })
            .to_string();

        result = DEREF_ADD_REV
            .replace_all(&result, |caps: &regex::Captures| {
                let base = &caps["base"];
                let idx = &caps["idx"];
                format!("{}[{}]", base, idx)
            })
            .to_string();

        result
    }

    // =========================================================================
    // A-2: Bit-op → Logical-op in conditions
    //   if ((a == b) & (c != d))   →  if ((a == b) && (c != d))
    //   if ((a < b) | (c > d))     →  if ((a < b) || (c > d))
    // Only when both sides are comparison expressions.
    // (RetDec bit_op_to_log_op_optimizer.cpp)
    // =========================================================================
    fn bitop_to_logicop(code: &str) -> String {
        // Match (comparison) & (comparison)  →  (comparison) && (comparison)
        static BIT_AND_TO_LOG_AND: Lazy<Regex> = Lazy::new(|| {
            Regex::new(concat!(
                r"\(\s*(?P<lhs>[^()]+?)\s*(?:==|!=|<=|>=|<|>)\s*[^()]+?\s*\)",
                r"\s*&\s*",
                r"\(\s*(?P<rhs>[^()]+?)\s*(?:==|!=|<=|>=|<|>)\s*[^()]+?\s*\)",
            )).unwrap()
        });
        // Match (comparison) | (comparison)  →  (comparison) || (comparison)
        static BIT_OR_TO_LOG_OR: Lazy<Regex> = Lazy::new(|| {
            Regex::new(concat!(
                r"\(\s*(?P<lhs>[^()]+?)\s*(?:==|!=|<=|>=|<|>)\s*[^()]+?\s*\)",
                r"\s*\|\s*",
                r"\(\s*(?P<rhs>[^()]+?)\s*(?:==|!=|<=|>=|<|>)\s*[^()]+?\s*\)",
            )).unwrap()
        });

        let result = BIT_AND_TO_LOG_AND
            .replace_all(code, |caps: &regex::Captures| {
                // Reconstruct with && — keep parens around each comparison
                let full = &caps[0];
                // Find the single '&' that we matched and replace with '&&'
                // The pattern guarantees there's exactly one standalone '&' between the ) and (
                full.replacen(") &", ") &&", 1)
            })
            .to_string();

        let result = BIT_OR_TO_LOG_OR
            .replace_all(&result, |caps: &regex::Captures| {
                let full = &caps[0];
                full.replacen(") |", ") ||", 1)
            })
            .to_string();

        result
    }

    // =========================================================================
    // A-3: Constant condition removal (RetDec dead_code_optimizer.cpp)
    //   if (true)  / if (1)  → keep body only
    //   if (false) / if (0)  → remove (keep else if present)
    //   while (false)        → remove
    //   Empty else { }       → remove
    // =========================================================================
    fn remove_constant_conditions(code: &str) -> String {
        let mut result = code.to_string();

        // while (false) { ... } → remove entirely
        static WHILE_FALSE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?s)\bwhile\s*\(\s*(?:false|0)\s*\)\s*\{[^}]*\}").unwrap()
        });
        result = WHILE_FALSE.replace_all(&result, "").to_string();

        // if (false) { ... } → remove (but keep else clause if present)
        static IF_FALSE_WITH_ELSE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?s)\bif\s*\(\s*(?:false|0)\s*\)\s*\{[^}]*\}\s*else\s*\{(?P<else_body>[^}]*)\}").unwrap()
        });
        result = IF_FALSE_WITH_ELSE
            .replace_all(&result, |caps: &regex::Captures| {
                caps["else_body"].trim().to_string()
            })
            .to_string();

        static IF_FALSE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?s)\bif\s*\(\s*(?:false|0)\s*\)\s*\{[^}]*\}").unwrap()
        });
        result = IF_FALSE.replace_all(&result, "").to_string();

        // if (true) { BODY } → BODY  (keep body, discard if wrapper)
        static IF_TRUE_WITH_ELSE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?s)\bif\s*\(\s*(?:true|1)\s*\)\s*\{(?P<body>[^}]*)\}\s*else\s*\{[^}]*\}").unwrap()
        });
        result = IF_TRUE_WITH_ELSE
            .replace_all(&result, |caps: &regex::Captures| {
                caps["body"].trim().to_string()
            })
            .to_string();

        static IF_TRUE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?s)\bif\s*\(\s*(?:true|1)\s*\)\s*\{(?P<body>[^}]*)\}").unwrap()
        });
        result = IF_TRUE
            .replace_all(&result, |caps: &regex::Captures| {
                caps["body"].trim().to_string()
            })
            .to_string();

        result
    }

    // =========================================================================
    // A-4: If structure simplification (RetDec if_structure_optimizer.cpp)
    //   Pattern 1: if (c) { return X; } else { S; }  →  if (c) { return X; } S;
    //   Pattern 2: Empty else removal:  } else { }  →  }
    // =========================================================================
    fn simplify_if_structure(code: &str) -> String {
        let mut result = code.to_string();

        // Pattern 1: if (c) { ... return ...; } else { BODY }  →  if (c) { ... return ...; } BODY
        // Safe because if-body always returns, so else is unreachable otherwise.
        static IF_RETURN_ELSE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(concat!(
                r"(?P<if_block>if\s*\([^)]+\)\s*\{[^}]*\breturn\b[^}]*\})",
                r"\s*else\s*\{(?P<else_body>[^}]*)\}",
            )).unwrap()
        });

        result = IF_RETURN_ELSE
            .replace_all(&result, |caps: &regex::Captures| {
                let if_block = &caps["if_block"];
                let else_body = caps["else_body"].trim();
                if else_body.is_empty() {
                    if_block.to_string()
                } else {
                    format!("{}\n{}", if_block, else_body)
                }
            })
            .to_string();

        // Empty else: } else { }  →  }
        static EMPTY_ELSE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\}\s*else\s*\{\s*\}").unwrap()
        });
        result = EMPTY_ELSE.replace_all(&result, "}").to_string();

        result
    }

    // =========================================================================
    // A-5: while(true) → while(cond)  (simple case)
    //   while (true) { if (cond) break; S; }  →  while (!cond) { S; }
    //   Only when the break-if is the FIRST statement in the loop body.
    // (RetDec while_true_to_while_cond_optimizer.cpp)
    // =========================================================================
    fn while_true_to_while_cond(code: &str) -> String {
        // Match: while (true) { if (COND) { break; } REST }
        // The COND must be a simple expression (no nested braces).
        static WHILE_TRUE_BREAK: Lazy<Regex> = Lazy::new(|| {
            Regex::new(concat!(
                r"(?P<indent>\s*)while\s*\(\s*(?:true|1)\s*\)\s*\{\s*\n",
                r"(?P<inner_indent>\s*)if\s*\(\s*(?P<cond>[^{}\n]+?)\s*\)\s*\{?\s*\n?\s*break\s*;\s*\}?\s*\n",
                r"(?P<body>(?s).*?)",
                r"\n(?P<close_indent>\s*)\}",
            )).unwrap()
        });

        WHILE_TRUE_BREAK
            .replace_all(code, |caps: &regex::Captures| {
                let indent = &caps["indent"];
                let cond = caps["cond"].trim();
                let body = &caps["body"];
                let close_indent = &caps["close_indent"];

                // Negate the condition
                let negated = negate_condition(cond);

                format!("{}while ({}) {{\n{}\n{}}}", indent, negated, body, close_indent)
            })
            .to_string()
    }

    // =========================================================================
    // B-1: while(true) → for loop  (RetDec while_true_to_for_loop_optimizer.cpp)
    //
    // Detects:
    //   init_var = start;
    //   while (true) {
    //       if (exit_cond) break;   // first statement
    //       body;
    //       init_var = init_var OP step;  // last statement (or init_var++/--)
    //   }
    // and transforms to:
    //   for (init_var = start; !exit_cond; init_var = init_var OP step) { body; }
    // =========================================================================
    fn while_true_to_for_loop(code: &str) -> String {
        let lines: Vec<&str> = code.lines().collect();
        let mut result_lines: Vec<String> = Vec::new();
        let mut changed = false;
        let mut i = 0;

        // Regex patterns
        static INIT_ASSIGN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^(\s*)(\w+)\s*=\s*(.+?)\s*;\s*$").unwrap()
        });
        static WHILE_TRUE_OPEN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^(\s*)while\s*\(\s*(?:true|1)\s*\)\s*\{\s*$").unwrap()
        });
        static IF_BREAK: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^\s*if\s*\(\s*(.+?)\s*\)\s*\{?\s*break\s*;\s*\}?\s*$").unwrap()
        });
        static UPDATE_INC: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^\s*(\w+)\s*(\+\+|--)\s*;\s*$").unwrap()
        });
        static UPDATE_COMPOUND: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^\s*(\w+)\s*(\+=|-=)\s*(.+?)\s*;\s*$").unwrap()
        });
        static UPDATE_PLAIN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^\s*(\w+)\s*=\s*(.+?)\s*;\s*$").unwrap()
        });

        while i < lines.len() {
            // Check for while(true) { preceded by init assignment
            if let Some(while_caps) = WHILE_TRUE_OPEN.captures(lines[i]) {
                let while_indent = while_caps[1].to_string();

                // Check previous line for init assignment
                let init_info = if i > 0 && !result_lines.is_empty() {
                    INIT_ASSIGN.captures(lines[i - 1]).map(|c| {
                        (c[2].to_string(), c[3].to_string())
                    })
                } else {
                    None
                };

                if let Some((init_var, init_expr)) = init_info {
                    // Find the closing brace (brace counting)
                    let mut depth: i32 = 1;
                    let mut body_end = lines.len() - 1;
                    for k in (i + 1)..lines.len() {
                        for ch in lines[k].chars() {
                            match ch {
                                '{' => depth += 1,
                                '}' => depth -= 1,
                                _ => {}
                            }
                        }
                        if depth == 0 {
                            body_end = k;
                            break;
                        }
                    }

                    // Collect body lines
                    let body_start = i + 1;
                    if body_end > body_start + 1 {
                        let body_lines = &lines[body_start..body_end];

                        // Check first body line for if(cond) break;
                        let break_cond = IF_BREAK.captures(body_lines[0]);

                        // Check last body line for update of init_var
                        let last_body = body_lines[body_lines.len() - 1];
                        let update_info = if let Some(caps) = UPDATE_INC.captures(last_body) {
                            let v = caps[1].to_string();
                            let op = caps[2].to_string();
                            Some((v, format!("{}{}", &caps[1], op)))
                        } else if let Some(caps) = UPDATE_COMPOUND.captures(last_body) {
                            let v = caps[1].to_string();
                            Some((v, format!("{} {} {}", &caps[1], &caps[2], &caps[3])))
                        } else if let Some(caps) = UPDATE_PLAIN.captures(last_body) {
                            let v = caps[1].to_string();
                            Some((v, caps[2].to_string()))
                        } else {
                            None
                        };

                        if let (Some(break_caps), Some((update_var, update_expr))) =
                            (break_cond, update_info)
                        {
                            let cond = break_caps[1].trim().to_string();

                            if update_var == init_var {
                                // All three components found → build for loop
                                let negated = negate_condition(&cond);

                                // Remove the init line we already pushed
                                result_lines.pop();

                                // Build update expression for the for header
                                let update_str = if update_expr.contains("++")
                                    || update_expr.contains("--")
                                    || update_expr.contains("+=")
                                    || update_expr.contains("-=")
                                {
                                    update_expr
                                } else {
                                    format!("{} = {}", update_var, update_expr)
                                };

                                result_lines.push(format!(
                                    "{}for ({} = {}; {}; {}) {{",
                                    while_indent, init_var, init_expr, negated, update_str
                                ));

                                // Body lines: skip first (if-break) and last (update)
                                for bl in &body_lines[1..body_lines.len() - 1] {
                                    result_lines.push(bl.to_string());
                                }

                                // Closing brace
                                result_lines.push(lines[body_end].to_string());

                                changed = true;
                                i = body_end + 1;
                                continue;
                            }
                        }
                    }
                }
            }

            result_lines.push(lines[i].to_string());
            i += 1;
        }

        if !changed {
            return code.to_string();
        }
        result_lines.join("\n")
    }

    // =========================================================================
    // B-2: Dead local assignment removal
    //  (RetDec dead_local_assign_optimizer.cpp)
    //
    // If a compiler-generated variable (local_XX, xVarN) appears exactly once
    // in the entire function — on the LHS of an assignment — and the RHS has
    // no side effects (no function call), the assignment is dead and removed.
    // Run twice to handle cascading dead assignments.
    // =========================================================================
    fn remove_dead_local_assigns(code: &str) -> String {
        static VAR_PATTERN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\b(local_\w+|[a-z]Var\d+)\b").unwrap()
        });
        static ASSIGN_LINE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^\s*(local_\w+|[a-z]Var\d+)\s*=\s*(.+?)\s*;\s*$").unwrap()
        });
        static FUNC_CALL: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\w+\s*\(").unwrap()
        });

        // Count occurrences of each variable
        let mut var_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for cap in VAR_PATTERN.find_iter(code) {
            *var_counts.entry(cap.as_str().to_string()).or_insert(0) += 1;
        }

        let lines: Vec<&str> = code.lines().collect();
        let mut result_lines: Vec<String> = Vec::new();
        let mut changed = false;

        for line in &lines {
            if let Some(caps) = ASSIGN_LINE.captures(line) {
                let lhs = &caps[1];
                let rhs = &caps[2];

                // Variable appears only once (on this LHS) → dead
                if let Some(&count) = var_counts.get(lhs) {
                    if count == 1 && !FUNC_CALL.is_match(rhs) {
                        changed = true;
                        continue; // skip dead assignment
                    }
                }
            }
            result_lines.push(line.to_string());
        }

        if !changed {
            return code.to_string();
        }
        result_lines.join("\n")
    }

    // =========================================================================
    // B-3: Induction variable naming
    //  (RetDec readable_var_renamer.cpp — visit(ForLoopStmt))
    //
    // Rename compiler-generated loop counter variables (local_XX, xVarN)
    // in for-loop headers to i, j, k, l, m, n based on nesting order.
    // Avoids collision with already-used identifiers.
    // =========================================================================
    fn rename_induction_vars(code: &str) -> String {
        static FOR_LOOP_VAR: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"for\s*\(\s*(\w+)\s*=").unwrap()
        });
        static COMPILER_VAR: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^(local_\w+|[a-z]Var\d+)$").unwrap()
        });

        let candidate_names = ["i", "j", "k", "l", "m", "n"];

        // Collect all identifiers currently in use
        let mut used_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        static ALL_IDENT: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\b([a-zA-Z_]\w*)\b").unwrap()
        });
        for cap in ALL_IDENT.find_iter(code) {
            used_ids.insert(cap.as_str().to_string());
        }

        // Collect unique induction variables in order of appearance
        let mut induction_vars: Vec<String> = Vec::new();
        for caps in FOR_LOOP_VAR.captures_iter(code) {
            let var = caps[1].to_string();
            if COMPILER_VAR.is_match(&var) && !induction_vars.contains(&var) {
                induction_vars.push(var);
            }
        }

        if induction_vars.is_empty() {
            return code.to_string();
        }

        let mut result = code.to_string();
        let mut assigned_names: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        let mut name_idx = 0;

        for var in &induction_vars {
            // Find next available name
            while name_idx < candidate_names.len() {
                let name = candidate_names[name_idx];
                // OK if the identifier is the var itself or not yet used
                if !used_ids.contains(name) || name == var.as_str() {
                    if !assigned_names.contains(name) {
                        break;
                    }
                }
                name_idx += 1;
            }
            if name_idx >= candidate_names.len() {
                break; // exhausted short names
            }

            let new_name = candidate_names[name_idx];
            assigned_names.insert(new_name.to_string());
            name_idx += 1;

            let pattern = format!(r"\b{}\b", regex::escape(var));
            if let Ok(re) = Regex::new(&pattern) {
                result = re.replace_all(&result, new_name).to_string();
            }
        }

        result
    }

    // =========================================================================
    // B-4: Semantic variable naming
    //  (RetDec readable_var_renamer.cpp — multiple visitors)
    //
    //  1. main() → param_1 = argc, param_2 = argv
    //  2. Single return-value temp → "result"
    //  3. API result naming: var = malloc(...) → ptr, strlen() → len, etc.
    // =========================================================================
    fn rename_semantic_vars(code: &str) -> String {
        let mut result = code.to_string();

        // --- 1. main() parameters ---
        static MAIN_SIG: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\bmain\s*\(").unwrap()
        });
        if MAIN_SIG.is_match(&result) {
            if let Ok(re) = Regex::new(r"\bparam_1\b") {
                if re.is_match(&result) {
                    result = re.replace_all(&result, "argc").to_string();
                }
            }
            if let Ok(re) = Regex::new(r"\bparam_2\b") {
                if re.is_match(&result) {
                    result = re.replace_all(&result, "argv").to_string();
                }
            }
        }

        // --- 2. Return-value variable naming ---
        // If the same compiler-generated temp is returned in every return
        // statement, rename it to "result".
        static RETURN_VAR: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\breturn\s+(\w+)\s*;").unwrap()
        });
        static COMPILER_NAME: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"^(local_\w+|[a-z]Var\d+)$").unwrap()
        });
        {
            let mut return_vars: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for caps in RETURN_VAR.captures_iter(&result) {
                let v = caps[1].to_string();
                if COMPILER_NAME.is_match(&v) {
                    return_vars.insert(v);
                }
            }
            if return_vars.len() == 1 {
                let var = return_vars.into_iter().next().unwrap();
                // Only rename if "result" isn't already used for something else
                if !result.contains("result") || result.contains(&var) {
                    let pat = format!(r"\b{}\b", regex::escape(&var));
                    if let Ok(re) = Regex::new(&pat) {
                        result = re.replace_all(&result, "result").to_string();
                    }
                }
            }
        }

        // --- 3. API result naming ---
        static API_ASSIGN: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"\b(local_\w+|[a-z]Var\d+)\s*=\s*(?:\([^)]*\)\s*)?(\w+)\s*\(").unwrap()
        });

        let api_map: std::collections::HashMap<&str, &str> = [
            ("malloc", "ptr"),
            ("calloc", "ptr"),
            ("realloc", "ptr"),
            ("mmap", "mapped"),
            ("strlen", "len"),
            ("wcslen", "len"),
            ("sizeof", "size"),
            ("fopen", "fp"),
            ("fdopen", "fp"),
            ("tmpfile", "fp"),
            ("fgets", "line"),
            ("fread", "bytes_read"),
            ("socket", "sock_fd"),
            ("accept", "client_fd"),
            ("open", "fd"),
            ("creat", "fd"),
            ("getenv", "env_val"),
            ("strcmp", "cmp"),
            ("strncmp", "cmp"),
            ("memcmp", "cmp"),
            ("strstr", "found"),
            ("strchr", "found"),
            ("strrchr", "found"),
            ("atoi", "num"),
            ("atol", "num"),
            ("strtol", "num"),
            ("strtoul", "num"),
            ("pthread_create", "thread_err"),
        ]
        .into_iter()
        .collect();

        // Collect rename pairs (old_name → new_base)
        let mut renames: Vec<(String, String)> = Vec::new();
        for caps in API_ASSIGN.captures_iter(&result.clone()) {
            let var = caps[1].to_string();
            let func = &caps[2];
            if let Some(&new_base) = api_map.get(func) {
                // Don't rename if already renamed by step 2
                if var != "result" && !renames.iter().any(|(o, _)| o == &var) {
                    renames.push((var, new_base.to_string()));
                }
            }
        }

        // Apply with collision avoidance
        let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (old, new_base) in &renames {
            let mut new_name = new_base.clone();
            let mut suffix = 2u32;
            while used.contains(&new_name) {
                new_name = format!("{}{}", new_base, suffix);
                suffix += 1;
            }
            used.insert(new_name.clone());
            let pat = format!(r"\b{}\b", regex::escape(old));
            if let Ok(re) = Regex::new(&pat) {
                result = re.replace_all(&result, new_name.as_str()).to_string();
            }
        }

        result
    }

    // =========================================================================
    // B-5: Loop idiom recognition  (LLVM LoopIdiomRecognize.cpp style)
    //
    //  Patterns:
    //  1. strlen:   while (*ptr != 0) { ptr++; }  →  len = strlen(ptr)
    //  2. popcount: cnt=0; while (v) { cnt++; v = v & (v-1); }
    //                 →  cnt = __builtin_popcount(v)
    //  3. memset:   for (i=0; i<N; i++) { buf[i] = 0; }
    //                 →  memset(buf, 0, N)
    // =========================================================================
    fn recognize_loop_idioms(code: &str) -> String {
        let mut result = code.to_string();

        // 1. strlen: while (*ptr != 0) { ptr = ptr + 1; }
        //        or: while (*(ptr + off) != 0) { off = off + 1; }
        static STRLEN_LOOP: Lazy<Regex> = Lazy::new(|| {
            Regex::new(concat!(
                r"while\s*\(\s*\*\s*(?P<ptr>[\w\->\.\[\]]+)\s*!=\s*(?:0|'\\0'|'\0')\s*\)\s*\{\s*",
                r"(?P<upd>[\w\->\.\[\]]+)\s*=\s*(?P<upd2>[\w\->\.\[\]]+)\s*\+\s*1\s*;\s*\}",
            ))
            .unwrap()
        });

        result = STRLEN_LOOP
            .replace_all(&result, |caps: &regex::Captures| {
                let ptr = &caps["ptr"];
                let upd = &caps["upd"];
                let upd2 = &caps["upd2"];
                if upd == upd2 {
                    format!("/* strlen loop detected */ {} += strlen({})", upd, ptr)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        // 2. popcount: cnt = 0; while (val != 0) { cnt = cnt + 1; val = val & val - 1; }
        //   Also variant: val = val & (val - 1);
        static POPCOUNT_LOOP: Lazy<Regex> = Lazy::new(|| {
            Regex::new(concat!(
                r"(?P<cnt>\w+)\s*=\s*0\s*;\s*",
                r"while\s*\(\s*(?P<val>\w+)\s*!=\s*0\s*\)\s*\{\s*",
                r"(?P<cnt2>\w+)\s*=\s*(?P<cnt3>\w+)\s*\+\s*1\s*;\s*",
                r"(?P<val2>\w+)\s*=\s*(?P<val3>\w+)\s*&\s*",
                r"(?:(?P<val4>\w+)\s*-\s*1|\(\s*(?P<val5>\w+)\s*-\s*1\s*\))\s*;\s*\}",
            ))
            .unwrap()
        });

        result = POPCOUNT_LOOP
            .replace_all(&result, |caps: &regex::Captures| {
                let cnt = &caps["cnt"];
                let val = &caps["val"];
                let val3 = &caps["val3"];
                let val_minus = caps
                    .name("val4")
                    .or_else(|| caps.name("val5"))
                    .map(|m| m.as_str())
                    .unwrap_or("");
                if cnt == &caps["cnt2"]
                    && cnt == &caps["cnt3"]
                    && val == &caps["val2"]
                    && val == val3
                    && val == val_minus
                {
                    format!("{} = __builtin_popcount({})", cnt, val)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        // 3. memset: for (i = 0; i < N; i++) { buf[i] = 0; }
        //   or: for (i = 0; i < N; i = i + 1) { buf[i] = 0; }
        static MEMSET_LOOP: Lazy<Regex> = Lazy::new(|| {
            Regex::new(concat!(
                r"for\s*\(\s*(?P<iv>\w+)\s*=\s*0\s*;\s*",
                r"(?P<iv2>\w+)\s*<\s*(?P<sz>[^;]+?)\s*;\s*",
                r"(?P<iv3>\w+)\s*(?:\+\+|=\s*(?P<iv4>\w+)\s*\+\s*1)\s*\)\s*\{\s*",
                r"(?P<buf>\w+)\s*\[\s*(?P<iv5>\w+)\s*\]\s*=\s*(?P<val>0|'\\0')\s*;\s*\}",
            ))
            .unwrap()
        });

        result = MEMSET_LOOP
            .replace_all(&result, |caps: &regex::Captures| {
                let iv = &caps["iv"];
                let buf = &caps["buf"];
                let sz = &caps["sz"];
                let iv4 = caps.name("iv4").map(|m| m.as_str()).unwrap_or(iv);
                if iv == &caps["iv2"]
                    && iv == &caps["iv3"]
                    && iv == iv4
                    && iv == &caps["iv5"]
                {
                    format!("memset({}, 0, {})", buf, sz.trim())
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        result
    }

    /// Reconstruct switch/case from BST (binary search tree) or sequential
    /// equality-check patterns that survive C++ post-processing.
    ///
    /// Patterns handled:
    ///
    /// 1. Flat sequential:
    ///    ```text
    ///    if (var == 0) { return 10; }
    ///    if (var == 1) { return 20; }
    ///    if (var == 2) { return 30; }
    ///    return default;
    ///    ```
    ///
    /// 2. BST with range guards:
    ///    ```text
    ///    if (var == 2) { return 30; }
    ///    if (var < 3) {
    ///        if (!var) { return 10; }
    ///        if (var == 1) { return 20; }
    ///    }
    ///    return default;
    ///    ```
    fn reconstruct_switch_from_bst(code: &str) -> String {
        let lines: Vec<&str> = code.lines().collect();
        if lines.len() < 4 {
            return code.to_string();
        }

        struct CaseEntry {
            value: String,
            stmt: String,
        }

        let close_brace_only =
            Regex::new(r"^\s*\}\s*$").unwrap();

        let mut result_lines: Vec<String> = Vec::new();
        let mut i = 0;
        let mut changed = false;

        while i < lines.len() {
            let mut cases: Vec<CaseEntry> = Vec::new();
            let mut var_name: Option<String> = None;
            let mut base_indent = String::new();
            let mut bst_depth: i32 = 0;
            let mut block_end = i;

            // Try to collect a run of equality-return patterns
            let mut j = i;
            while j < lines.len() {
                let line = lines[j];

                // Try equality-return: if (var == N) { return X; }
                if let Some(caps) = SEQ_EQ_RETURN.captures(line) {
                    let vn = caps[2].to_string();
                    if var_name.is_none() {
                        var_name = Some(vn.clone());
                        base_indent = caps[1].to_string();
                    }
                    if var_name.as_deref() == Some(vn.as_str()) {
                        cases.push(CaseEntry {
                            value: caps[3].to_string(),
                            stmt: caps[4].to_string(),
                        });
                        block_end = j;
                        j += 1;
                        continue;
                    }
                }

                // Try reverse form: if (N == var) { return X; }
                if let Some(caps) = SEQ_EQ_RETURN_REV.captures(line) {
                    let vn = caps[3].to_string();
                    if var_name.is_none() {
                        var_name = Some(vn.clone());
                        base_indent = caps[1].to_string();
                    }
                    if var_name.as_deref() == Some(vn.as_str()) {
                        cases.push(CaseEntry {
                            value: caps[2].to_string(),
                            stmt: caps[4].to_string(),
                        });
                        block_end = j;
                        j += 1;
                        continue;
                    }
                }

                // Try: if (!var) { return X; } — equivalent to var == 0
                if let Some(caps) = SEQ_NOT_RETURN.captures(line) {
                    let vn = caps[2].to_string();
                    if var_name.is_none() {
                        var_name = Some(vn.clone());
                        base_indent = caps[1].to_string();
                    }
                    if var_name.as_deref() == Some(vn.as_str()) {
                        cases.push(CaseEntry {
                            value: "0".to_string(),
                            stmt: caps[3].to_string(),
                        });
                        block_end = j;
                        j += 1;
                        continue;
                    }
                }

                // Try range guard: if (var < N) { — BST node
                if let Some(caps) = RANGE_GUARD_OPEN.captures(line) {
                    let vn = caps[1].to_string();
                    if var_name.as_deref() == Some(vn.as_str()) {
                        // Count braces
                        let nb: i32 = line.chars().map(|c| match c {
                            '{' => 1,
                            '}' => -1,
                            _ => 0,
                        }).sum();
                        bst_depth += nb;
                        block_end = j;
                        j += 1;
                        continue;
                    }
                }

                // Closing brace for BST range guard
                if bst_depth > 0 && close_brace_only.is_match(line) {
                    bst_depth -= 1;
                    block_end = j;
                    j += 1;
                    continue;
                }

                // No match — end of block
                if cases.is_empty() {
                    break;
                }
                break;
            }

            // Need at least 3 cases to reconstruct a switch
            if cases.len() < 3 {
                result_lines.push(lines[i].to_string());
                i += 1;
                continue;
            }

            // Check for a default return after the block
            let mut has_default = false;
            let mut default_stmt = String::new();
            let after = block_end + 1;
            if after < lines.len() {
                if let Some(caps) = DEFAULT_RETURN.captures(lines[after]) {
                    default_stmt = caps[1].to_string();
                    has_default = true;
                    block_end = after;
                }
            }

            // Build switch
            result_lines.push(format!("{}switch ({}) {{", base_indent, var_name.as_deref().unwrap_or("?")));
            for c in &cases {
                result_lines.push(format!("{}case {}:", base_indent, c.value));
                result_lines.push(format!("{}    {}", base_indent, c.stmt));
            }
            if has_default {
                result_lines.push(format!("{}default:", base_indent));
                result_lines.push(format!("{}    {}", base_indent, default_stmt));
            }
            result_lines.push(format!("{}}}", base_indent));

            changed = true;
            i = block_end + 1;
        }

        if !changed {
            return code.to_string();
        }

        result_lines.join("\n")
    }

    /// Apply DWARF debug info to substitute parameter and local variable names.
    ///
    /// Ghidra generates names like `param_1`, `param_2`, `local_38`, `local_10`, etc.
    /// If DWARF provides real names, we substitute them.
    ///
    /// Matching strategies:
    /// - `param_N` → Nth DWARF parameter name (1-indexed)
    /// - `local_XX` → DWARF local var where XX is the absolute hex stack offset
    ///   (Ghidra: `local_38` means StackOffset(-0x38))
    /// - `in_REG` → DWARF param/var located in that register
    fn apply_dwarf_names(&self, code: &str) -> String {
        let Some(ref dwarf) = self.dwarf_info else {
            return code.to_string();
        };
        let mut result = code.to_string();

        // 1. Substitute param_N → DWARF parameter names
        for (i, param) in dwarf.params.iter().enumerate() {
            let ghidra_name = format!("param_{}", i + 1);
            // Word-boundary replacement to avoid partial matches
            let pattern = format!(r"\b{}\b", regex::escape(&ghidra_name));
            if let Ok(re) = Regex::new(&pattern) {
                result = re.replace_all(&result, param.name.as_str()).to_string();
            }
        }

        // 2. Substitute local_XX → DWARF local variable names (stack offset matching)
        for var in &dwarf.local_vars {
            if let DwarfLocation::StackOffset(offset) = &var.location {
                // Ghidra convention: local_XX where XX is the absolute hex offset from frame base
                let abs_offset = offset.unsigned_abs();
                if abs_offset > 0 {
                    let ghidra_name = format!("local_{:x}", abs_offset);
                    let pattern = format!(r"\b{}\b", regex::escape(&ghidra_name));
                    if let Ok(re) = Regex::new(&pattern) {
                        result = re.replace_all(&result, var.name.as_str()).to_string();
                    }
                }
            }
        }

        // 3. Substitute in_REG → DWARF param/var in that register
        // Build register name → DWARF name mapping
        let x86_64_dwarf_to_name = |reg_num: u16| -> Option<&'static str> {
            match reg_num {
                0 => Some("RAX"),
                1 => Some("RDX"),
                2 => Some("RCX"),
                3 => Some("RBX"),
                4 => Some("RSI"),
                5 => Some("RDI"),
                6 => Some("RBP"),
                7 => Some("RSP"),
                8 => Some("R8"),
                9 => Some("R9"),
                10 => Some("R10"),
                11 => Some("R11"),
                12 => Some("R12"),
                13 => Some("R13"),
                14 => Some("R14"),
                15 => Some("R15"),
                _ => None,
            }
        };

        for param in &dwarf.params {
            if let DwarfLocation::Register(ref reg_str) = param.location {
                // reg_str is like "reg5" → map to x86_64 register name
                if let Some(num_str) = reg_str.strip_prefix("reg") {
                    if let Ok(reg_num) = num_str.parse::<u16>() {
                        if let Some(reg_name) = x86_64_dwarf_to_name(reg_num) {
                            let ghidra_name = format!("in_{}", reg_name);
                            let pattern = format!(r"\b{}\b", regex::escape(&ghidra_name));
                            if let Ok(re) = Regex::new(&pattern) {
                                result =
                                    re.replace_all(&result, param.name.as_str()).to_string();
                            }
                        }
                    }
                }
            }
        }

        // 4. Substitute return type in function signature if available
        if let Some(ref ret_type) = dwarf.return_type {
            // Replace "undefined8" or "undefined" with actual return type
            static RET_TYPE_PATTERN: Lazy<Regex> = Lazy::new(|| {
                Regex::new(r"\b(undefined\d*)\b\s+(\w+\s*\()").unwrap()
            });
            result = RET_TYPE_PATTERN
                .replace(&result, |caps: &regex::Captures| {
                    format!("{} {}", ret_type, &caps[2])
                })
                .to_string();
        }

        result
    }
}

/// Negate a simple C condition expression for while(true)→while(cond) conversion.
///
/// Examples:
/// - `x >= 10`  → `x < 10`
/// - `!done`    → `done`
/// - `x == 0`   → `x != 0`
/// - `complex`  → `!(complex)`
fn negate_condition(cond: &str) -> String {
    let cond = cond.trim();

    // Already negated: !expr → expr
    if let Some(inner) = cond.strip_prefix('!') {
        let inner = inner.trim();
        // !(expr) → expr  or  !var → var
        if let Some(stripped) = inner.strip_prefix('(') {
            if let Some(stripped) = stripped.strip_suffix(')') {
                return stripped.trim().to_string();
            }
        }
        return inner.to_string();
    }

    // Comparison operators — flip them
    let comparisons = [
        (">=", "<"),
        ("<=", ">"),
        ("!=", "=="),
        ("==", "!="),
        (">", "<="),
        ("<", ">="),
    ];
    for (op, negated) in &comparisons {
        if let Some(pos) = cond.find(op) {
            let lhs = &cond[..pos];
            let rhs = &cond[pos + op.len()..];
            return format!("{}{}{}", lhs, negated, rhs);
        }
    }

    // Fallback: wrap with !()
    format!("!({})", cond)
}

impl Default for PostProcessor {
    fn default() -> Self {
        Self::new()
    }
}
