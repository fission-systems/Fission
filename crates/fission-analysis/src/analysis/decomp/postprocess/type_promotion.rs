use super::PostProcessor;
use fission_signatures::win_types::WindowsStructures;
use fission_signatures::WIN_API_DB;
use regex::{Captures, Regex};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

static ARRAY_PARAM_DECL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"uint8_t\s*\(\*\s*(param_\d+)\s*\)\s*\[\s*(\d+)\s*\]")
        .expect("valid struct param regex")
});

#[derive(Debug, Clone)]
struct StructPromotionRule {
    function_name: String,
    param_index: usize,
    pointer_type: String,
    struct_name: String,
    sizes: Vec<usize>,
}

#[derive(Debug, Clone)]
struct StructTypeInfo {
    struct_name: String,
    sizes: Vec<usize>,
}

#[derive(Debug, Clone)]
pub(super) struct PromotedStructParam {
    pub param_name: String,
    pub pointer_type: String,
    pub struct_name: String,
    pub size: usize,
}

static STRUCT_PROMOTION_RULES: LazyLock<Vec<StructPromotionRule>> =
    LazyLock::new(build_struct_promotion_rules);

static POINTER_STRUCT_TYPES: LazyLock<HashMap<String, StructTypeInfo>> =
    LazyLock::new(build_pointer_struct_types);

fn build_struct_promotion_rules() -> Vec<StructPromotionRule> {
    let structures = WindowsStructures::new();
    let mut rules = Vec::new();

    for sig in WIN_API_DB.iter() {
        for (param_index, param) in sig.params.iter().enumerate() {
            let Some(struct_name) = resolve_struct_name(&param.type_name, &structures) else {
                continue;
            };
            let Some(struct_def) = structures.get(&struct_name) else {
                continue;
            };
            let sizes = unique_sizes([struct_def.size_32, struct_def.size_64]);
            if sizes.is_empty() {
                continue;
            }
            rules.push(StructPromotionRule {
                function_name: sig.name.clone(),
                param_index,
                pointer_type: param.type_name.clone(),
                struct_name,
                sizes,
            });
        }
    }

    rules
}

fn build_pointer_struct_types() -> HashMap<String, StructTypeInfo> {
    let mut types = HashMap::new();
    for rule in STRUCT_PROMOTION_RULES.iter() {
        types
            .entry(rule.pointer_type.clone())
            .and_modify(|entry: &mut StructTypeInfo| {
                entry.sizes = merge_sizes(&entry.sizes, &rule.sizes);
            })
            .or_insert_with(|| StructTypeInfo {
                struct_name: rule.struct_name.clone(),
                sizes: rule.sizes.clone(),
            });
    }
    types
}

fn resolve_struct_name(type_name: &str, structures: &WindowsStructures) -> Option<String> {
    if type_name.contains('*') {
        return None;
    }

    for prefix in ["LP", "P"] {
        let Some(candidate) = type_name.strip_prefix(prefix) else {
            continue;
        };
        if structures.get(candidate).is_some() {
            return Some(candidate.to_string());
        }
    }

    None
}

fn unique_sizes<const N: usize>(sizes: [usize; N]) -> Vec<usize> {
    let mut seen = HashSet::new();
    let mut ordered = Vec::new();
    for size in sizes {
        if size == 0 || !seen.insert(size) {
            continue;
        }
        ordered.push(size);
    }
    ordered
}

fn merge_sizes(existing: &[usize], incoming: &[usize]) -> Vec<usize> {
    let mut merged = existing.to_vec();
    for size in incoming {
        if !merged.contains(size) {
            merged.push(*size);
        }
    }
    merged
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn extract_call_args<'a>(code: &'a str, function_name: &str) -> Vec<&'a str> {
    let mut calls = Vec::new();
    let bytes = code.as_bytes();
    let mut search_from = 0usize;

    while let Some(relative) = code[search_from..].find(function_name) {
        let start = search_from + relative;
        let end = start + function_name.len();

        let valid_prefix = start == 0 || !is_ident_byte(bytes[start - 1]);
        let valid_suffix = end >= bytes.len() || !is_ident_byte(bytes[end]);
        if !valid_prefix || !valid_suffix {
            search_from = end;
            continue;
        }

        let mut open = end;
        while open < bytes.len() && bytes[open].is_ascii_whitespace() {
            open += 1;
        }
        if open >= bytes.len() || bytes[open] != b'(' {
            search_from = end;
            continue;
        }

        let mut depth = 1usize;
        let mut cursor = open + 1;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        calls.push(&code[open + 1..cursor]);
                        break;
                    }
                }
                _ => {}
            }
            cursor += 1;
        }

        search_from = end;
    }

    calls
}

fn split_top_level_args(args: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut paren = 0usize;
    let mut bracket = 0usize;
    let mut brace = 0usize;

    for (idx, ch) in args.char_indices() {
        match ch {
            '(' => paren += 1,
            ')' => paren = paren.saturating_sub(1),
            '[' => bracket += 1,
            ']' => bracket = bracket.saturating_sub(1),
            '{' => brace += 1,
            '}' => brace = brace.saturating_sub(1),
            ',' if paren == 0 && bracket == 0 && brace == 0 => {
                parts.push(args[start..idx].trim());
                start = idx + 1;
            }
            _ => {}
        }
    }

    let tail = args[start..].trim();
    if !tail.is_empty() {
        parts.push(tail);
    }

    parts
}

