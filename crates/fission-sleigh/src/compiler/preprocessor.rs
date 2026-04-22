use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};

use super::token::{tokenize_line, TokenKind, TokenizedLine};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncludeManifestEntry {
    pub relative_path: String,
    pub depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreprocessedLine {
    pub file: PathBuf,
    pub line_number: usize,
    pub text: String,
    pub include_depth: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandedSpec {
    pub entry_spec: PathBuf,
    pub include_manifest: Vec<IncludeManifestEntry>,
    pub defines: BTreeMap<String, String>,
    pub lines: Vec<PreprocessedLine>,
}

#[derive(Debug, Clone, Copy)]
struct ConditionalFrame {
    parent_active: bool,
    branch_taken: bool,
    current_active: bool,
}

pub fn expand_entry_spec(entry_spec: &Path) -> Result<ExpandedSpec> {
    let mut state = PreprocessorState {
        root_dir: entry_spec
            .parent()
            .ok_or_else(|| anyhow!("entry spec has no parent: {}", entry_spec.display()))?
            .to_path_buf(),
        include_manifest: Vec::new(),
        defines: BTreeMap::new(),
        lines: Vec::new(),
    };
    state.expand_file(entry_spec, 0)?;
    Ok(ExpandedSpec {
        entry_spec: entry_spec.to_path_buf(),
        include_manifest: state.include_manifest,
        defines: state.defines,
        lines: state.lines,
    })
}

struct PreprocessorState {
    root_dir: PathBuf,
    include_manifest: Vec<IncludeManifestEntry>,
    defines: BTreeMap<String, String>,
    lines: Vec<PreprocessedLine>,
}

impl PreprocessorState {
    fn expand_file(&mut self, path: &Path, depth: usize) -> Result<()> {
        let relative_path = path
            .strip_prefix(&self.root_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        self.include_manifest.push(IncludeManifestEntry {
            relative_path,
            depth,
        });

        let source = fs::read_to_string(path)
            .with_context(|| format!("read sleigh source {}", path.display()))?;
        let mut conditionals: Vec<ConditionalFrame> = Vec::new();

        for (line_number, raw_line) in source.lines().enumerate() {
            let tokenized = tokenize_line(path, line_number + 1, raw_line);
            self.handle_line(path, depth, &tokenized, &mut conditionals)?;
        }

        if !conditionals.is_empty() {
            bail!("unterminated conditional block in {}", path.display());
        }

        Ok(())
    }

    fn handle_line(
        &mut self,
        current_file: &Path,
        depth: usize,
        tokenized: &TokenizedLine,
        conditionals: &mut Vec<ConditionalFrame>,
    ) -> Result<()> {
        let is_active = conditionals.iter().all(|frame| frame.current_active);
        let trimmed = strip_comments(&tokenized.raw).trim();
        if trimmed.is_empty() {
            return Ok(());
        }

        let Some(first) = tokenized.tokens.first() else {
            return Ok(());
        };

        if first.kind == TokenKind::Directive {
            match first.lexeme.as_str() {
                "@define" => {
                    if is_active {
                        let name = tokenized
                            .tokens
                            .get(1)
                            .ok_or_else(|| anyhow!("missing define name in {}", current_file.display()))?
                            .lexeme
                            .to_string();
                        let value = tokenized
                            .tokens
                            .get(2)
                            .map(|token| token.lexeme.trim_matches('"').to_string())
                            .unwrap_or_default();
                        self.defines.insert(name, value);
                    }
                }
                "@include" => {
                    if is_active {
                        let include_path = tokenized
                            .tokens
                            .get(1)
                            .ok_or_else(|| anyhow!("missing include target in {}", current_file.display()))?
                            .lexeme
                            .trim_matches('"')
                            .to_string();
                        let resolved = current_file
                            .parent()
                            .ok_or_else(|| anyhow!("include parent missing for {}", current_file.display()))?
                            .join(include_path);
                        self.expand_file(&resolved, depth + 1)?;
                    }
                }
                "@ifdef" | "@ifndef" => {
                    let name = tokenized
                        .tokens
                        .get(1)
                        .ok_or_else(|| anyhow!("missing conditional symbol in {}", current_file.display()))?
                        .lexeme
                        .to_string();
                    let defined = self.defines.contains_key(&name);
                    let predicate = if first.lexeme == "@ifdef" {
                        defined
                    } else {
                        !defined
                    };
                    let parent_active = conditionals.iter().all(|frame| frame.current_active);
                    conditionals.push(ConditionalFrame {
                        parent_active,
                        branch_taken: predicate,
                        current_active: parent_active && predicate,
                    });
                }
                "@else" => {
                    let Some(last) = conditionals.last_mut() else {
                        bail!("@else without @ifdef in {}", current_file.display());
                    };
                    let new_active = last.parent_active && !last.branch_taken;
                    last.current_active = new_active;
                    last.branch_taken = true;
                }
                "@endif" => {
                    if conditionals.pop().is_none() {
                        bail!("@endif without @ifdef in {}", current_file.display());
                    }
                }
                other => {
                    if is_active {
                        self.lines.push(PreprocessedLine {
                            file: current_file.to_path_buf(),
                            line_number: tokenized.line_number,
                            text: format!("// unsupported directive preserved: {other}"),
                            include_depth: depth,
                        });
                    }
                }
            }
            return Ok(());
        }

        if !is_active {
            return Ok(());
        }

        self.lines.push(PreprocessedLine {
            file: current_file.to_path_buf(),
            line_number: tokenized.line_number,
            text: tokenized.raw.clone(),
            include_depth: depth,
        });
        Ok(())
    }
}

fn strip_comments(raw: &str) -> &str {
    raw.split('#').next().unwrap_or(raw)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn expand_entry_spec_resolves_include_graph() {
        let expanded = expand_entry_spec(&super::super::x86_64_entry_spec_path())
            .expect("expand x86-64 entry spec");
        assert!(expanded.include_manifest.len() >= 3);
        assert_eq!(
            expanded.include_manifest.first().map(|item| item.relative_path.as_str()),
            Some("x86-64.slaspec")
        );
        assert!(expanded
            .include_manifest
            .iter()
            .any(|item| item.relative_path == "x86.slaspec"));
        assert!(expanded
            .include_manifest
            .iter()
            .any(|item| item.relative_path == "ia.sinc"));
        assert!(expanded.defines.contains_key("IA64"));
        assert!(!expanded.lines.is_empty());
    }

    #[test]
    fn preprocess_handles_ifdef_else_and_include() {
        let dir = tempdir().expect("tempdir");
        let entry = dir.path().join("entry.slaspec");
        let child = dir.path().join("child.sinc");
        fs::write(&child, "child_line\n").expect("write child");
        fs::write(
            &entry,
            "@define IA64 \"IA64\"\n\
             @ifdef IA64\n\
             kept_line\n\
             @else\n\
             dropped_line\n\
             @endif\n\
             @include \"child.sinc\"\n",
        )
        .expect("write entry");

        let expanded = expand_entry_spec(&entry).expect("expand custom entry");
        let rendered = expanded
            .lines
            .iter()
            .map(|line| line.text.trim().to_string())
            .collect::<Vec<_>>();
        assert!(rendered.contains(&"kept_line".to_string()));
        assert!(!rendered.contains(&"dropped_line".to_string()));
        assert!(rendered.contains(&"child_line".to_string()));
    }
}
