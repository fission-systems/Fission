//! Centralized Regex Pattern Library
//!
//! This module consolidates all regex patterns used across the analysis codebase
//! to eliminate duplication and improve maintainability.
//!
//! # Pattern Categories
//!
//! - **Arithmetic**: Sign extension, division, multiplication, bitwise ops
//! - **Naming**: Variable naming, field access, function calls
//! - **Control Flow**: If/else, loops, switch statements
//! - **Cleanup**: Code simplification and sanitization
//! - **Structure**: Code structure normalization

use regex::Regex;

// ============================================================================
// Arithmetic Patterns
// ============================================================================

/// Pattern for int64 sign extension via CONCAT44
pub static SIGN_EXT_INT64_CONCAT: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(concat!(
        r"(?s)",
        r"(?P<s1>\w+)\s*=\s*(?P<high>\w+)\s*>>\s*0x1[fF];\s*",
        r"(?P<m1>\w+)\s*=\s*\((?P<low>[\w\->\.\*]+)\s*\^\s*(?P<s2>\w+)\)\s*-\s*(?P<s3>\w+)\s*&\s*1\s*\^\s*(?P<s4>\w+);\s*",
        r"return\s*CONCAT44\s*\(-\s*\(uint\)\s*\((?P<m2>\w+)\s*<\s*(?P<s5>\w+)\),\s*(?P<m3>\w+)\s*-\s*(?P<s6>\w+)\);"
    ))
    .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Pattern for int32 sign extension
pub static SIGN_EXT_INT32: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?s)(?P<s1>\w+)\s*=\s*(?P<high>\w+)\s*>>\s*0x1[fF];\s*(?P<out>\w+)\s*=\s*\((?P<val>[\w\->\.\*]+)\s*\^\s*(?P<s2>\w+)\)\s*-\s*(?P<s3>\w+);")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Pattern for aligned division optimization
pub static ALIGNED_DIV: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?s)\(\s*(?P<val>[\w\->\.\*]+)\s*\+\s*\((?P<s1>\w+)\s*>>\s*(?P<sh1>0x[0-9a-fA-F]+)\)\s*&\s*(?P<mask>0x[0-9a-fA-F]+|[\d]+)\s*\)\s*-\s*\(\s*(?P<s2>\w+)\s*>>\s*(?P<sh2>0x[0-9a-fA-F]+|[\d]+)\s*\)")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Extract sign bit pattern
pub static SIGN_BIT: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\(int\)\s*(\w+)\s*>>\s*0x1[fF]")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Magic number division pattern
pub static MAGIC_DIV: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?s)\(uint\)\s*\(\s*\(ulonglong\)\s*(?P<val>[\w\->\.\*]+)\s*\*\s*(?P<magic>0x[0-9a-fA-F]+)\s*>>\s*0x20\s*\)\s*(?:>>\s*(?P<shift>0x[0-9a-fA-F]+|[\d]+))?")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// CONCAT44 sign extension pattern
pub static CONCAT44_SIGN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(
        r"CONCAT44\s*\(\s*(?P<hi>[\w\->\.\*]+)\s*>>\s*0x1[fF]\s*,\s*(?P<lo>[\w\->\.\*]+)\s*\)",
    )
    .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// CONCAT44 zero extension pattern
