//! Minimal SLEIGH `.slaspec`/`.sinc` preprocessor and `define register` extractor.
//!
//! Used to build [`RegisterModel`](super::register_model::RegisterModel) from checked-in
//! `utils/sleigh-specs` without a Ghidra install or compiled `.sla` artifacts.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use super::slaspec_cache;

/// One hardware register entry extracted from a `define register` statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRegister {
    pub name: String,
    pub offset: u64,
    pub size: u32,
    /// Slot index within the `define register [...]` name list (including `_` gaps).
    pub slot_index: usize,
    /// Base offset from the `define register offset=...` header.
    pub base_offset: u64,
}

/// Expand a `.slaspec` entry file and extract all register definitions.
///
/// The result depends only on the bytes of the files the expansion reads, so it
/// is cached on disk keyed by their content; see [`slaspec_cache`].
pub fn parse_registers_from_slaspec(entry_spec: &Path) -> Result<Vec<ParsedRegister>> {
    parse_registers_cached(entry_spec, slaspec_cache::default_cache_dir().as_deref())
}

pub(crate) fn parse_registers_cached(
    entry_spec: &Path,
    cache_dir: Option<&Path>,
) -> Result<Vec<ParsedRegister>> {
    let cache_path = cache_dir.map(|dir| slaspec_cache::entry_path(dir, entry_spec));
    if let Some(registers) = cache_path.as_deref().and_then(slaspec_cache::load) {
        return Ok(registers);
    }
    let expanded = expand_entry_spec(entry_spec)?;
    let registers = extract_define_registers(&expanded)?;
    if let Some(path) = cache_path.as_deref() {
        slaspec_cache::store(path, &expanded.sources, &registers);
    }
    Ok(registers)
}

struct ExpandedSpec {
    defines: BTreeMap<String, String>,
    lines: Vec<String>,
    /// Every file read, in read order, with the hash of the bytes parsed.
    sources: Vec<(PathBuf, u64)>,
}

fn expand_entry_spec(entry_spec: &Path) -> Result<ExpandedSpec> {
    let root_dir = entry_spec
        .parent()
        .ok_or_else(|| anyhow!("entry spec has no parent: {}", entry_spec.display()))?
        .to_path_buf();
    let mut state = PreprocessorState {
        root_dir,
        defines: BTreeMap::new(),
        lines: Vec::new(),
        seen_files: HashSet::default(),
        sources: Vec::new(),
    };
    state.expand_file(entry_spec)?;
    Ok(ExpandedSpec {
        defines: state.defines,
        lines: state.lines,
        sources: state.sources,
    })
}

struct PreprocessorState {
    root_dir: PathBuf,
    defines: BTreeMap<String, String>,
    lines: Vec<String>,
    seen_files: HashSet<PathBuf>,
    sources: Vec<(PathBuf, u64)>,
}

#[derive(Clone, Copy)]
struct ConditionalFrame {
    parent_active: bool,
    branch_taken: bool,
    current_active: bool,
}

