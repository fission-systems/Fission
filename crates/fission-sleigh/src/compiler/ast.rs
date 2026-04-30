use std::path::PathBuf;

use anyhow::{Result, anyhow, bail};

use super::preprocessor::{ExpandedSpec, PreprocessedLine};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecAst {
    pub items: Vec<AstItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AstItem {
    Define(AstDefine),
    Constructor(AstConstructor),
    Macro(AstMacro),
    WithBlock(AstWithBlock),
    Raw(AstRaw),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstDefine {
    pub file: PathBuf,
    pub line_number: usize,
    pub statement: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstConstructor {
    pub file: PathBuf,
    pub line_number: usize,
    pub signature: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstMacro {
    pub file: PathBuf,
    pub line_number: usize,
    pub signature: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstWithBlock {
    pub file: PathBuf,
    pub line_number: usize,
    pub header: String,
    pub body: String,
    pub items: Vec<AstItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AstRaw {
    pub file: PathBuf,
    pub line_number: usize,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithContextFrame {
    pub header: String,
}

pub fn parse_expanded_spec(expanded: &ExpandedSpec) -> Result<SpecAst> {
    let mut cursor = ParseCursor {
        lines: &expanded.lines,
        index: 0,
    };
    Ok(SpecAst {
        items: cursor.parse_items(false)?,
    })
}

struct ParseCursor<'a> {
    lines: &'a [PreprocessedLine],
    index: usize,
}

impl<'a> ParseCursor<'a> {
    fn parse_items(&mut self, stop_at_closing_brace: bool) -> Result<Vec<AstItem>> {
        let mut items = Vec::new();

        while self.index < self.lines.len() {
            let line = &self.lines[self.index];
            let trimmed = strip_comments(&line.text).trim();

            if trimmed.is_empty() {
                self.index += 1;
                continue;
            }

            if stop_at_closing_brace && trimmed == "}" {
                self.index += 1;
                break;
            }

            if trimmed.starts_with("define ") {
                items.push(AstItem::Define(self.collect_define()?));
                continue;
            }

            if trimmed.starts_with("macro ") {
                items.push(AstItem::Macro(self.collect_macro()?));
                continue;
            }

            if trimmed.starts_with("with ") && trimmed.contains('{') {
                items.push(AstItem::WithBlock(self.collect_with_block()?));
                continue;
            }

            if trimmed.starts_with(':')
                || (trimmed.contains(':')
                    && !trimmed.starts_with('@')
                    && !trimmed.starts_with("define"))
            {
                items.push(AstItem::Constructor(self.collect_constructor()?));
                continue;
            }

            items.push(AstItem::Raw(AstRaw {
                file: line.file.clone(),
                line_number: line.line_number,
                text: line.text.clone(),
            }));
            self.index += 1;
        }

        Ok(items)
    }

    fn collect_define(&mut self) -> Result<AstDefine> {
        let start = self
            .current()
            .ok_or_else(|| anyhow!("missing define start"))?;
        let statement = self.collect_until_semicolon();
        Ok(AstDefine {
            file: start.file.clone(),
            line_number: start.line_number,
            statement,
        })
    }

    fn collect_constructor(&mut self) -> Result<AstConstructor> {
        let start = self
            .current()
            .ok_or_else(|| anyhow!("missing constructor start"))?
            .clone();
        let block = self.collect_constructor_block()?;
        let (signature, body) = if block.contains('{') {
            split_signature_and_body(&block)?
        } else {
            (block.trim().to_string(), String::new())
        };
        Ok(AstConstructor {
            file: start.file,
            line_number: start.line_number,
            signature,
            body,
        })
    }

    fn collect_macro(&mut self) -> Result<AstMacro> {
        let start = self
            .current()
            .ok_or_else(|| anyhow!("missing macro start"))?
            .clone();
        let block = self.collect_braced_block()?;
        let (signature, body) = split_signature_and_body(&block)?;
        Ok(AstMacro {
            file: start.file,
            line_number: start.line_number,
            signature,
            body,
        })
    }

    fn collect_with_block(&mut self) -> Result<AstWithBlock> {
        let start = self
            .current()
            .ok_or_else(|| anyhow!("missing with-block start"))?
            .clone();
        let block_lines = self.collect_braced_block_lines()?;
        let block = block_lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let (header, body) = split_signature_and_body(&block)?;
        let nested_lines = preserve_inner_block_lines(&block_lines, &body);
        let mut nested = ParseCursor {
            lines: &nested_lines,
            index: 0,
        };
        let items = nested.parse_items(false)?;
        Ok(AstWithBlock {
            file: start.file,
            line_number: start.line_number,
            header,
            body,
            items,
        })
    }

    fn collect_until_semicolon(&mut self) -> String {
        let mut collected = Vec::new();
        while let Some(line) = self.current() {
            collected.push(line.text.clone());
            self.index += 1;
            if strip_comments(&line.text).contains(';') {
                break;
            }
        }
        collected.join("\n")
    }

    fn collect_constructor_block(&mut self) -> Result<String> {
        let mut collected = Vec::new();
        let mut brace_depth = 0i64;
        let mut bracket_depth = 0i64;
        let mut seen_any_block = false;
        let start_line = self.current().cloned();

        while let Some(line) = self.current() {
            let text = line.text.clone();
            let structural_text = strip_comments(&text).trim().to_string();
            let b_open = count_structural_char(&structural_text, '{');
            let b_close = count_structural_char(&structural_text, '}');
            let br_open = count_structural_char(&structural_text, '[');
            let br_close = count_structural_char(&structural_text, ']');

            if structural_text.is_empty() {
                collected.push(text);
                self.index += 1;
                continue;
            }

            // Check for parent block termination BEFORE processing depths
            if brace_depth == 0 && b_open == 0 && b_close > 0 {
                break;
            }
            if bracket_depth == 0 && br_open == 0 && br_close > 0 {
                break;
            }

            // A new constructor start should terminate the current block IF we're at depth 0
            if !collected.is_empty() && brace_depth == 0 && bracket_depth == 0 {
                if structural_text.starts_with(':')
                    || (structural_text.contains(':')
                        && !structural_text.starts_with("define")
                        && !structural_text.starts_with('@')
                        && !structural_text.starts_with("attach"))
                {
                    break;
                }
            }

            brace_depth += b_open as i64;
            brace_depth -= b_close as i64;
            if b_open > 0 || b_close > 0 {
                seen_any_block = true;
            }

            bracket_depth += br_open as i64;
            bracket_depth -= br_close as i64;
            if br_open > 0 || br_close > 0 {
                seen_any_block = true;
            }

            collected.push(text);
            self.index += 1;

            if seen_any_block && brace_depth == 0 && bracket_depth == 0 {
                return Ok(collected.join("\n"));
            }

            if is_unbraced_constructor_terminal(&structural_text)
                && brace_depth == 0
                && bracket_depth == 0
            {
                return Ok(collected.join("\n"));
            }
        }

        if !collected.is_empty() && brace_depth == 0 && bracket_depth == 0 {
            return Ok(collected.join("\n"));
        }

        let start = start_line;
        bail!(
            "unterminated constructor starting at {}:{}",
            start
                .as_ref()
                .map(|l| l.file.display().to_string())
                .unwrap_or_default(),
            start.as_ref().map(|l| l.line_number).unwrap_or_default()
        )
    }

    fn collect_braced_block(&mut self) -> Result<String> {
        Ok(self
            .collect_braced_block_lines()?
            .into_iter()
            .map(|line| line.text)
            .collect::<Vec<_>>()
            .join("\n"))
    }

    fn collect_braced_block_lines(&mut self) -> Result<Vec<PreprocessedLine>> {
        let mut collected = Vec::new();
        let mut brace_depth = 0i64;
        let mut seen_open = false;
        let start = self.current().cloned();

        while let Some(line) = self.current() {
            let text = line.text.clone();
            let structural_text = strip_comments(&text);
            brace_depth += count_structural_char(structural_text, '{') as i64;
            if count_structural_char(structural_text, '{') > 0 {
                seen_open = true;
            }
            brace_depth -= count_structural_char(structural_text, '}') as i64;
            collected.push(line.clone());
            self.index += 1;

            if seen_open && brace_depth == 0 {
                return Ok(collected);
            }
        }

        bail!(
            "unterminated braced block starting at {}:{}",
            start
                .as_ref()
                .map(|l| l.file.display().to_string())
                .unwrap_or_default(),
            start.as_ref().map(|l| l.line_number).unwrap_or_default()
        )
    }

    fn current(&self) -> Option<&'a PreprocessedLine> {
        self.lines.get(self.index)
    }
}

fn preserve_inner_block_lines(
    block_lines: &[PreprocessedLine],
    body: &str,
) -> Vec<PreprocessedLine> {
    let mut lines = Vec::new();
    if block_lines.len() > 2 {
        lines.extend(block_lines[1..block_lines.len() - 1].iter().cloned());
    } else {
        lines.extend(body.lines().enumerate().map(|(offset, text)| {
            PreprocessedLine {
                file: block_lines
                    .first()
                    .map(|line| line.file.clone())
                    .unwrap_or_default(),
                line_number: block_lines
                    .first()
                    .map(|line| line.line_number + offset + 1)
                    .unwrap_or(offset + 1),
                text: text.to_string(),
                include_depth: block_lines
                    .first()
                    .map(|line| line.include_depth)
                    .unwrap_or_default(),
            }
        }));
    }
    lines
}

fn strip_comments(raw: &str) -> &str {
    let mut in_string = false;
    for (idx, ch) in raw.char_indices() {
        if ch == '"' {
            in_string = !in_string;
        } else if ch == '#' && !in_string {
            return &raw[..idx];
        }
    }
    raw
}

fn count_structural_char(text: &str, needle: char) -> usize {
    let mut in_string = false;
    let mut count = 0usize;
    for ch in text.chars() {
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if !in_string && ch == needle {
            count += 1;
        }
    }
    count
}

fn first_structural_char(text: &str, needle: char) -> Option<usize> {
    let mut in_string = false;
    for (idx, ch) in text.char_indices() {
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if !in_string && ch == needle {
            return Some(idx);
        }
    }
    None
}

fn last_structural_char(text: &str, needle: char) -> Option<usize> {
    let mut in_string = false;
    let mut last = None;
    for (idx, ch) in text.char_indices() {
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if !in_string && ch == needle {
            last = Some(idx);
        }
    }
    last
}

fn is_unbraced_constructor_terminal(trimmed: &str) -> bool {
    trimmed == "unimpl" || trimmed.ends_with(" unimpl")
}

fn split_signature_and_body(block: &str) -> Result<(String, String)> {
    let open = first_structural_char(block, '{')
        .ok_or_else(|| anyhow!("missing opening brace in block"))?;
    let close = last_structural_char(block, '}')
        .ok_or_else(|| anyhow!("missing closing brace in block"))?;
    if close <= open {
        bail!("invalid block ordering");
    }
    let signature = block[..open].trim().to_string();
    let body = block[open + 1..close].trim().to_string();
    Ok((signature, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{expand_entry_spec, x86_64_entry_spec_path};

    #[test]
    fn parse_expanded_spec_finds_with_blocks_and_constructors() {
        let expanded = expand_entry_spec(&x86_64_entry_spec_path()).expect("expand spec");
        let ast = parse_expanded_spec(&expanded).expect("parse ast");
        assert!(
            ast.items
                .iter()
                .any(|item| matches!(item, AstItem::WithBlock(_)))
        );
        assert!(
            ast.items
                .iter()
                .any(|item| matches!(item, AstItem::Constructor(_)))
        );
    }

    #[test]
    fn parse_with_block_recurses_into_inner_items() {
        let expanded = ExpandedSpec {
            entry_spec: PathBuf::from("/tmp/sample"),
            include_manifest: Vec::new(),
            defines: Default::default(),
            lines: vec![
                PreprocessedLine {
                    file: PathBuf::from("/tmp/sample"),
                    line_number: 1,
                    text: "with : mode=0 {".to_string(),
                    include_depth: 0,
                },
                PreprocessedLine {
                    file: PathBuf::from("/tmp/sample"),
                    line_number: 2,
                    text: ":NOP is byte=0x90 { }".to_string(),
                    include_depth: 0,
                },
                PreprocessedLine {
                    file: PathBuf::from("/tmp/sample"),
                    line_number: 3,
                    text: "}".to_string(),
                    include_depth: 0,
                },
            ],
        };
        let ast = parse_expanded_spec(&expanded).expect("parse spec");
        let with_block = ast
            .items
            .iter()
            .find_map(|item| match item {
                AstItem::WithBlock(block) => Some(block),
                _ => None,
            })
            .expect("with block");
        assert!(
            with_block
                .items
                .iter()
                .any(|item| matches!(item, AstItem::Constructor(_)))
        );
    }
}
