//! Minimal SLEIGH `.slaspec`/`.sinc` preprocessor and `define register` extractor.
//!
//! Used to build [`RegisterModel`](super::register_model::RegisterModel) from checked-in
//! `utils/sleigh-specs` without a Ghidra install or compiled `.sla` artifacts.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};

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
pub fn parse_registers_from_slaspec(entry_spec: &Path) -> Result<Vec<ParsedRegister>> {
    let expanded = expand_entry_spec(entry_spec)?;
    extract_define_registers(&expanded)
}

struct ExpandedSpec {
    defines: BTreeMap<String, String>,
    lines: Vec<String>,
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
        seen_files: HashSet::new(),
    };
    state.expand_file(entry_spec)?;
    Ok(ExpandedSpec {
        defines: state.defines,
        lines: state.lines,
    })
}

struct PreprocessorState {
    root_dir: PathBuf,
    defines: BTreeMap<String, String>,
    lines: Vec<String>,
    seen_files: HashSet<PathBuf>,
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
        if !self.seen_files.insert(canonical) {
            return Ok(());
        }
        let source = fs::read_to_string(path)
            .with_context(|| format!("read sleigh source {}", path.display()))?;
        let mut conditionals: Vec<ConditionalFrame> = Vec::new();
        let mut with_depth: u32 = 0;

        for raw_line in source.lines() {
            let trimmed = strip_comment(raw_line).trim();
            if trimmed.is_empty() {
                continue;
            }

            if trimmed.starts_with("with ") {
                if let Some(open) = trimmed.find('{') {
                    with_depth = with_depth.saturating_add(1);
                    let inner = trimmed[open + 1..].trim();
                    if !inner.is_empty() && !inner.starts_with('}') {
                        self.handle_content_line(inner, path, &mut conditionals)?;
                    }
                    if inner.ends_with('}') {
                        with_depth = with_depth.saturating_sub(1);
                    }
                }
                continue;
            }
            if with_depth > 0 {
                if trimmed == "}" || trimmed.ends_with('}') {
                    with_depth = with_depth.saturating_sub(1);
                }
                if trimmed == "}" {
                    continue;
                }
            }

            self.handle_content_line(trimmed, path, &mut conditionals)?;
        }

        if !conditionals.is_empty() {
            bail!("unterminated conditional block in {}", path.display());
        }
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
                    .ok_or_else(|| anyhow!("include parent missing for {}", current_file.display()))?
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
            let predicate =
                evaluate_if_expression(trimmed["@elif ".len()..].trim(), &self.defines);
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
            conditionals.pop().ok_or_else(|| {
                anyhow!("@endif without @if in {}", current_file.display())
            })?;
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

fn evaluate_if_expression(expr: &str, defines: &BTreeMap<String, String>) -> bool {
    if let Some((lhs, rhs)) = expr.split_once("==") {
        let lhs = lhs.trim();
        let rhs = rhs.trim().trim_matches('"');
        let left_val = defines.get(lhs).map(String::as_str).unwrap_or("");
        return left_val == rhs;
    }
    if let Some((lhs, rhs)) = expr.split_once("!=") {
        let lhs = lhs.trim();
        let rhs = rhs.trim().trim_matches('"');
        let left_val = defines.get(lhs).map(String::as_str).unwrap_or("");
        return left_val != rhs;
    }
    defines.contains_key(expr.trim())
}

fn substitute_macros(text: &str, defines: &BTreeMap<String, String>) -> String {
    let mut out = text.to_string();
    for (name, value) in defines {
        let needle = format!("$({name})");
        if out.contains(&needle) {
            out = out.replace(&needle, value);
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
            loop {
                idx += 1;
                if idx >= expanded.lines.len() {
                    break;
                }
                let next =
                    normalize_define_line_with_macros(expanded.lines[idx].trim(), &expanded.defines);
                block.push(' ');
                block.push_str(&next);
                if next.contains(']') {
                    break;
                }
            }
            registers.extend(parse_define_register_block(&block, &expanded.defines)?);
        }
        idx += 1;
    }
    Ok(registers)
}

fn normalize_define_line_with_macros(line: &str, defines: &BTreeMap<String, String>) -> String {
    let mut normalized = line.replace("offset =", "offset=").replace("size =", "size=");
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
        .find(|c: char| c.is_ascii_whitespace())
        .unwrap_or(offset_rest.len());
    let base_offset = parse_u64_token(&offset_rest[..offset_end], defines)?;

    let size_start = rest
        .find(size_key)
        .ok_or_else(|| anyhow!("missing size in {context}"))?
        + size_key.len();
    let size_rest = rest[size_start..].trim_start();
    let size_end = size_rest
        .find(|c: char| c.is_ascii_whitespace() || c == '[')
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
    let size_key_pos = rest.find("size=").unwrap_or(0) + "size=".len();
    let size_rest = rest[size_key_pos..].trim_start();
    let size_token_end = size_rest
        .find(|c: char| c.is_ascii_whitespace() || c == '[')
        .unwrap_or(size_rest.len());
    // Skip macro/size token characters.
    let mut idx = 0;
    for ch in size_rest.chars() {
        if idx >= size_token_end {
            break;
        }
        idx += ch.len_utf8();
    }
    let name = size_rest[idx..].trim().trim_end_matches(';');
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
    if let Some(hex) = token.strip_prefix("0x").or_else(|| token.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).with_context(|| format!("bad hex offset {token}"))
    } else {
        token
            .parse::<u64>()
            .with_context(|| format!("bad decimal offset {token}"))
    }
}

fn parse_u32_token(token: &str, defines: &BTreeMap<String, String>) -> Result<u32> {
    let token = substitute_macros(token.trim(), defines);
    token
        .parse::<u32>()
        .with_context(|| format!("bad size token {token}"))
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
        let path = super::super::ldefs::global_language_slaspec_index(&root)
            ["PowerPC:BE:32:default"]
            .clone();
        let parsed = parse_registers_from_slaspec(&path).expect("parse powerpc");
        assert!(parsed.iter().any(|r| r.name == "r0" && r.offset == 0));
    }
    #[test]
    fn aarch64_slaspec_contains_x0() {
        let root = super::super::apply::sleigh_languages_root();
        let path = super::super::ldefs::global_language_slaspec_index(&root)
            ["AARCH64:LE:64:v8A"]
            .clone();
        let parsed = parse_registers_from_slaspec(&path).expect("parse AARCH64");
        assert!(
            parsed.iter().any(|r| r.name == "x0" && r.offset == 0x4000 && r.size == 8),
            "parsed {} registers, sample: {:?}",
            parsed.len(),
            parsed.iter().take(5).collect::<Vec<_>>()
        );
    }
}