impl PreprocessorState {
    fn expand_file(&mut self, path: &Path) -> Result<()> {
        let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if !self.seen_files.insert(canonical.clone()) {
            return Ok(());
        }
        let source = fs::read_to_string(path)
            .with_context(|| format!("read sleigh source {}", path.display()))?;
        self.sources
            .push((canonical, slaspec_cache::content_hash(source.as_bytes())));
        let mut conditionals: Vec<ConditionalFrame> = Vec::new();
        let mut with_depth: u32 = 0;
        let mut inner_brace_depth: u32 = 0;

        for raw_line in source.lines() {
            let trimmed = strip_comment(raw_line).trim();
            if trimmed.is_empty() {
                continue;
            }

            if trimmed.strip_prefix("with").is_some_and(|rest| {
                rest.starts_with(|c: char| c.is_ascii_whitespace() || c == ':' || c == '{')
            }) {
                if let Some(open) = trimmed.find('{') {
                    with_depth = with_depth.saturating_add(1);
                    let mut inner = trimmed[open + 1..].trim();
                    let (opens, closes) = count_braces(inner);
                    let mut total_depth = inner_brace_depth.saturating_add(opens as u32);
                    for _ in 0..closes {
                        if total_depth > inner_brace_depth {
                            total_depth -= 1;
                        } else if with_depth > 0 {
                            with_depth -= 1;
                            if inner.ends_with('}') {
                                inner = inner[..inner.len() - 1].trim_end();
                            }
                        }
                    }
                    inner_brace_depth = total_depth;
                    if !inner.is_empty() {
                        self.handle_content_line(inner, path, &mut conditionals)?;
                    }
                }
                continue;
            }
            if with_depth > 0 {
                let mut content = trimmed;
                let (opens, closes) = count_braces(content);
                for _ in 0..opens {
                    inner_brace_depth = inner_brace_depth.saturating_add(1);
                }
                for _ in 0..closes {
                    if inner_brace_depth > 0 {
                        inner_brace_depth -= 1;
                    } else if with_depth > 0 {
                        with_depth -= 1;
                        if content.ends_with('}') {
                            content = content[..content.len() - 1].trim_end();
                        }
                    }
                }
                if content.is_empty() {
                    continue;
                }
                self.handle_content_line(content, path, &mut conditionals)?;
                continue;
            }

            self.handle_content_line(trimmed, path, &mut conditionals)?;
        }

        // Same rule as `fission-sleigh`'s preprocessor and Ghidra's: the
        // conditional stack is per file and an `@if` still open at EOF is not
        // an error (upstream `ARM/ARMv8.sinc` has one).
        Ok(())
    }

    fn handle_content_line(
        &mut self,
        trimmed: &str,
        current_file: &Path,
        conditionals: &mut Vec<ConditionalFrame>,
    ) -> Result<()> {
        let is_active = conditionals.iter().all(|frame| frame.current_active);

        if trimmed.starts_with("@define ") {
            if is_active {
                let rest = trimmed["@define ".len()..].trim();
                let (name, value) = parse_define_directive(rest)?;
                self.defines.insert(name, value);
            }
            return Ok(());
        }
        if trimmed.starts_with("@include ") {
            if is_active {
                let include_path = trimmed["@include ".len()..]
                    .trim()
                    .trim_matches('"')
                    .to_string();
                let resolved = current_file
                    .parent()
                    .ok_or_else(|| {
                        anyhow!("include parent missing for {}", current_file.display())
                    })?
                    .join(include_path);
                self.expand_file(&resolved)?;
            }
            return Ok(());
        }
        if let Some(rest) = trimmed.strip_prefix("@ifdef ") {
            let name = rest.trim().to_string();
            let defined = self.defines.contains_key(&name);
            let parent_active = conditionals.iter().all(|frame| frame.current_active);
            conditionals.push(ConditionalFrame {
                parent_active,
                branch_taken: defined,
                current_active: parent_active && defined,
            });
            return Ok(());
        }
        if let Some(rest) = trimmed.strip_prefix("@ifndef ") {
            let name = rest.trim().to_string();
            let defined = self.defines.contains_key(&name);
            let parent_active = conditionals.iter().all(|frame| frame.current_active);
            conditionals.push(ConditionalFrame {
                parent_active,
                branch_taken: !defined,
                current_active: parent_active && !defined,
            });
            return Ok(());
        }
        if trimmed.starts_with("@if ") {
            let predicate = evaluate_if_expression(trimmed["@if ".len()..].trim(), &self.defines);
            let parent_active = conditionals.iter().all(|frame| frame.current_active);
            conditionals.push(ConditionalFrame {
                parent_active,
                branch_taken: predicate,
                current_active: parent_active && predicate,
            });
            return Ok(());
        }
        if trimmed.starts_with("@elif ") {
            let predicate = evaluate_if_expression(trimmed["@elif ".len()..].trim(), &self.defines);
            let Some(last) = conditionals.last_mut() else {
                bail!("@elif without @if in {}", current_file.display());
            };
            if last.branch_taken {
                last.current_active = false;
            } else {
                last.current_active = last.parent_active && predicate;
                last.branch_taken = predicate;
            }
            return Ok(());
        }
        if trimmed == "@else" {
            let Some(last) = conditionals.last_mut() else {
                bail!("@else without @if in {}", current_file.display());
            };
            let new_active = last.parent_active && !last.branch_taken;
            last.branch_taken = last.branch_taken || new_active;
            last.current_active = new_active;
            return Ok(());
        }
        if trimmed == "@endif" {
            conditionals
                .pop()
                .ok_or_else(|| anyhow!("@endif without @if in {}", current_file.display()))?;
            return Ok(());
        }

        if is_active && !trimmed.starts_with('@') {
            self.lines.push(substitute_macros(trimmed, &self.defines));
        }
        Ok(())
    }
}

