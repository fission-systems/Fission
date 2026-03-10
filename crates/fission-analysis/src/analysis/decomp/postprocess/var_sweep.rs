use super::PostProcessor;
use once_cell::sync::Lazy;
use regex::Regex;
use std::borrow::Cow;

static TEMP_ASSIGN_LINE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^(?P<indent>\s*)(?:(?:const\s+)?[A-Za-z_][A-Za-z0-9_\s\*\[\]]*?\s+)?(?P<name>(?:temp_[A-Za-z0-9_]+|_tmp[A-Za-z0-9_]*|tmp[A-Za-z0-9_]*|uVar[A-Za-z0-9_]*|iVar[A-Za-z0-9_]*|xVar[A-Za-z0-9_]*|bVar[A-Za-z0-9_]*))\s*=\s*(?P<expr>[^;]+);\s*$",
    )
    .unwrap_or_else(|e| panic!("invalid TEMP_ASSIGN_LINE regex: {e}"))
});

static LABEL_LINE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*[A-Za-z_][A-Za-z0-9_]*:\s*$")
        .unwrap_or_else(|e| panic!("invalid LABEL_LINE regex: {e}"))
});

static CALL_LIKE_EXPR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b[A-Za-z_][A-Za-z0-9_]*\s*\(")
        .unwrap_or_else(|e| panic!("invalid CALL_LIKE_EXPR regex: {e}"))
});

static SIMPLE_INLINE_EXPR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(?:[A-Za-z_][A-Za-z0-9_]*|0x[0-9A-Fa-f]+|\d+|true|false|null)$")
        .unwrap_or_else(|e| panic!("invalid SIMPLE_INLINE_EXPR regex: {e}"))
});

impl PostProcessor {
    pub(super) fn inline_single_use_temps_cow<'a>(code: &'a str) -> Cow<'a, str> {
        if !code.lines().any(|line| TEMP_ASSIGN_LINE.is_match(line)) {
            return Cow::Borrowed(code);
        }

        let mut lines: Vec<String> = code.lines().map(str::to_string).collect();
        let mut changed = false;

        for _ in 0..4 {
            let (next, pass_changed) = inline_single_use_temps_once(&lines);
            if !pass_changed {
                break;
            }
            lines = next;
            changed = true;
        }

        if changed {
            Cow::Owned(lines.join("\n"))
        } else {
            Cow::Borrowed(code)
        }
    }

    pub(super) fn inline_single_use_temps(code: &str) -> String {
        Self::inline_single_use_temps_cow(code).into_owned()
    }
}

fn inline_single_use_temps_once(lines: &[String]) -> (Vec<String>, bool) {
    let mut result = Vec::with_capacity(lines.len());
    let mut i = 0;
    let mut changed = false;

    while i < lines.len() {
        let line = &lines[i];
        if let Some(caps) = TEMP_ASSIGN_LINE.captures(line) {
            let name = caps.name("name").map(|m| m.as_str()).unwrap_or_default();
            let expr = caps.name("expr").map(|m| m.as_str()).unwrap_or_default().trim();

            if is_safe_inline_expr(expr)
                && count_identifier_occurrences(&lines[i + 1..], name) == 1
                && let Some(j) = first_non_empty_line_after(lines, i + 1)
                && count_identifier_occurrences(std::slice::from_ref(&lines[j]), name) == 1
                && can_inline_into_line(&lines[j], name)
            {
                let replacement = format_inline_expr(expr);
                let rewritten = replace_identifier_once(&lines[j], name, &replacement);
                if rewritten != lines[j] {
                    for line in lines.iter().take(j).skip(i + 1) {
                        result.push(line.clone());
                    }
                    result.push(rewritten);
                    i = j + 1;
                    changed = true;
                    continue;
                }
            }
        }

        result.push(line.clone());
        i += 1;
    }

    (result, changed)
}

fn first_non_empty_line_after(lines: &[String], start: usize) -> Option<usize> {
    lines
        .iter()
        .enumerate()
        .skip(start)
        .find_map(|(idx, line)| (!line.trim().is_empty()).then_some(idx))
}

fn can_inline_into_line(line: &str, name: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty()
        || LABEL_LINE.is_match(trimmed)
        || trimmed.starts_with("case ")
        || trimmed == "default:"
        || trimmed.starts_with("goto ")
        || trimmed.starts_with("break;")
        || trimmed.starts_with("continue;")
    {
        return false;
    }

    let lhs = trimmed.split('=').next().unwrap_or_default().trim();
    lhs != name && !lhs.ends_with(&format!(" {}", name))
}

fn is_safe_inline_expr(expr: &str) -> bool {
    if expr.is_empty()
        || expr.contains('{')
        || expr.contains('}')
        || expr.contains(';')
        || expr.contains('?')
        || expr.contains("++")
        || expr.contains("--")
        || expr.contains(',')
        || CALL_LIKE_EXPR.is_match(expr)
    {
        return false;
    }

    let bytes = expr.as_bytes();
    for (idx, ch) in bytes.iter().enumerate() {
        if *ch == b'=' {
            let prev = idx.checked_sub(1).and_then(|i| bytes.get(i)).copied();
            let next = bytes.get(idx + 1).copied();
            let comparison = matches!(prev, Some(b'!') | Some(b'<') | Some(b'>') | Some(b'='))
                || matches!(next, Some(b'='));
            if !comparison {
                return false;
            }
        }
    }

    true
}

fn format_inline_expr(expr: &str) -> String {
    let trimmed = expr.trim();
    if SIMPLE_INLINE_EXPR.is_match(trimmed)
        || trimmed.starts_with('(')
        || trimmed.starts_with('*')
        || trimmed.starts_with('&')
        || trimmed.starts_with('!')
        || trimmed.starts_with('-')
    {
        trimmed.to_string()
    } else {
        format!("({trimmed})")
    }
}

fn count_identifier_occurrences(lines: &[String], name: &str) -> usize {
    lines.iter()
        .map(|line| count_identifier_occurrences_in_str(line, name))
        .sum()
}

fn count_identifier_occurrences_in_str(s: &str, name: &str) -> usize {
    let mut count = 0;
    let bytes = s.as_bytes();
    let needle = name.as_bytes();
    let mut i = 0;

    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle
            && (i == 0 || !is_ident_char(bytes[i - 1] as char))
            && (i + needle.len() == bytes.len() || !is_ident_char(bytes[i + needle.len()] as char))
        {
            count += 1;
            i += needle.len();
        } else {
            i += 1;
        }
    }

    count
}

fn replace_identifier_once(line: &str, name: &str, replacement: &str) -> String {
    let pattern = format!(r"\b{}\b", regex::escape(name));
    Regex::new(&pattern)
        .unwrap_or_else(|e| panic!("invalid temp inline regex for {name}: {e}"))
        .replacen(line, 1, replacement)
        .to_string()
}

fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}