pub static CONCAT44_ZERO: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"CONCAT44\s*\(\s*0\s*,\s*(?P<lo>[^)]+?)\s*\)")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// CONCAT with input register pattern (first position)
pub static CONCAT_INPUT_FIRST: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"CONCAT\d+\s*\(\s*(?:(?:\([^)]*\)\s*)?in_\w+\s*,\s*(?P<real1>[^,)]+))\s*\)")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// CONCAT with input register pattern (second position)
pub static CONCAT_INPUT_SECOND: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"CONCAT\d+\s*\(\s*(?P<real2>[^,)]+?)\s*,\s*(?:\([^)]*\)\s*)?in_\w+\s*\)")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// CONCAT with capital input register pattern
pub static CONCAT_CAP_INPUT: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"CONCAT\d+\s*\(\s*(?:\([^)]*\)\s*)?(?:in_[A-Z]\w*)\s*,\s*(?P<lo_val>[^)]+?)\s*\)")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Modulo to subtraction pattern
pub static MODULO_TO_SUB: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?s)\(\s*(?P<val>[\w\->\.\*]+)\s*-\s*\(\s*(?:[\w\s\(\)\*>>&\^|~]+)\s*\+\s*(?P<v2>[\w\->\.\*]+)\s*/\s*(?P<divisor>\d+)\s*\)\s*\)")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Unsigned right shift pattern
pub static UNSIGNED_RSHIFT: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\(uint\)\s*(?P<val>\w+)\s*>>\s*(?P<sh>\d+)\b")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Left shift pattern
pub static LEFT_SHIFT: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\b(?P<val>\w+)\s*<<\s*(?P<sh>[1-9]\d*)\b")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Bitwise AND mask pattern
pub static BITWISE_AND_MASK: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\b(?P<val>\w+)\s*&\s*(?P<mask>0x[0-9a-fA-F]+)\b")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Signed 32-bit overflow/wrap pattern (complex)
pub static SIGNED_OVERFLOW_32: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(concat!(
        r"\(\s*\((?P<val1>\w+)\s*-\s*(?P<low1>0x[0-9a-fA-F]+|-?\d+)\s*\)\s*",
        r"\^\s*(?P<high>\w+)\s*>>\s*0x1[fF]\s*\)\s*-\s*",
        r"\(\s*(?P<val2>\w+)\s*-\s*(?P<low2>0x[0-9a-fA-F]+|-?\d+)\s*\)"
    ))
    .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Signed 64-bit overflow/wrap pattern (complex)
pub static SIGNED_OVERFLOW_64: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(concat!(
        r"CONCAT44\s*\(\s*",
        r"\(\s*\(\s*(?P<valh1>\w+)\s*-\s*(?P<lowh1>0x[0-9a-fA-F]+|-?\d+)\s*\)\s*",
        r"\^\s*(?P<high>\w+)\s*>>\s*0x3[fF]\s*\)\s*-\s*",
        r"\(\s*(?P<valh2>\w+)\s*-\s*(?P<lowh2>0x[0-9a-fA-F]+|-?\d+)\s*\)\s*,\s*",
        r"\(\s*\(\s*(?P<vall1>\w+)\s*-\s*(?P<lowl1>0x[0-9a-fA-F]+|-?\d+)\s*\)\s*",
        r"\^\s*(?P<carry>\w+)\s*\)\s*-\s*",
        r"\(\s*(?P<vall2>\w+)\s*-\s*(?P<lowl2>0x[0-9a-fA-F]+|-?\d+)\s*\)\s*\)"
    ))
    .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// XOR with sign bit pattern
pub static XOR_SIGN_BIT: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\b(?P<val>\w+)\s*\^\s*(?:0x80000000|-2147483648)\b")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Multiplication context detector
pub static MULT_CONTEXT: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\*\s*(0[xX][0-9a-fA-F]+|[1-9][0-9]*)")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Bitwise operation context detector
pub static BITWISE_CTX: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r">>|<<|\^|\b&\b|\|").unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

// ============================================================================
// Naming Patterns
// ============================================================================