fn parse_define_directive(rest: &str) -> Result<(String, String)> {
    let mut parts = rest.split_whitespace();
    let name = parts
        .next()
        .ok_or_else(|| anyhow!("missing define name"))?
        .to_string();
    let value = parts
        .next()
        .map(|v| v.trim_matches('"').to_string())
        .unwrap_or_default();
    Ok((name, value))
}

/// Evaluate a `@if`/`@elif` condition.
///
/// The grammar the checked-in specs actually use: `defined(NAME)`,
/// `NAME == "value"`, `NAME != "value"`, joined by `||` and `&&`, with
/// parentheses. Splitting on the first `==` and otherwise testing the whole
/// string as a macro name -- which is what this did -- made every condition
/// containing `defined(` false, since `"defined(VFPv2) || defined(VFPv3)"` is
/// not the name of any macro. That silently dropped the guarded blocks: ARM's
/// VFP and NEON register banks live behind exactly that condition, so `s0`,
/// `d0`, `q0` and `fpscr` were absent from every ARM register model and any
/// function touching a float lost the value entirely.
fn evaluate_if_expression(expr: &str, defines: &BTreeMap<String, String>) -> bool {
    let expr = expr.trim();
    if expr.is_empty() {
        return false;
    }
    // `||` binds loosest, then `&&`, so they are peeled in that order and
    // only at paren depth zero.
    if let Some(parts) = split_top_level(expr, "||") {
        return parts
            .iter()
            .any(|part| evaluate_if_expression(part, defines));
    }
    if let Some(parts) = split_top_level(expr, "&&") {
        return parts
            .iter()
            .all(|part| evaluate_if_expression(part, defines));
    }
    if let Some(inner) = strip_enclosing_parens(expr) {
        return evaluate_if_expression(inner, defines);
    }
    if let Some(name) = expr
        .strip_prefix("defined(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        return defines.contains_key(name.trim());
    }
    if let Some((lhs, rhs)) = expr.split_once("==") {
        let left_val = defines.get(lhs.trim()).map(String::as_str).unwrap_or("");
        return left_val == rhs.trim().trim_matches('"');
    }
    if let Some((lhs, rhs)) = expr.split_once("!=") {
        let left_val = defines.get(lhs.trim()).map(String::as_str).unwrap_or("");
        return left_val != rhs.trim().trim_matches('"');
    }
    defines.contains_key(expr)
}

/// Split on `separator` where it sits outside every parenthesis, or `None`
/// when it does not occur there -- so `(A == "1") || (B == "2")` splits and
/// `((A == "1") || (B == "2"))` does not, leaving its outer parens to be
/// stripped first.
fn split_top_level<'a>(expr: &'a str, separator: &str) -> Option<Vec<&'a str>> {
    let bytes = expr.as_bytes();
    let mut parts = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && expr[index..].starts_with(separator) {
            parts.push(expr[start..index].trim());
            index += separator.len();
            start = index;
            continue;
        }
        index += 1;
    }
    if parts.is_empty() {
        return None;
    }
    parts.push(expr[start..].trim());
    Some(parts)
}

/// The inside of `( ... )` when the whole expression is one parenthesised
/// group, rather than two adjacent ones.
fn strip_enclosing_parens(expr: &str) -> Option<&str> {
    let inner = expr.strip_prefix('(')?.strip_suffix(')')?;
    let mut depth = 0i32;
    for byte in inner.bytes() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth < 0 {
                    return None;
                }
            }
            _ => {}
        }
    }
    (depth == 0).then_some(inner.trim())
}

fn count_braces(text: &str) -> (usize, usize) {
    let mut opens = 0usize;
    let mut closes = 0usize;
    for b in text.bytes() {
        match b {
            b'{' => opens += 1,
            b'}' => closes += 1,
            _ => {}
        }
    }
    (opens, closes)
}

