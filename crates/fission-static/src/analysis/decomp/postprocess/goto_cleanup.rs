use super::PostProcessor;
use super::condition::negate_condition;
use regex::Regex;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

static GOTO_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*goto\s+([A-Za-z_]\w*)\s*;\s*$").expect("valid goto regex"));

static IF_GOTO_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\s*)if\s*\((.+)\)\s*goto\s+([A-Za-z_]\w*)\s*;\s*$").expect("valid if-goto regex")
});

static LABEL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*([A-Za-z_]\w*)\s*:\s*$").expect("valid label regex"));
static IF_BLOCK_OPEN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\s*)if\s*\((.+)\)\s*\{\s*$").expect("valid if-block-open regex")
});
static ELSE_BLOCK_OPEN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\}\s*else\s*\{\s*$").expect("valid else-open regex"));
static BLOCK_CLOSE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*\}\s*$").expect("valid block-close regex"));

fn parse_goto(line: &str) -> Option<&str> {
    GOTO_RE
        .captures(line)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str())
}

fn parse_if_goto(line: &str) -> Option<(&str, &str, &str)> {
    IF_GOTO_RE.captures(line).and_then(|caps| {
        Some((
            caps.get(1)?.as_str(),
            caps.get(2)?.as_str().trim(),
            caps.get(3)?.as_str(),
        ))
    })
}

fn parse_label(line: &str) -> Option<&str> {
    LABEL_RE
        .captures(line)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str())
}

fn parse_if_block_open(line: &str) -> Option<(&str, &str)> {
    IF_BLOCK_OPEN_RE
        .captures(line)
        .and_then(|caps| Some((caps.get(1)?.as_str(), caps.get(2)?.as_str().trim())))
}

fn is_else_block_open(line: &str) -> bool {
    ELSE_BLOCK_OPEN_RE.is_match(line)
}

fn is_block_close(line: &str) -> bool {
    BLOCK_CLOSE_RE.is_match(line)
}

fn is_simple_statement(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty()
        && trimmed.ends_with(';')
        && !trimmed.starts_with("goto ")
        && parse_label(trimmed).is_none()
        && !trimmed.contains('{')
        && !trimmed.contains('}')
}

fn is_terminal_statement(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("return ")
        || trimmed == "return;"
        || trimmed == "break;"
        || trimmed == "continue;"
}

fn indent_of(line: &str) -> &str {
    let trimmed = line.trim_start();
    &line[..line.len() - trimmed.len()]
}

fn count_label_references(lines: &[String]) -> HashMap<String, usize> {
    let mut refs = HashMap::new();
    for line in lines {
        if let Some(label) = parse_goto(line) {
            *refs.entry(label.to_string()).or_insert(0) += 1;
        }
        if let Some((_, _, label)) = parse_if_goto(line) {
            *refs.entry(label.to_string()).or_insert(0) += 1;
        }
    }
    refs
}

fn label_positions(lines: &[String]) -> HashMap<String, usize> {
    lines
        .iter()
        .enumerate()
        .filter_map(|(idx, line)| parse_label(line).map(|label| (label.to_string(), idx)))
        .collect()
}

fn next_label_index(lines: &[String], start: usize) -> usize {
    for (idx, line) in lines.iter().enumerate().skip(start) {
        if parse_label(line).is_some() {
            return idx;
        }
    }
    lines.len()
}

fn next_block_boundary_index(lines: &[String], start: usize) -> usize {
    for (idx, line) in lines.iter().enumerate().skip(start) {
        if parse_label(line).is_some() || line.trim() == "}" {
            return idx;
        }
    }
    lines.len()
}

fn remove_self_fallthrough_gotos(lines: &[String]) -> Vec<String> {
    let mut result = Vec::with_capacity(lines.len());
    let mut idx = 0;
    while idx < lines.len() {
        if idx + 1 < lines.len()
            && let Some(target) = parse_goto(&lines[idx])
            && parse_label(&lines[idx + 1]) == Some(target)
        {
            idx += 1;
            continue;
        }
        result.push(lines[idx].clone());
        idx += 1;
    }
    result
}

