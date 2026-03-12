//! String pointer replacement pass
//!
//! Replaces raw address literals (e.g. `0x408020`, `&DAT_140004038`) with
//! actual string content when the address corresponds to a known string
//! in `.rdata`/`.rodata`.

use super::pass::{PassCategory, PassMetadata, PassResult, PostProcessPass};
use regex::Regex;
use std::borrow::Cow;

/// Escape a string for use inside C string literals.
///
/// Converts `\n`, `\r`, `\t`, `\`, `"` to their escaped form.
fn escape_for_c_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\0' => out.push_str("\\0"),
            c if c.is_ascii() && (c as u8) < 0x20 => {
                out.push_str(&format!("\\x{:02x}", c as u8));
            }
            c => out.push(c),
        }
    }
    out
}

/// Parse a 16-digit hex address from a match (supports 32 and 64-bit).
fn parse_hex_address(s: &str) -> Option<u64> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    u64::from_str_radix(s, 16).ok()
}

/// String pointer replacement pass
///
/// Replaces address literals that match known strings in string_map.
pub struct ReplaceStringPointersPass;

impl PostProcessPass for ReplaceStringPointersPass {
    fn metadata(&self) -> PassMetadata {
        PassMetadata {
            id: "replace_string_pointers",
            name: "Replace String Pointers",
            description: "Replaces address literals with actual string content from .rdata/.rodata",
            category: PassCategory::TypeBased,
        }
    }

    fn run<'a>(&self, code: &'a str, context: &super::pass::PassContext) -> PassResult<'a> {
        let string_map = match &context.string_map {
            Some(m) if !m.is_empty() => m,
            _ => return Ok(Cow::Borrowed(code)),
        };

        let mut result = code.to_string();
        let mut changed = false;

        // Order matters: match more specific patterns first.
        // Each pattern: (regex, capture_group_index_for_hex)
        let pattern_specs: &[(&str, usize)] = &[
            (r"&DAT_0x([0-9a-fA-F]+)", 1),
            (r"&DAT_([0-9a-fA-F]+)", 1),
            (
                r"\(char\s*\*\)\s*\(\s*\(?\s*long\s*long\s*\)?\s*&\s*DAT_0x([0-9a-fA-F]+)\s*\+\s*[0-9]+\s*\)",
                1,
            ),
            (
                r"\(char\s*\*\)\s*\(\s*\(?\s*long\s*long\s*\)?\s*&\s*DAT_([0-9a-fA-F]+)\s*\+\s*[0-9]+\s*\)",
                1,
            ),
            (r"\(char\s*\*\)\s*0x([0-9a-fA-F]+)", 1),
            (r"\b0x([0-9a-fA-F]{6,16})\b", 1),
        ];

        for (pattern, cap_idx) in pattern_specs {
            let re = match Regex::new(pattern) {
                Ok(r) => r,
                Err(_) => continue,
            };

            let mut replacements: Vec<(std::ops::Range<usize>, String)> = Vec::new();
            for cap in re.captures_iter(&result) {
                let full_match = cap.get(0).expect("capture 0");
                let hex_part = match cap.get(*cap_idx) {
                    Some(m) => m.as_str(),
                    None => continue,
                };
                let addr = match parse_hex_address(hex_part) {
                    Some(a) => a,
                    None => continue,
                };
                let replacement = match string_map.get(&addr) {
                    Some(s) => format!("\"{}\"", escape_for_c_string(s)),
                    None => continue,
                };
                replacements.push((full_match.range(), replacement));
            }

            for (range, replacement) in replacements.into_iter().rev() {
                result.replace_range(range, &replacement);
                changed = true;
            }
        }

        if changed {
            Ok(Cow::Owned(result))
        } else {
            Ok(Cow::Borrowed(code))
        }
    }

    fn should_run(&self, context: &super::pass::PassContext) -> bool {
        context.string_map.as_ref().map_or(false, |m| !m.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn ctx_with_strings(map: HashMap<u64, String>) -> super::super::pass::PassContext {
        let mut ctx = super::super::pass::PassContext::default();
        ctx.string_map = Some(map);
        ctx
    }

    #[test]
    fn test_escape_basic() {
        assert_eq!(escape_for_c_string("hello"), "hello");
        assert_eq!(escape_for_c_string("a\nb"), "a\\nb");
        assert_eq!(escape_for_c_string("say \"hi\""), "say \\\"hi\\\"");
        assert_eq!(escape_for_c_string("tab\there"), "tab\\there");
    }

    #[test]
    fn test_replace_plain_hex() {
        let mut map = HashMap::new();
        map.insert(0x140004020, "Hello World".to_string());
        let ctx = ctx_with_strings(map);
        let pass = ReplaceStringPointersPass;
        let code = "puts(0x140004020);";
        let result = pass.run(code, &ctx).unwrap();
        assert_eq!(result, "puts(\"Hello World\");");
    }

    #[test]
    fn test_replace_dat_symbol() {
        let mut map = HashMap::new();
        // Input has literal backslash + n (like C "Test\\nString")
        map.insert(0x140004038, "Test\\nString".to_string());
        let ctx = ctx_with_strings(map);
        let pass = ReplaceStringPointersPass;
        let code = "puts(&DAT_0x140004038);";
        let result = pass.run(code, &ctx).unwrap();
        // Output: backslash must be escaped in C → "Test\\\\nString"
        assert_eq!(result, "puts(\"Test\\\\nString\");");
    }

    #[test]
    fn test_replace_char_cast() {
        let mut map = HashMap::new();
        map.insert(0x408000, "Format: %d".to_string());
        let ctx = ctx_with_strings(map);
        let pass = ReplaceStringPointersPass;
        let code = "printf((char *)0x408000, x);";
        let result = pass.run(code, &ctx).unwrap();
        assert_eq!(result, "printf(\"Format: %d\", x);");
    }

    #[test]
    fn test_no_change_when_unknown_addr() {
        let mut map = HashMap::new();
        map.insert(0x140004020, "Hello".to_string());
        let ctx = ctx_with_strings(map);
        let pass = ReplaceStringPointersPass;
        let code = "puts(0x999999);";
        let result = pass.run(code, &ctx).unwrap();
        assert_eq!(result, "puts(0x999999);");
    }

    #[test]
    fn test_empty_map_no_change() {
        let ctx = ctx_with_strings(HashMap::new());
        let pass = ReplaceStringPointersPass;
        let code = "puts(0x140004020);";
        let result = pass.run(code, &ctx).unwrap();
        assert!(matches!(result, Cow::Borrowed(_)));
    }
}