fn substitute_macros(text: &str, defines: &BTreeMap<String, String>) -> String {
    // Every replacement needle begins with this prefix. Literal lines need
    // no per-definition allocation/search, regardless of the definition map.
    if !text.contains("$(") || defines.is_empty() {
        return text.to_owned();
    }
    const MAX_PASSES: usize = 16;
    let mut out = text.to_string();
    for _ in 0..MAX_PASSES {
        let mut changed = false;
        for (name, value) in defines {
            let needle = format!("$({name})");
            if out.contains(&needle) {
                out = out.replace(&needle, value);
                changed = true;
            }
        }
        if !changed || !out.contains("$(") {
            break;
        }
    }
    out
}

fn strip_comment(line: &str) -> &str {
    line.split('#').next().unwrap_or(line)
}

fn extract_define_registers(expanded: &ExpandedSpec) -> Result<Vec<ParsedRegister>> {
    let mut registers = Vec::new();
    let mut idx = 0;
    while idx < expanded.lines.len() {
        let line = normalize_define_line_with_macros(expanded.lines[idx].trim(), &expanded.defines);
        if !line.starts_with("define register") {
            idx += 1;
            continue;
        }
        if line.contains('[') {
            let mut block = line;
            while !block.contains(']') {
                idx += 1;
                if idx >= expanded.lines.len() {
                    break;
                }
                block.push(' ');
                block.push_str(&normalize_define_line_with_macros(
                    expanded.lines[idx].trim(),
                    &expanded.defines,
                ));
            }
            registers.extend(parse_define_register_block(&block, &expanded.defines)?);
        } else if line.ends_with(';') {
            registers.extend(parse_single_define_register(&line, &expanded.defines)?);
        } else {
            let mut block = line;
            let mut is_bracket_block = false;
            loop {
                idx += 1;
                if idx >= expanded.lines.len() {
                    break;
                }
                let next = normalize_define_line_with_macros(
                    expanded.lines[idx].trim(),
                    &expanded.defines,
                );
                block.push(' ');
                block.push_str(&next);
                if block.contains('[') {
                    is_bracket_block = true;
                    if block.contains(']') {
                        break;
                    }
                } else if next.ends_with(';') {
                    break;
                }
            }
            if is_bracket_block {
                registers.extend(parse_define_register_block(&block, &expanded.defines)?);
            } else {
                registers.extend(parse_single_define_register(&block, &expanded.defines)?);
            }
        }
        idx += 1;
    }
    Ok(registers)
}

fn normalize_define_line_with_macros(line: &str, defines: &BTreeMap<String, String>) -> String {
    let mut normalized = line
        .replace("offset =", "offset=")
        .replace("size =", "size=");
    while normalized.contains("offset= ") {
        normalized = normalized.replace("offset= ", "offset=");
    }
    while normalized.contains("size= ") {
        normalized = normalized.replace("size= ", "size=");
    }
    substitute_macros(&normalized, defines)
}

fn parse_offset_and_size(
    rest: &str,
    defines: &BTreeMap<String, String>,
    context: &str,
) -> Result<(u64, u32)> {
    let offset_key = "offset=";
    let size_key = "size=";
    let offset_start = rest
        .find(offset_key)
        .ok_or_else(|| anyhow!("missing offset in {context}"))?
        + offset_key.len();
    let offset_rest = rest[offset_start..].trim_start();
    let offset_end = offset_rest
        .find(|c: char| c.is_ascii_whitespace() || c == '[' || c == ';')
        .unwrap_or(offset_rest.len());
    let base_offset = parse_u64_token(&offset_rest[..offset_end], defines)?;

    let size_start = rest
        .find(size_key)
        .ok_or_else(|| anyhow!("missing size in {context}"))?
        + size_key.len();
    let size_rest = rest[size_start..].trim_start();
    let size_end = size_rest
        .find(|c: char| c.is_ascii_whitespace() || c == '[' || c == ';')
        .unwrap_or(size_rest.len());
    let size = parse_u32_token(&size_rest[..size_end], defines)?;
    Ok((base_offset, size))
}

