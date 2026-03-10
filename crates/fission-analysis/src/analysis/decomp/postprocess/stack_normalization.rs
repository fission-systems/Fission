use super::PostProcessor;
use crate::utils::patterns::IDENTIFIER;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

static STACK_VAR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?P<name>(?:_?[A-Za-z]+Stack_[0-9A-Fa-f]+))\b")
        .unwrap_or_else(|e| panic!("invalid STACK_VAR regex: {e}"))
});

static STACK_PIECE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"\b(?P<var>(?:local_[0-9A-Za-z_]+|_?[A-Za-z]+Stack_[0-9A-Fa-f]+))\._(?P<offset>\d+)_(?P<size>\d+)_",
    )
    .unwrap_or_else(|e| panic!("invalid STACK_PIECE regex: {e}"))
});

impl PostProcessor {
    pub(super) fn normalize_stack_artifacts_cow<'a>(code: &'a str) -> Cow<'a, str> {
        if !code.contains("Stack_") && !code.contains("._") {
            return Cow::Borrowed(code);
        }

        let renamed = rename_stack_locals(code);
        let rewritten = rewrite_stack_piece_accesses(&renamed);

        if rewritten == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(rewritten)
        }
    }

    pub(super) fn normalize_stack_artifacts(code: &str) -> String {
        Self::normalize_stack_artifacts_cow(code).into_owned()
    }
}

fn rename_stack_locals(code: &str) -> String {
    if !code.contains("Stack_") {
        return code.to_string();
    }

    let mut used: HashSet<String> = IDENTIFIER
        .find_iter(code)
        .map(|m| m.as_str().to_string())
        .collect();
    let mut mapping: HashMap<String, String> = HashMap::new();

    for caps in STACK_VAR.captures_iter(code) {
        let Some(name) = caps.name("name").map(|m| m.as_str()) else {
            continue;
        };
        if mapping.contains_key(name) {
            continue;
        }

        let Some((_, suffix)) = name.rsplit_once("Stack_") else {
            continue;
        };
        let base = format!("local_{}", suffix.to_ascii_lowercase());
        let mut candidate = base.clone();
        let mut n = 2u32;
        while used.contains(&candidate) && candidate != name {
            candidate = format!("{base}_{n}");
            n += 1;
        }
        used.insert(candidate.clone());
        mapping.insert(name.to_string(), candidate);
    }

    if mapping.is_empty() {
        return code.to_string();
    }

    let mut result = code.to_string();
    for (old, new) in mapping {
        let pat = format!(r"\b{}\b", regex::escape(&old));
        if let Ok(re) = Regex::new(&pat) {
            result = re.replace_all(&result, new.as_str()).to_string();
        }
    }
    result
}

fn rewrite_stack_piece_accesses(code: &str) -> String {
    if !code.contains("._") {
        return code.to_string();
    }

    STACK_PIECE
        .replace_all(code, |caps: &regex::Captures| {
            let var = caps.name("var").map(|m| m.as_str()).unwrap_or_default();
            let offset = caps
                .name("offset")
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .unwrap_or(0);
            let size = caps
                .name("size")
                .and_then(|m| m.as_str().parse::<u32>().ok())
                .unwrap_or(0);

            let Some(ty) = stack_piece_type(size) else {
                return caps[0].to_string();
            };

            if offset == 0 {
                format!("*({ty} *)&{var}")
            } else {
                format!("*({ty} *)((uint8_t *)&{var} + {offset})")
            }
        })
        .to_string()
}

fn stack_piece_type(size: u32) -> Option<&'static str> {
    match size {
        1 => Some("uint8_t"),
        2 => Some("uint16_t"),
        4 => Some("uint32_t"),
        8 => Some("uint64_t"),
        _ => None,
    }
}