fn argument_mentions_param(arg: &str, param_name: &str) -> bool {
    let bytes = arg.as_bytes();
    let mut search_from = 0usize;

    while let Some(relative) = arg[search_from..].find(param_name) {
        let start = search_from + relative;
        let end = start + param_name.len();
        let valid_prefix = start == 0 || !is_ident_byte(bytes[start - 1]);
        let valid_suffix = end >= bytes.len() || !is_ident_byte(bytes[end]);
        if valid_prefix && valid_suffix {
            return true;
        }
        search_from = end;
    }

    false
}

fn raw_whole_object_write_re(param_name: &str) -> Regex {
    Regex::new(&format!(
        r"\*\s*{}\s*=\s*CONCAT0?\d+\s*\(",
        regex::escape(param_name)
    ))
    .expect("valid whole-object write regex")
}

fn whole_object_assign_re(param_name: &str) -> Regex {
    Regex::new(&format!(
        r"\*\s*{}\s*=\s*(CONCAT0?\d+\s*\([^;]+\));",
        regex::escape(param_name)
    ))
    .expect("valid whole-object assign regex")
}

fn param_decl_re(param_name: &str, size: usize) -> Regex {
    Regex::new(&format!(
        r"uint8_t\s*\(\*\s*{}\s*\)\s*\[\s*{}\s*\]",
        regex::escape(param_name),
        size
    ))
    .expect("valid specific param decl regex")
}

fn promoted_param_decl_re(pointer_type: &str) -> Regex {
    Regex::new(&format!(
        r"\b{}\s+(param_\d+)\b",
        regex::escape(pointer_type)
    ))
    .expect("valid promoted param decl regex")
}

fn call_uses_param_at_index(
    code: &str,
    function_name: &str,
    param_index: usize,
    param_name: &str,
) -> bool {
    extract_call_args(code, function_name)
        .into_iter()
        .map(split_top_level_args)
        .any(|args| {
            args.get(param_index)
                .is_some_and(|arg| argument_mentions_param(arg, param_name))
        })
}

fn find_struct_promotion_candidates(code: &str) -> Vec<PromotedStructParam> {
    let mut promoted = Vec::new();
    let mut seen = HashSet::new();

    for captures in ARRAY_PARAM_DECL_RE.captures_iter(code) {
        let Some(param_name) = captures.get(1).map(|m| m.as_str()) else {
            continue;
        };
        let Some(size) = captures
            .get(2)
            .and_then(|m| m.as_str().parse::<usize>().ok())
        else {
            continue;
        };
        if !raw_whole_object_write_re(param_name).is_match(code) {
            continue;
        }

        let Some(rule) = STRUCT_PROMOTION_RULES.iter().find(|rule| {
            rule.sizes.contains(&size)
                && call_uses_param_at_index(code, &rule.function_name, rule.param_index, param_name)
        }) else {
            continue;
        };

        if !seen.insert(param_name.to_string()) {
            continue;
        }

        promoted.push(PromotedStructParam {
            param_name: param_name.to_string(),
            pointer_type: rule.pointer_type.clone(),
            struct_name: rule.struct_name.clone(),
            size,
        });
    }

    promoted
}

pub(super) fn lookup_promoted_struct_param(
    code: &str,
    param_name: &str,
    size: usize,
) -> Option<PromotedStructParam> {
    for (pointer_type, info) in POINTER_STRUCT_TYPES.iter() {
        if !info.sizes.contains(&size) {
            continue;
        }
        let decl_re = promoted_param_decl_re(pointer_type);
        if !decl_re.captures_iter(code).any(|caps| {
            caps.get(1).map(|m| m.as_str()) == Some(param_name)
        }) {
            continue;
        }
        return Some(PromotedStructParam {
            param_name: param_name.to_string(),
            pointer_type: pointer_type.clone(),
            struct_name: info.struct_name.clone(),
            size,
        });
    }

    None
}

impl PostProcessor {
    pub(super) fn promote_rect_params_cow<'a>(code: &'a str) -> Cow<'a, str> {
        let promoted_params = find_struct_promotion_candidates(code);

        if promoted_params.is_empty() {
            return Cow::Borrowed(code);
        }

        let mut rewritten = code.to_string();
        for promoted in promoted_params {
            rewritten = param_decl_re(&promoted.param_name, promoted.size)
                .replace_all(&rewritten, |caps: &Captures| {
                    if caps.get(0).is_some() {
                        format!("{} {}", promoted.pointer_type, promoted.param_name)
                    } else {
                        caps.get(0).map_or("", |m| m.as_str()).to_string()
                    }
                })
                .into_owned();

            let assign_re = whole_object_assign_re(&promoted.param_name);
            rewritten = assign_re
                .replace_all(&rewritten, |caps: &Captures| {
                    let rhs = caps.get(1).map_or("", |m| m.as_str());
                    format!(
                        "*(uint8_t (*)[{}]){} = {};",
                        promoted.size, promoted.param_name, rhs
                    )
                })
                .into_owned();
        }

        if rewritten == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(rewritten)
        }
    }

    pub(super) fn promote_rect_params(code: &str) -> String {
        Self::promote_rect_params_cow(code).into_owned()
    }
}