fn parse_single_define_register(
    line: &str,
    defines: &BTreeMap<String, String>,
) -> Result<Vec<ParsedRegister>> {
    let rest = line
        .strip_prefix("define register")
        .ok_or_else(|| anyhow!("not a define register line"))?
        .trim()
        .trim_end_matches(';');
    let (base_offset, size) = parse_offset_and_size(rest, defines, line)?;
    let key_token_end = |key: &str| -> usize {
        let Some(pos) = rest.find(key) else {
            return 0;
        };
        let value_start = pos + key.len();
        let trimmed = rest[value_start..].trim_start();
        let skipped_ws = rest[value_start..].len() - trimmed.len();
        let tok_len = trimmed
            .find(|c: char| c.is_ascii_whitespace() || c == '[' || c == ';')
            .unwrap_or(trimmed.len());
        value_start + skipped_ws + tok_len
    };
    let header_end = key_token_end("offset=").max(key_token_end("size="));
    let name = rest[header_end..].trim().trim_end_matches(';').trim();
    if name.is_empty() || name == "_" || name.starts_with('[') {
        return Ok(Vec::new());
    }
    Ok(vec![ParsedRegister {
        name: name.to_ascii_lowercase(),
        offset: base_offset,
        size,
        slot_index: 0,
        base_offset,
    }])
}

fn parse_define_register_block(
    block: &str,
    defines: &BTreeMap<String, String>,
) -> Result<Vec<ParsedRegister>> {
    let rest = block
        .strip_prefix("define register")
        .ok_or_else(|| anyhow!("not a define register block"))?
        .trim()
        .trim_end_matches(';');

    let (base_offset, size) = parse_offset_and_size(rest, defines, block)?;

    let bracket_start = rest
        .find('[')
        .ok_or_else(|| anyhow!("missing [ in {block}"))?
        + 1;
    let bracket_end = rest
        .rfind(']')
        .ok_or_else(|| anyhow!("missing ] in {block}"))?;
    let names_blob = &rest[bracket_start..bracket_end];

    let mut out = Vec::new();
    for (slot_index, token) in names_blob.split_whitespace().enumerate() {
        if token == "_" {
            continue;
        }
        let offset = base_offset
            .checked_add((slot_index as u64).saturating_mul(u64::from(size)))
            .ok_or_else(|| anyhow!("register offset overflow in {block}"))?;
        out.push(ParsedRegister {
            name: token.to_ascii_lowercase(),
            offset,
            size,
            slot_index,
            base_offset,
        });
    }
    Ok(out)
}

fn parse_u64_token(token: &str, defines: &BTreeMap<String, String>) -> Result<u64> {
    let token = substitute_macros(token.trim(), defines);
    if let Some(hex) = token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).with_context(|| format!("bad hex offset {token}"))
    } else {
        token
            .parse::<u64>()
            .with_context(|| format!("bad decimal offset {token}"))
    }
}

