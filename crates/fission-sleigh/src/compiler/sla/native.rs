use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{bail, Result};

use super::*;
use crate::compiler::ir::*;

/// Decoded Ghidra `.sla` owner model.
///
/// This model is deliberately shaped after Ghidra's compiled artifact owners:
/// language -> subtable -> decision node/constructor -> operand symbols ->
/// ConstructTpl.  It is the migration boundary for replacing Fission-local
/// constructor overlays with `.sla` native identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlaLanguage {
    pub path: PathBuf,
    pub version: u8,
    pub source_files: BTreeMap<u64, String>,
    pub spaces: BTreeMap<u64, CompiledSpaceRef>,
    /// Index of the unique (temporary) address space in the `.sla` space table.
    pub unique_space_index: u64,
    /// Index of the register address space in the `.sla` space table.
    pub register_space_index: u64,
    /// Base offset for unique temporary varnode allocation (`uniqbase` from `.sla`).
    pub uniqbase: u64,
    pub subtables: BTreeMap<String, SlaSubtable>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlaSubtable {
    pub id: u32,
    pub name: String,
    pub constructors: Vec<SlaConstructor>,
    pub decision_tree: Option<SlaDecisionTree>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlaConstructor {
    pub subtable_id: u32,
    pub subtable_name: String,
    pub constructor_id: u32,
    pub constructor_slot: usize,
    pub decode_status: CompiledSlaDecodeStatus,
    pub operands: Vec<SlaOperandSymbol>,
    pub print: CompiledDisplayTemplate,
    pub construct_tpl: SlaConstructTpl,
    pub debug_source_file: String,
    pub debug_source_line: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlaOperandSymbol {
    pub symbol_id: Option<u32>,
    pub hand_index: usize,
    pub spec: CompiledOperandSpec,
    pub display: CompiledDisplayOperand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlaDecisionTree {
    pub root_node_index: usize,
    pub nodes: Vec<SlaDecisionNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlaDecisionNode {
    pub context_decision: bool,
    pub start_bit: u32,
    pub bit_size: u32,
    pub children: Vec<SlaDecisionEdge>,
    pub terminal_pairs: Vec<SlaDecisionPair>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlaDecisionEdge {
    pub value: u32,
    pub next_node_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlaDecisionPair {
    pub subtable_id: u32,
    pub constructor_id: u32,
    pub pattern: SlaDisjointPattern,
}

pub type SlaDisjointPattern = CompiledDisjointPattern;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlaConstructTpl {
    pub num_labels: u32,
    pub result: Option<CompiledHandleTpl>,
    pub ops: Vec<CompiledOpTpl>,
}

impl SlaLanguage {
    pub(super) fn from_compiled_library(library: &CompiledSlaTemplateLibrary) -> Self {
        let subtables = library
            .subtables
            .iter()
            .map(|(name, subtable)| {
                let id = subtable
                    .constructors
                    .iter()
                    .map(|constructor| constructor.subtable_id)
                    .next()
                    .unwrap_or(u32::MAX);
                (
                    name.clone(),
                    SlaSubtable {
                        id,
                        name: name.clone(),
                        constructors: subtable
                            .constructors
                            .iter()
                            .map(SlaConstructor::from_compiled_template)
                            .collect(),
                        decision_tree: subtable
                            .decision_tree
                            .as_ref()
                            .map(|tree| SlaDecisionTree::from_compiled_tree(id, tree)),
                    },
                )
            })
            .collect();
        Self {
            path: library.path.clone(),
            version: library.version,
            source_files: library.source_files.clone(),
            spaces: library.spaces.clone(),
            unique_space_index: library.unique_space_index,
            register_space_index: library.register_space_index,
            uniqbase: library.uniqbase,
            subtables,
        }
    }
}

impl SlaConstructor {
    fn from_compiled_template(template: &CompiledSlaConstructorTemplate) -> Self {
        Self {
            subtable_id: template.subtable_id,
            subtable_name: template.subtable_name.clone(),
            constructor_id: template.id,
            constructor_slot: template.constructor_slot,
            decode_status: template.decode_status,
            operands: template
                .operand_specs
                .iter()
                .cloned()
                .enumerate()
                .map(|(index, spec)| SlaOperandSymbol {
                    symbol_id: None,
                    hand_index: index,
                    spec,
                    display: template.display_operands.get(index).cloned().unwrap_or(
                        CompiledDisplayOperand {
                            operand_index: index,
                            kind: CompiledDisplayOperandKind::Generic,
                        },
                    ),
                })
                .collect(),
            print: template.display_template.clone(),
            construct_tpl: SlaConstructTpl {
                num_labels: template.constructor_template.num_labels,
                result: template.constructor_template.result.clone(),
                ops: template.constructor_template.ops.clone(),
            },
            debug_source_file: template.source_file.clone(),
            debug_source_line: template.line,
        }
    }
}

impl SlaDecisionTree {
    fn from_compiled_tree(
        subtable_id: u32,
        tree: &crate::compiler::ir::CompiledDecisionTree,
    ) -> Self {
        Self {
            root_node_index: tree.root_node_index,
            nodes: tree
                .nodes
                .iter()
                .map(|node| SlaDecisionNode {
                    context_decision: matches!(
                        node.probe,
                        crate::compiler::ir::CompiledDecisionProbe::SlaContextBits { .. }
                            | crate::compiler::ir::CompiledDecisionProbe::ContextBitSlice { .. }
                            | crate::compiler::ir::CompiledDecisionProbe::ContextFieldRef(_)
                    ),
                    start_bit: match node.probe {
                        crate::compiler::ir::CompiledDecisionProbe::SlaInstructionBits {
                            start_bit,
                            ..
                        }
                        | crate::compiler::ir::CompiledDecisionProbe::SlaContextBits {
                            start_bit,
                            ..
                        } => start_bit,
                        _ => 0,
                    },
                    bit_size: match node.probe {
                        crate::compiler::ir::CompiledDecisionProbe::SlaInstructionBits {
                            bit_size,
                            ..
                        }
                        | crate::compiler::ir::CompiledDecisionProbe::SlaContextBits {
                            bit_size,
                            ..
                        } => bit_size,
                        _ => 0,
                    },
                    children: node
                        .branches
                        .iter()
                        .map(|edge| SlaDecisionEdge {
                            value: u32::from(edge.value),
                            next_node_index: edge.next_node_index,
                        })
                        .collect(),
                    terminal_pairs: node
                        .leaf_entries
                        .iter()
                        .map(|entry| SlaDecisionPair {
                            subtable_id: if entry.subtable_id == 0 {
                                subtable_id
                            } else {
                                entry.subtable_id
                            },
                            constructor_id: entry.constructor_id,
                            pattern: entry.pattern.clone(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}