/// Pointer offset pattern: *(ptr + offset)
pub static PTR_OFFSET: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\*\s*\(\s*(\w+)\s*\+\s*(0x[0-9a-fA-F]+|\d+)\s*\)")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Cast pointer offset pattern: *(type*)(ptr + offset)
pub static CAST_PTR_OFFSET: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\*\s*\(\s*[\w\s]+\*\s*\)\s*\(\s*(\w+)\s*\+\s*(0x[0-9a-fA-F]+|\d+)\s*\)")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Array index pattern: var[0xNN]
pub static ARRAY_INDEX: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(\w+)\[\s*(0x[0-9a-fA-F]+)\s*\]")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Field offset pattern: var._`N_M`_
pub static FIELD_OFFSET: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(\w+)\._([\d]+)_([\d]+)_")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Double pointer deref pattern: (**(*ptr + offset))
pub static DOUBLE_PTR_DEREF: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\(\*\*\([\w\s\*]+\)\(\*(\w+)\s*\+\s*(0x[0-9a-fA-F]+)\)\)")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// XMM register field pattern: axVarN._`8_8`_
pub static XMM_FIELD: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(axVar\d+)\._(8)_(8)_").unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Swift/Rust mangled name pattern
pub static MANGLED_NAME: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(_\$s[a-zA-Z0-9_\$]+|__T[a-zA-Z0-9_\$]+|_T[a-zA-Z0-9_\$]+)")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// For loop initialization pattern
pub static FOR_INIT: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"for\s*\(\s*(\w+)\s*=").unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Generic variable pattern
pub static GENERIC_VAR: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^(local_\w+|[a-z]Var\d+)$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Identifier pattern
pub static IDENTIFIER: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\b([a-zA-Z_]\w*)\b").unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Main function entry pattern
pub static MAIN_FUNC: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\bmain\s*\(").unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Return statement pattern
pub static RETURN_STMT: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\breturn\s+(\w+)\s*;").unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Function call assignment pattern
pub static FUNC_CALL_ASSIGN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\b(local_\w+|[a-z]Var\d+)\s*=\s*(?:\([^)]*\)\s*)?(\w+)\s*\(")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Undefined type declaration pattern
pub static UNDEF_TYPE_DECL: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\b(undefined\d*)\b\s+(\w+\s*\()")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

// ============================================================================
// Control Flow Patterns
// ============================================================================

/// Empty else block pattern
pub static EMPTY_ELSE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\}\s*else\s*\{\s*\}").unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// If-else with gotos pattern
pub static IF_ELSE_GOTO: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(concat!(
        r"(?s)if\s*\(\s*([^\{]+?)\s*\)\s*\{\s*",
        r"(?P<then_body>(?:[^{}]|\{[^}]*\})*?)",
        r"goto\s+(?P<then_label>\w+);\s*\}\s*else\s*\{\s*",
        r"(?P<else_body>(?:[^{}]|\{[^}]*\})*?)",
        r"goto\s+(?P<else_label>\w+);\s*\}"
    ))
    .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// If not pattern: if (!var) { return ...; }
pub static IF_NOT_PATTERN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^(\s*)if\s*\(\s*!(\w+)\s*\)\s*\{\s*(return\s+[^;]+;)\s*\}")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// If comparison pattern
pub static IF_COMPARISON: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*if\s*\(\s*(\w+)\s*[<>]=?\s*(?:0[xX][0-9a-fA-F]+|\d+)\s*\)\s*\{")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Return statement (line level)
pub static RETURN_LINE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(return\s+[^;]+;)\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Close brace only pattern
pub static CLOSE_BRACE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*\}\s*$").unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// If-else combined pattern
pub static IF_ELSE_COMBINED: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^(\s*)if\s*\(\s*(\w+)\s*(?:==|!=|<|<=|>|>=)\s*([^)]+)\s*\)\s*\{\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Else block start pattern
pub static ELSE_START: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(?:\}\s*)?else\s*\{\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Assignment statement pattern
pub static ASSIGNMENT: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(\w+)\s*=\s*([^;]+);\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

// ============================================================================
// Loop Patterns
// ============================================================================

/// While true pattern
pub static WHILE_TRUE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^(\s*)while\s*\(\s*(?:true|1)\s*\)\s*\{\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// If break pattern
pub static IF_BREAK: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*if\s*\(\s*(.+?)\s*\)\s*\{?\s*break\s*;\s*\}?\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Increment/decrement pattern
pub static INC_DEC: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(\w+)\s*(\+\+|--)\s*;\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Compound assignment pattern
pub static COMPOUND_ASSIGN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(\w+)\s*(\+=|-=)\s*(.+?)\s*;\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Simple assignment in loop pattern
pub static LOOP_ASSIGN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(\w+)\s*=\s*(.+?)\s*;\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// While true (multiline) pattern
pub static WHILE_TRUE_ML: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?m)^(\s*)while\s*\(\s*(?:true|1)\s*\)\s*\{")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// For init-cond-inc pattern
pub static FOR_PATTERN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(concat!(
        r"(?s)^(\s*)(\w+)\s*=\s*([^;]+);\s*",
        r"while\s*\(\s*(?:true|1)\s*\)\s*\{\s*",
        r"if\s*\(\s*(.+?)\s*\)\s*\{?\s*break;\s*\}?\s*",
        r"(.*?)\s*",
        r"(\w+)\s*(\+\+|--|\+=|-=|\*=|/=|\+|-|=)\s*([^;]*);?\s*",
        r"\}"
    ))
    .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// While to for conversion pattern
pub static WHILE_TO_FOR: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(concat!(
        r"^(\s*)(\w+)\s*=\s*([^;]+);\s*",
        r"while\s*\(\s*(\w+)\s*",
        r"(==|!=|<|<=|>|>=)\s*",
        r"([^)]+)\s*\)\s*\{\s*$"
    ))
    .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// For with initialization pattern
pub static FOR_WITH_INIT: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(concat!(
        r"^(\s*)for\s*\(\s*",
        r"(\w+)\s*=\s*([^;]+)\s*;\s*",
        r"(\w+)\s*(==|!=|<|<=|>|>=)\s*([^;]+)\s*;\s*",
        r"(\w+)(\+\+|--)\s*\)\s*\{\s*$"
    ))
    .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Do-while pattern
pub static DO_OPEN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^(\s*)do\s*\{\s*$").unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Do-while close pattern
pub static DO_WHILE_CLOSE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^(\s*)\}\s*while\s*\(\s*(\w+)\s*(!=|<|<=|>|>=)\s*([^)]+)\)\s*;\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Single identifier pattern
pub static SINGLE_IDENT: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"^\w+$").unwrap_or_else(|e| panic!("regex should compile: {e}")));