/// A `size=` token, which SLEIGH writes in either base just as it does
/// `offset=`.
///
/// Only `offset=` was ever read that way. V850 writes `size=0x4`, so its whole
/// spec failed to parse -- one bad token aborts the file, taking every
/// register with it. BPF and eBPF write hex sizes too.
fn parse_u32_token(token: &str, defines: &BTreeMap<String, String>) -> Result<u32> {
    let token = substitute_macros(token.trim(), defines);
    let parsed = match token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))
    {
        Some(hex) => u32::from_str_radix(hex, 16),
        None => token.parse::<u32>(),
    };
    parsed.with_context(|| format!("bad size token {token}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_line_define_register() {
        let regs = parse_define_register_block(
            "define register offset=0 size=8 [ RAX RCX RDX RBX RSP RBP RSI RDI ];",
            &BTreeMap::new(),
        )
        .expect("parse");
        assert_eq!(regs.len(), 8);
        assert_eq!(regs[0].name, "rax");
        assert_eq!(regs[0].offset, 0);
        assert_eq!(regs[0].size, 8);
        assert_eq!(regs[1].name, "rcx");
        assert_eq!(regs[1].offset, 8);
    }

    #[test]
    fn parse_gaps_in_define_register() {
        let regs = parse_define_register_block(
            "define register offset=0 size=4 [ EAX _ ECX _ EDX _ EBX _ ];",
            &BTreeMap::new(),
        )
        .expect("parse");
        assert_eq!(regs.len(), 4);
        assert_eq!(regs[0].name, "eax");
        assert_eq!(regs[0].offset, 0);
        assert_eq!(regs[1].name, "ecx");
        assert_eq!(regs[1].offset, 8);
    }

    #[test]
    fn powerpc32_slaspec_parses() {
        let root = super::super::apply::sleigh_languages_root();
        let path =
            super::super::ldefs::global_language_slaspec_index(&root)["PowerPC:BE:32:default"]
                .clone();
        let parsed = parse_registers_from_slaspec(&path).expect("parse powerpc");
        assert!(parsed.iter().any(|r| r.name == "r0" && r.offset == 0));
    }

    #[test]
    fn arm_v8_slaspec_parses_despite_its_unclosed_conditional() {
        // `ARMv8.sinc` has one more `@if` than `@endif`; Ghidra compiles it, so
        // the parser must too.
        let root = super::super::apply::sleigh_languages_root();
        let path =
            super::super::ldefs::global_language_slaspec_index(&root)["ARM:LE:32:v8"].clone();
        let parsed = parse_registers_from_slaspec(&path).expect("parse arm v8");
        assert!(parsed.iter().any(|r| r.name == "r0"));
    }

    #[test]
    fn aarch64_slaspec_contains_x0() {
        let root = super::super::apply::sleigh_languages_root();
        let path =
            super::super::ldefs::global_language_slaspec_index(&root)["AARCH64:LE:64:v8A"].clone();
        let parsed = parse_registers_from_slaspec(&path).expect("parse AARCH64");
        assert!(
            parsed
                .iter()
                .any(|r| r.name == "x0" && r.offset == 0x4000 && r.size == 8),
            "parsed {} registers, sample: {:?}",
            parsed.len(),
            parsed.iter().take(5).collect::<Vec<_>>()
        );
    }
}

#[cfg(test)]
mod token_tests {
    use super::parse_u32_token;
    use std::collections::BTreeMap;

    #[test]
    fn a_size_is_hex_or_decimal_the_way_an_offset_is() {
        let none = BTreeMap::new();
        assert_eq!(parse_u32_token("4", &none).expect("decimal"), 4);
        // V850 writes `size=0x4`, and only `offset=` was read as hex. One bad
        // token aborts the whole file, so its entire register set was lost.
        assert_eq!(parse_u32_token("0x4", &none).expect("hex"), 4);
        assert_eq!(parse_u32_token("0X10", &none).expect("upper hex"), 16);
        assert!(parse_u32_token("zz", &none).is_err());
    }
}

#[cfg(test)]
mod if_expression_tests {
    use super::evaluate_if_expression;
    use std::collections::BTreeMap;