fn normalize_braced_if_gotos(lines: &[String]) -> Vec<String> {
    let mut result = Vec::with_capacity(lines.len());
    let mut idx = 0;

    while idx < lines.len() {
        let Some((indent, cond)) = parse_if_block_open(&lines[idx]) else {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        };
        let Some(then_label) = lines.get(idx + 1).and_then(|line| parse_goto(line)) else {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        };

        if lines.get(idx + 2).is_some_and(|line| is_block_close(line)) {
            result.push(format!("{indent}if ({cond}) goto {then_label};"));
            idx += 3;
            continue;
        }

        let Some(true) = lines.get(idx + 2).map(|line| is_else_block_open(line)) else {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        };
        if !true {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        }
        let Some(else_label) = lines.get(idx + 3).and_then(|line| parse_goto(line)) else {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        };
        if !lines.get(idx + 4).is_some_and(|line| is_block_close(line)) {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        }

        result.push(format!("{indent}if ({cond}) goto {then_label};"));
        result.push(format!("{indent}goto {else_label};"));
        idx += 5;
    }

    result
}

fn thread_chained_gotos(lines: &[String]) -> Vec<String> {
    let labels = label_positions(lines);
    let mut redirects: HashMap<String, String> = HashMap::new();

    for (label, &label_pos) in &labels {
        let boundary = next_block_boundary_index(lines, label_pos + 1);
        let body: Vec<&str> = lines[label_pos + 1..boundary]
            .iter()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect();
        if body.len() == 1
            && let Some(target) = parse_goto(body[0])
            && target != label
        {
            redirects.insert(label.clone(), target.to_string());
        }
    }

    if redirects.is_empty() {
        return lines.to_vec();
    }

    fn resolve_target<'a>(
        target: &'a str,
        redirects: &'a HashMap<String, String>,
    ) -> Option<&'a str> {
        let mut current = target;
        let mut seen = HashSet::new();
        while let Some(next) = redirects.get(current) {
            if !seen.insert(current) {
                return None;
            }
            current = next;
        }
        Some(current)
    }

    lines
        .iter()
        .map(|line| {
            if let Some(target) = parse_goto(line)
                && let Some(final_target) = resolve_target(target, &redirects)
                && final_target != target
            {
                return line.replacen(target, final_target, 1);
            }
            if let Some((_, _, target)) = parse_if_goto(line)
                && let Some(final_target) = resolve_target(target, &redirects)
                && final_target != target
            {
                return line.replacen(target, final_target, 1);
            }
            line.clone()
        })
        .collect()
}

fn fold_if_else_gotos(lines: &[String]) -> Vec<String> {
    let labels = label_positions(lines);
    let mut result = Vec::new();
    let mut idx = 0;

    while idx < lines.len() {
        let Some((indent, cond, then_label)) = parse_if_goto(&lines[idx]) else {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        };
        let Some(else_label) = lines.get(idx + 1).and_then(|line| parse_goto(line)) else {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        };
        let Some(&then_label_pos) = labels.get(then_label) else {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        };
        if then_label_pos != idx + 2 {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        }

        let then_goto_idx = next_label_index(lines, then_label_pos + 1).saturating_sub(1);
        let Some(end_label) = lines.get(then_goto_idx).and_then(|line| parse_goto(line)) else {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        };
        let Some(&else_label_pos) = labels.get(else_label) else {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        };
        if else_label_pos != then_goto_idx + 1 {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        }
        let Some(&end_label_pos) = labels.get(end_label) else {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        };
        if end_label_pos <= else_label_pos {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        }

        let then_body = &lines[then_label_pos + 1..then_goto_idx];
        let else_body = &lines[else_label_pos + 1..end_label_pos];
        if then_body.is_empty()
            || else_body.is_empty()
            || then_body.iter().any(|line| parse_label(line).is_some())
            || else_body.iter().any(|line| parse_label(line).is_some())
        {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        }

        result.push(format!("{indent}if ({cond}) {{"));
        for line in then_body {
            result.push(format!("{indent}  {}", line.trim()));
        }
        result.push(format!("{indent}}} else {{"));
        for line in else_body {
            result.push(format!("{indent}  {}", line.trim()));
        }
        result.push(format!("{indent}}}"));
        idx = end_label_pos + 1;
    }

    result
}