/// Cast to simple variable pattern
pub static CAST_VAR: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(?:\(\s*\w+\s*\)\s*)?(\w+)\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Increment (++) pattern
pub static INC_PP: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(\w+)\s*\+\+\s*;\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Add-assign (+=) pattern
pub static ADD_ASSIGN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(\w+)\s*\+=\s*(.+?)\s*;\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Add pattern: var = var + expr
pub static ADD_PATTERN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(\w+)\s*=\s*(\w+)\s*\+\s*(.+?)\s*;\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

// ============================================================================
// Cleanup Patterns
// ============================================================================

/// Inline assembly pattern
pub static INLINE_ASM: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?s)__asm\s*\([^\)]*\)|asm\s*\([^\)]*\)|__asm__\s*\([^\)]*\)")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Printf-like debug pattern
pub static PRINTF_DEBUG: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?s)(printf|fprintf|NSLog|os_log|android_log_print)\s*\([^\{]*\);?")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Go panic pattern
pub static GO_PANIC: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?s)if\s*\([^\{]*\)\s*\{\s*runtime\.gopanic\([^\{]*\);?\s*\}")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Malloc/calloc/realloc pattern
pub static MALLOC_PATTERN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(\w+)\s*=\s*((?:malloc|calloc|realloc)\s*\([^;]*\))\s*;")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Long type pointer offset pattern
pub static LONG_PTR_OFFSET: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\*\s*\(\s*((?:ulong|ulonglong|undefined\d*|long|longlong)\s+\w+)\s*\+\s*(0x[0-9a-fA-F]+|\d+)\s*\)")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Base array access pattern: *(base + idx)
pub static BASE_ARRAY_ACCESS: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\*\s*\(\s*(?P<base>[\w\->\.]+)\s*\+\s*(?P<idx>[\w\->\.0-9]+)\s*\)")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Reverse array access pattern: *(idx + base)
pub static REV_ARRAY_ACCESS: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\*\s*\(\s*(?P<idx>\d+|0x[0-9a-fA-F]+)\s*\+\s*(?P<base>[\w\->\.]+)\s*\)")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Multi-line array conversion pattern (first part)
pub static ARRAY_CONV_FIRST: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(concat!(
        r"(?s)(\w+)\s*=\s*",
        r"\*\s*\(\s*(?P<type>[\w\s]+?)\s*\)\s*",
        r"\(\s*(?P<base>[\w\->\.]+)\s*\+\s*(?P<idx>[\w\->\.0-9]+)\s*\)\s*;"
    ))
    .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Multi-line array conversion pattern (second part)
pub static ARRAY_CONV_SECOND: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(concat!(
        r"(?s)\*\s*\(\s*(?P<type>[\w\s]+?)\s*\)\s*",
        r"\(\s*(?P<base>[\w\->\.]+)\s*\+\s*(?P<idx>[\w\->\.0-9]+)\s*\)\s*=\s*",
        r"([^;]+)\s*;"
    ))
    .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// While false pattern
pub static WHILE_FALSE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?s)\bwhile\s*\(\s*(?:false|0)\s*\)\s*\{(?P<body>[^}]*)\}")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// If false with else pattern
pub static IF_FALSE_ELSE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?s)\bif\s*\(\s*(?:false|0)\s*\)\s*\{[^}]*\}\s*else\s*\{(?P<else_body>[^}]*)\}")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// If false pattern
pub static IF_FALSE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?s)\bif\s*\(\s*(?:false|0)\s*\)\s*\{(?P<body>[^}]*)\}")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// If true with else pattern
pub static IF_TRUE_ELSE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?s)\bif\s*\(\s*(?:true|1)\s*\)\s*\{(?P<body>[^}]*)\}\s*else\s*\{[^}]*\}")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// If true pattern
pub static IF_TRUE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"(?s)\bif\s*\(\s*(?:true|1)\s*\)\s*\{(?P<body>[^}]*)\}")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Generic local variable pattern
pub static LOCAL_VAR_PATTERN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"\b(local_\w+|[a-z]Var\d+)\b")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Local variable assignment pattern
pub static LOCAL_VAR_ASSIGN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(local_\w+|[a-z]Var\d+)\s*=\s*(.+?)\s*;\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Function call context pattern
pub static FUNC_CALL: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| Regex::new(r"\w+\s*\(").unwrap_or_else(|e| panic!("regex should compile: {e}")));