    fn defines(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn defined_is_a_call_not_a_macro_name() {
        // The whole condition used to be looked up as if it were one macro
        // name, so anything containing `defined(` was false. ARM's VFP and
        // NEON banks sit behind exactly this condition, which is why `s0`,
        // `d0` and `q0` were missing from every ARM register model.
        let d = defines(&[("VFPv3", ""), ("VERSION_8", "")]);
        assert!(evaluate_if_expression("defined(VFPv3)", &d));
        assert!(!evaluate_if_expression("defined(VFPv2)", &d));
        assert!(evaluate_if_expression(
            "defined(VFPv2) || defined(VFPv3)",
            &d
        ));
        assert!(!evaluate_if_expression(
            "defined(VFPv2) || defined(NEON)",
            &d
        ));
        assert!(!evaluate_if_expression(
            "defined(VFPv2) && defined(VFPv3)",
            &d
        ));
        assert!(evaluate_if_expression(
            "defined(VFPv3) && defined(VERSION_8)",
            &d
        ));
    }

    #[test]
    fn comparisons_still_work_and_combine() {
        let d = defines(&[("ENDIAN", "little"), ("ADDRSIZE", "64")]);
        assert!(evaluate_if_expression("ENDIAN == \"little\"", &d));
        assert!(evaluate_if_expression("ENDIAN != \"big\"", &d));
        assert!(evaluate_if_expression(
            "ADDRSIZE == \"32\" || ADDRSIZE == \"64\"",
            &d
        ));
        assert!(!evaluate_if_expression(
            "ADDRSIZE == \"32\" || ADDRSIZE == \"128\"",
            &d
        ));
    }

    #[test]
    fn parentheses_group_rather_than_split() {
        // `((A) || (B))` is one group, not two adjacent ones: splitting it on
        // the `||` inside would leave `((A` and `B))`.
        let d = defines(&[("FPSIZE", "128")]);
        assert!(evaluate_if_expression(
            "((FPSIZE == \"64\") || (FPSIZE == \"128\"))",
            &d
        ));
        assert!(!evaluate_if_expression(
            "((FPSIZE == \"64\") || (FPSIZE == \"32\"))",
            &d
        ));
        assert!(evaluate_if_expression(
            "(ADDRSIZE == \"64\") || (FPSIZE == \"128\")",
            &d
        ));
    }

    #[test]
    fn a_bare_macro_name_is_still_a_presence_test() {
        let d = defines(&[("T_VARIANT", "")]);
        assert!(evaluate_if_expression("T_VARIANT", &d));
        assert!(!evaluate_if_expression("IA64", &d));
    }
}

#[cfg(test)]
mod substitution_preparation_tests {
    use super::*;

    #[test]
    fn literal_fast_path_and_fixed_point_macro_substitution() {
        let defines = BTreeMap::from([
            ("A".to_owned(), "$(B)".to_owned()),
            ("B".to_owned(), "wide".to_owned()),
            ("Z".to_owned(), "$(A)".to_owned()),
            ("CYCLE1".to_owned(), "$(CYCLE2)".to_owned()),
            ("CYCLE2".to_owned(), "$(CYCLE1)".to_owned()),
            ("이름".to_owned(), "값".to_owned()),
        ]);
        for (text, expected) in [
            ("", ""),
            ("literal register r0", "literal register r0"),
            ("unicode 값", "unicode 값"),
            ("$", "$"),
            ("$(", "$("),
            ("$()", "$()"),
            ("$(unknown)", "$(unknown)"),
            ("$(A) $(A)", "wide wide"),
            // Z -> $(A) -> $(B) -> wide requires multi-pass substitution across alphabetical order.
            ("$(Z)", "wide"),
            ("$(이름)", "값"),
            ("prefix $(B) suffix", "prefix wide suffix"),
            ("$(A))", "wide)"),
        ] {
            assert_eq!(substitute_macros(text, &defines), expected, "{text}");
            assert_eq!(substitute_macros(text, &BTreeMap::new()), text);
        }
        // Cyclic macros must terminate without infinite looping.
        let _ = substitute_macros("$(CYCLE1)", &defines);
    }

    #[test]
    fn slaspec_edge_cases_multiline_bracketless_no_space_bracket_and_with_blocks() {
        let tmp = std::env::temp_dir().join(format!(
            "fission-cspec-slaspec-edge-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let entry = tmp.join("edge.slaspec");
        std::fs::write(
            &entry,
            r#"@define OUT "$(INNER)"
@define INNER "4"
with : mode=1 { @define IN_WITH "0x10" }
with : mode=2 {
    @define MULTI_WITH "0x20" }
define register size=$(OUT) offset=$(IN_WITH)
    pc;
define register offset=$(MULTI_WITH) size=$(OUT)[r0 r1];
"#,
        )
        .unwrap();

        let parsed = parse_registers_from_slaspec(&entry).expect("parse edge slaspec");
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(
            parsed
                .iter()
                .any(|r| r.name == "pc" && r.offset == 0x10 && r.size == 4),
            "expected pc at 0x10 size 4 in {parsed:?}"
        );
        assert!(
            parsed
                .iter()
                .any(|r| r.name == "r0" && r.offset == 0x20 && r.size == 4),
            "expected r0 at 0x20 size 4 in {parsed:?}"
        );
        assert!(
            parsed
                .iter()
                .any(|r| r.name == "r1" && r.offset == 0x24 && r.size == 4),
            "expected r1 at 0x24 size 4 in {parsed:?}"
        );
    }
}
