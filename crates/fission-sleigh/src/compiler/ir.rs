use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;

use super::ast::{AstConstructor, AstItem, SpecAst, WithContextFrame};
use super::preprocessor::ExpandedSpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledFrontend {
    pub arch: String,
    pub entry_spec: String,
    pub include_manifest: Vec<String>,
    pub defines: Vec<(String, String)>,
    pub definitions: Vec<CompiledSpecDefinition>,
    pub macros: Vec<CompiledMacro>,
    pub constructors: Vec<CompiledConstructor>,
    pub pcode_ops: Vec<CompiledPcodeOp>,
    pub pattern_nodes: Vec<CompiledPatternNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledSpecDefinition {
    pub kind: String,
    pub source: String,
    pub statement: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledMacro {
    pub name: String,
    pub source: String,
    pub body_line_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledConstructor {
    pub mnemonic: String,
    pub display: String,
    pub source: String,
    pub control_flow: ControlFlowClass,
    pub with_stack: Vec<String>,
    pub semantic_ops: Vec<String>,
    pub signature_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPcodeOp {
    pub name: String,
    pub defined_in: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPatternNode {
    pub node_id: String,
    pub source: String,
    pub mnemonic: String,
    pub with_depth: usize,
    pub control_flow: ControlFlowClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ControlFlowClass {
    None,
    Branch,
    ConditionalBranch,
    Call,
    Return,
    Mixed,
}

impl ControlFlowClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Branch => "branch",
            Self::ConditionalBranch => "conditional_branch",
            Self::Call => "call",
            Self::Return => "return",
            Self::Mixed => "mixed",
        }
    }
}

pub fn compile_frontend(expanded: &ExpandedSpec, ast: &SpecAst) -> Result<CompiledFrontend> {
    let mut collector = Collector {
        definitions: Vec::new(),
        macros: Vec::new(),
        constructors: Vec::new(),
        pcode_ops: BTreeSet::new(),
        pcode_op_sources: BTreeMap::new(),
        pattern_nodes: Vec::new(),
    };
    collector.collect_items(&ast.items, &mut Vec::new());

    let mut pcode_ops = collector
        .pcode_ops
        .into_iter()
        .map(|name| CompiledPcodeOp {
            defined_in: collector
                .pcode_op_sources
                .get(&name)
                .cloned()
                .unwrap_or_else(|| "<unknown>".to_string()),
            name,
        })
        .collect::<Vec<_>>();
    pcode_ops.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name));

    Ok(CompiledFrontend {
        arch: "x86".to_string(),
        entry_spec: expanded
            .entry_spec
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("x86-64.slaspec")
            .to_string(),
        include_manifest: expanded
            .include_manifest
            .iter()
            .map(|entry| format!("{}@{}", entry.relative_path, entry.depth))
            .collect(),
        defines: expanded
            .defines
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
        definitions: collector.definitions,
        macros: collector.macros,
        constructors: collector.constructors,
        pcode_ops,
        pattern_nodes: collector.pattern_nodes,
    })
}

struct Collector {
    definitions: Vec<CompiledSpecDefinition>,
    macros: Vec<CompiledMacro>,
    constructors: Vec<CompiledConstructor>,
    pcode_ops: BTreeSet<String>,
    pcode_op_sources: BTreeMap<String, String>,
    pattern_nodes: Vec<CompiledPatternNode>,
}

impl Collector {
    fn collect_items(&mut self, items: &[AstItem], with_stack: &mut Vec<WithContextFrame>) {
        for item in items {
            match item {
                AstItem::Define(definition) => {
                    let kind = definition
                        .statement
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("unknown")
                        .trim_end_matches(';')
                        .to_string();
                    let source = format!(
                        "{}:{}",
                        definition
                            .file
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("<unknown>"),
                        definition.line_number
                    );
                    if kind == "pcodeop" {
                        if let Some(name) = definition
                            .statement
                            .split_whitespace()
                            .nth(2)
                            .map(|value| value.trim_end_matches(';').to_string())
                        {
                            self.pcode_ops.insert(name.clone());
                            self.pcode_op_sources.insert(name, source.clone());
                        }
                    }
                    self.definitions.push(CompiledSpecDefinition {
                        kind,
                        source,
                        statement: definition.statement.clone(),
                    });
                }
                AstItem::Macro(m) => {
                    self.macros.push(CompiledMacro {
                        name: macro_name(&m.signature),
                        source: format!(
                            "{}:{}",
                            m.file.file_name().and_then(|name| name.to_str()).unwrap_or("<unknown>"),
                            m.line_number
                        ),
                        body_line_count: m.body.lines().count(),
                    });
                }
                AstItem::Constructor(c) => {
                    self.record_constructor(c, with_stack);
                }
                AstItem::WithBlock(block) => {
                    with_stack.push(WithContextFrame {
                        header: block.header.clone(),
                    });
                    self.collect_items(&block.items, with_stack);
                    with_stack.pop();
                }
                AstItem::Raw(_) => {}
            }
        }
    }

    fn record_constructor(
        &mut self,
        constructor: &AstConstructor,
        with_stack: &[WithContextFrame],
    ) {
        let mnemonic = constructor_mnemonic(&constructor.signature);
        let source = format!(
            "{}:{}",
            constructor
                .file
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("<unknown>"),
            constructor.line_number
        );
        let control_flow = classify_control_flow(&constructor.body);
        let semantic_ops = constructor_semantic_ops(&constructor.body, &self.pcode_ops);
        let signature_hash = stable_hash(&constructor.signature);
        self.pattern_nodes.push(CompiledPatternNode {
            node_id: format!("{source}#{:016x}", signature_hash),
            source: source.clone(),
            mnemonic: mnemonic.clone(),
            with_depth: with_stack.len(),
            control_flow,
        });
        self.constructors.push(CompiledConstructor {
            mnemonic,
            display: constructor.signature.clone(),
            source,
            control_flow,
            with_stack: with_stack.iter().map(|frame| frame.header.clone()).collect(),
            semantic_ops,
            signature_hash,
        });
    }
}

fn constructor_mnemonic(signature: &str) -> String {
    signature
        .trim_start_matches(':')
        .split_whitespace()
        .next()
        .unwrap_or("<unknown>")
        .trim_end_matches(',')
        .to_string()
}

fn macro_name(signature: &str) -> String {
    signature
        .strip_prefix("macro ")
        .unwrap_or(signature)
        .split('(')
        .next()
        .unwrap_or("<unknown>")
        .trim()
        .to_string()
}

fn classify_control_flow(body: &str) -> ControlFlowClass {
    let lower = body.to_ascii_lowercase();
    let has_call = lower.contains("call ");
    let has_return = lower.contains("return");
    let has_cbranch = lower.contains("cbranch") || lower.contains("if ");
    let has_branch = lower.contains("goto ") || lower.contains("branch");

    match (has_call, has_return, has_cbranch, has_branch) {
        (false, false, false, false) => ControlFlowClass::None,
        (true, false, false, false) => ControlFlowClass::Call,
        (false, true, false, false) => ControlFlowClass::Return,
        (false, false, true, _) => ControlFlowClass::ConditionalBranch,
        (false, false, false, true) => ControlFlowClass::Branch,
        _ => ControlFlowClass::Mixed,
    }
}

fn constructor_semantic_ops(body: &str, defined_pcode_ops: &BTreeSet<String>) -> Vec<String> {
    let mut found = BTreeSet::new();
    for candidate in defined_pcode_ops {
        let probe = format!("{candidate}(");
        if body.contains(&probe) {
            found.insert(candidate.clone());
        }
    }
    found.into_iter().collect()
}

fn stable_hash(text: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{expand_entry_spec, parse_expanded_spec, x86_64_entry_spec_path};

    #[test]
    fn compile_frontend_collects_pcode_ops_and_patterns() {
        let expanded = expand_entry_spec(&x86_64_entry_spec_path()).expect("expand spec");
        let ast = parse_expanded_spec(&expanded).expect("parse spec");
        let compiled = compile_frontend(&expanded, &ast).expect("compile frontend");
        assert!(!compiled.pcode_ops.is_empty());
        assert!(!compiled.pattern_nodes.is_empty());
        assert!(compiled
            .constructors
            .iter()
            .any(|item| item.mnemonic.eq_ignore_ascii_case("RET") || item.control_flow != ControlFlowClass::None));
    }

    #[test]
    fn control_flow_classifier_separates_branch_from_none() {
        assert_eq!(classify_control_flow("tmp = x + y;"), ControlFlowClass::None);
        assert_eq!(classify_control_flow("goto inst_next;"), ControlFlowClass::Branch);
        assert_eq!(
            classify_control_flow("if cond goto inst_next;"),
            ControlFlowClass::ConditionalBranch
        );
    }
}