// ============================================================================
// Switch/Case Reconstruction Patterns
// ============================================================================

/// Sequential equality check with return: if (var == N) { return expr; }
pub static SEQ_EQ_RETURN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(
        r"^(\s*)if\s*\(\s*(\w+)\s*==\s*(-?(?:0[xX][0-9a-fA-F]+|\d+))\s*\)\s*\{\s*(return\s+[^;]+;)\s*\}",
    )
    .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Reverse form: if (N == var) { return expr; }
pub static SEQ_EQ_RETURN_REV: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(
        r"^(\s*)if\s*\(\s*(-?(?:0[xX][0-9a-fA-F]+|\d+))\s*==\s*(\w+)\s*\)\s*\{\s*(return\s+[^;]+;)\s*\}",
    )
    .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Pattern: if (!var) { return expr; } (equivalently var == 0)
pub static SEQ_NOT_RETURN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^(\s*)if\s*\(\s*!(\w+)\s*\)\s*\{\s*(return\s+[^;]+;)\s*\}")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Range guard for BST: if (var < N) { or if (var > N) {
pub static RANGE_GUARD_OPEN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*if\s*\(\s*(\w+)\s*[<>]=?\s*(?:0[xX][0-9a-fA-F]+|\d+)\s*\)\s*\{")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Default return statement
pub static DEFAULT_RETURN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(return\s+[^;]+;)\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Multi-line equality opening: if (var == N) {
pub static ML_EQ_OPEN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^(\s*)if\s*\(\s*(\w+)\s*==\s*(-?(?:0[xX][0-9a-fA-F]+|\d+))\s*\)\s*\{\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Multi-line not opening: if (!var) {
pub static ML_NOT_OPEN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^(\s*)if\s*\(\s*!(\w+)\s*\)\s*\{\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Multi-line return line pattern
pub static ML_RETURN_LINE: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(return\s+[^;]+;)\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// If-not opening for switch reconstruction: if (!var) {
pub static IF_NOT_OPEN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^(\s*)if\s*\(\s*!(\w+)\s*\)\s*\{\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// If-equals opening: if (var == N) {
pub static IF_EQ_OPEN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^(\s*)if\s*\(\s*(\w+)\s*==\s*(-?(?:0[xX][0-9a-fA-F]+|\d+))\s*\)\s*\{\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Else-if arm opening: } else if (var == N) {
pub static ELSE_IF_EQ_OPEN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(
        r"^\s*(?:\}\s*)?else\s+if\s*\(\s*(\w+)\s*==\s*(-?(?:0[xX][0-9a-fA-F]+|\d+))\s*\)\s*\{\s*$",
    )
    .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Else block opening: } else {
pub static ELSE_OPEN: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*(?:\}\s*)?else\s*\{\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});

/// Return variable pattern: return var;
pub static RETURN_VAR: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r"^\s*return\s+(\w+)\s*;\s*$")
        .unwrap_or_else(|e| panic!("regex should compile: {e}"))
});