fn fold_guarded_if_gotos(lines: &[String]) -> Vec<String> {
    let refs = count_label_references(lines);
    let labels = label_positions(lines);
    let mut result = Vec::new();
    let mut idx = 0;

    while idx < lines.len() {
        let Some((indent, cond, end_label)) = parse_if_goto(&lines[idx]) else {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        };
        if refs.get(end_label).copied().unwrap_or(0) != 1 {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        }
        let Some(&label_pos) = labels.get(end_label) else {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        };
        if label_pos <= idx + 1 {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        }

        let body = &lines[idx + 1..label_pos];
        if body.is_empty() || body.iter().any(|line| parse_label(line).is_some()) {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        }

        result.push(format!("{}if ({}) {{", indent, negate_condition(cond)));
        for line in body {
            result.push(format!("{}  {}", indent, line.trim()));
        }
        result.push(format!("{}}}", indent));
        idx = label_pos + 1;
    }

    result
}

fn merge_adjacent_if_goto_chains(lines: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    let mut idx = 0;

    while idx < lines.len() {
        let Some((indent, cond, target)) = parse_if_goto(&lines[idx]) else {
            result.push(lines[idx].clone());
            idx += 1;
            continue;
        };

        let mut merged_cond = cond.to_string();
        let mut next_idx = idx + 1;
        let mut merged = false;

        while next_idx < lines.len() {
            let Some((next_indent, next_cond, next_target)) = parse_if_goto(&lines[next_idx])
            else {
                break;
            };
            if next_indent != indent || next_target != target {
                break;
            }
            merged_cond = format!("({merged_cond}) || ({next_cond})");
            merged = true;
            next_idx += 1;
        }

        if merged {
            result.push(format!("{indent}if ({merged_cond}) goto {target};"));
            idx = next_idx;
            continue;
        }

        result.push(lines[idx].clone());
        idx += 1;
    }

    result
}

fn sink_label_prefixes(lines: &[String]) -> Vec<String> {
    let labels = label_positions(lines);
    let mut prefixes: HashMap<String, (Vec<String>, String)> = HashMap::new();

    for (label, &label_pos) in &labels {
        let block_end = next_block_boundary_index(lines, label_pos + 1);
        if block_end <= label_pos + 1 || block_end >= lines.len() {
            continue;
        }
        let Some(next_label) = parse_label(&lines[block_end]) else {
            continue;
        };
        let body: Vec<String> = lines[label_pos + 1..block_end]
            .iter()
            .map(|line| line.trim().to_string())
            .filter(|line| !line.is_empty())
            .collect();
        if body.is_empty()
            || body.len() > 2
            || !body
                .iter()
                .all(|line| is_simple_statement(line) && !is_terminal_statement(line))
        {
            continue;
        }
        prefixes.insert(label.clone(), (body, next_label.to_string()));
    }

    if prefixes.is_empty() {
        return lines.to_vec();
    }

    let mut result = Vec::new();
    for line in lines {
        if let Some(target) = parse_goto(line)
            && let Some((body, next_label)) = prefixes.get(target)
        {
            let indent = indent_of(line).to_string();
            result.extend(body.iter().map(|stmt| format!("{indent}{stmt}")));
            result.push(format!("{indent}goto {next_label};"));
            continue;
        }

        if let Some((indent, cond, target)) = parse_if_goto(line)
            && let Some((body, next_label)) = prefixes.get(target)
        {
            result.push(format!("{indent}if ({cond}) {{"));
            result.extend(body.iter().map(|stmt| format!("{indent}  {stmt}")));
            result.push(format!("{indent}  goto {next_label};"));
            result.push(format!("{indent}}}"));
            continue;
        }

        result.push(line.clone());
    }

    result
}

fn inline_single_use_labels(lines: &[String]) -> Vec<String> {
    let refs = count_label_references(lines);
    let labels = label_positions(lines);
    let mut removed_ranges = HashSet::new();
    let mut result = Vec::new();
    let mut idx = 0;

    while idx < lines.len() {
        if removed_ranges.contains(&idx) {
            idx += 1;
            continue;
        }

        if let Some(target) = parse_goto(&lines[idx])
            && refs.get(target).copied().unwrap_or(0) == 1
            && let Some(&label_pos) = labels.get(target)
        {
            let block_end = next_block_boundary_index(lines, label_pos + 1);
            let body: Vec<&String> = lines[label_pos + 1..block_end]
                .iter()
                .filter(|line| !line.trim().is_empty())
                .collect();
            if !body.is_empty()
                && body.len() <= 2
                && body.iter().all(|line| is_simple_statement(line))
            {
                let indent = indent_of(&lines[idx]).to_string();
                for line in body {
                    result.push(format!("{indent}{}", line.trim()));
                }
                for skip in label_pos..block_end {
                    removed_ranges.insert(skip);
                }
                idx += 1;
                continue;
            }
        }

        result.push(lines[idx].clone());
        idx += 1;
    }

    result
}

fn collect_terminal_inline_body(
    label: &str,
    lines: &[String],
    labels: &HashMap<String, usize>,
    cache: &mut HashMap<String, Option<Vec<String>>>,
    visiting: &mut HashSet<String>,
) -> Option<Vec<String>> {
    if let Some(cached) = cache.get(label) {
        return cached.clone();
    }
    if !visiting.insert(label.to_string()) {
        return None;
    }

    const MAX_INLINE_LINES: usize = 4;

    let result = (|| {
        let &label_pos = labels.get(label)?;
        let mut idx = label_pos + 1;
        let mut collected = Vec::new();

        while idx < lines.len() {
            let trimmed = lines[idx].trim();
            if trimmed.is_empty() {
                idx += 1;
                continue;
            }
            if trimmed == "}" {
                break;
            }
            if let Some(next_label) = parse_label(trimmed) {
                if collected.is_empty() || collected.len() >= MAX_INLINE_LINES {
                    return None;
                }
                let mut tail =
                    collect_terminal_inline_body(next_label, lines, labels, cache, visiting)?;
                if collected.len() + tail.len() > MAX_INLINE_LINES {
                    return None;
                }
                collected.append(&mut tail);
                return Some(collected);
            }
            if !is_simple_statement(trimmed) {
                return None;
            }
            collected.push(trimmed.to_string());
            if collected.len() > MAX_INLINE_LINES {
                return None;
            }
            if is_terminal_statement(trimmed) {
                return Some(collected);
            }
            idx += 1;
        }

        None
    })();

    visiting.remove(label);
    cache.insert(label.to_string(), result.clone());
    result
}

fn inline_terminal_label_blocks(lines: &[String]) -> Vec<String> {
    let labels = label_positions(lines);
    let mut cache: HashMap<String, Option<Vec<String>>> = HashMap::new();
    let mut result = Vec::new();

    for line in lines {
        if let Some(target) = parse_goto(line)
            && let Some(body) = collect_terminal_inline_body(
                target,
                lines,
                &labels,
                &mut cache,
                &mut HashSet::new(),
            )
        {
            let indent = indent_of(line).to_string();
            result.extend(body.into_iter().map(|stmt| format!("{indent}{stmt}")));
            continue;
        }

        if let Some((indent, cond, target)) = parse_if_goto(line)
            && let Some(body) = collect_terminal_inline_body(
                target,
                lines,
                &labels,
                &mut cache,
                &mut HashSet::new(),
            )
        {
            result.push(format!("{indent}if ({cond}) {{"));
            result.extend(body.into_iter().map(|stmt| format!("{indent}  {stmt}")));
            result.push(format!("{indent}}}"));
            continue;
        }

        result.push(line.clone());
    }

    result
}

fn remove_dead_labels(lines: &[String]) -> Vec<String> {
    let refs = count_label_references(lines);
    lines
        .iter()
        .filter(|line| {
            parse_label(line)
                .map(|label| refs.get(label).copied().unwrap_or(0) > 0)
                .unwrap_or(true)
        })
        .cloned()
        .collect()
}

impl PostProcessor {
    pub(super) fn cleanup_gotos_cow<'a>(code: &'a str) -> Cow<'a, str> {
        if !code.contains("goto ") {
            return Cow::Borrowed(code);
        }

        let mut current = code.to_string();

        for _ in 0..3 {
            let lines: Vec<String> = current.lines().map(str::to_string).collect();
            let next =
                remove_dead_labels(&inline_single_use_labels(&inline_terminal_label_blocks(
                    &sink_label_prefixes(&merge_adjacent_if_goto_chains(&fold_guarded_if_gotos(
                        &fold_if_else_gotos(&thread_chained_gotos(&remove_self_fallthrough_gotos(
                            &normalize_braced_if_gotos(&lines),
                        ))),
                    ))),
                )))
                .join("\n");
            if next == current {
                break;
            }
            current = next;
        }

        if current == code {
            Cow::Borrowed(code)
        } else {
            Cow::Owned(current)
        }
    }

    pub(super) fn cleanup_gotos(code: &str) -> String {
        Self::cleanup_gotos_cow(code).into_owned()
    }
}
