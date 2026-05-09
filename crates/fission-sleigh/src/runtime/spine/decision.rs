use anyhow::Result;

use crate::compiler::{
    CompiledDecisionLeafEntry, CompiledDecisionProbe, CompiledDisjointPattern,
    CompiledExecutableConstructor, CompiledFrontend, CompiledPatternBlock,
    CompiledSubtableDefinition,
};

pub trait DecisionProbeEvaluator {
    fn probe_values(&mut self, probe: CompiledDecisionProbe) -> Result<Vec<u8>>;
    fn instruction_bytes(&self, offset: i32, size: u32) -> Result<u32>;
    fn context_bytes(&self, offset: i32, size: u32) -> Result<u32>;
}

#[derive(Debug, Clone)]
pub struct RuntimeSelection<'a> {
    pub constructor: &'a CompiledExecutableConstructor,
    pub constructor_index: usize,
    pub subtable_id: u32,
    pub constructor_id: u32,
    pub constructor_slot: usize,
    pub trace: RuntimeMatchTrace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMatchTrace {
    pub root_bucket: String,
    pub probes: Vec<RuntimeMatchProbe>,
    pub leaf_constructor_indexes: Vec<usize>,
    pub matched_leaf_pattern: Option<CompiledDisjointPattern>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMatchProbe {
    pub probe: CompiledDecisionProbe,
    pub value: u8,
}

pub fn select_constructor<'a, E, M>(
    compiled: &'a CompiledFrontend,
    roots: impl IntoIterator<Item = (String, usize)>,
    mut evaluator_factory: impl FnMut() -> E,
    mut constructor_matches: M,
) -> Option<RuntimeSelection<'a>>
where
    E: DecisionProbeEvaluator,
    M: FnMut(&CompiledExecutableConstructor) -> Result<()>,
{
    for (table_name, root_node_index) in roots {
        let subtable = compiled.subtables.get(&table_name)?;
        let mut evaluator = evaluator_factory();
        if let Some(selection) = walk_decision_tree(
            subtable,
            &subtable.decision_tree,
            root_node_index,
            &mut evaluator,
            &mut constructor_matches,
            RuntimeMatchTrace {
                root_bucket: table_name.clone(),
                probes: Vec::new(),
                leaf_constructor_indexes: Vec::new(),
                matched_leaf_pattern: None,
            },
        ) {
            return Some(selection);
        }
    }
    None
}

fn walk_decision_tree<'a, E, M>(
    subtable: &'a CompiledSubtableDefinition,
    decision_tree: &'a crate::compiler::CompiledDecisionTree,
    node_index: usize,
    evaluator: &mut E,
    constructor_matches: &mut M,
    mut trace: RuntimeMatchTrace,
) -> Option<RuntimeSelection<'a>>
where
    E: DecisionProbeEvaluator,
    M: FnMut(&CompiledExecutableConstructor) -> Result<()>,
{
    let node = decision_tree.nodes.get(node_index)?;
    let trace_walk = crate::runtime::diagnostics::terminal_reselect_trace_enabled();
    match node.probe {
        CompiledDecisionProbe::Terminal => {
            trace.leaf_constructor_indexes = if node.leaf_entries.is_empty() {
                node.leaf_constructor_indexes.clone()
            } else {
                node.leaf_entries
                    .iter()
                    .map(|entry| entry.constructor_index)
                    .collect()
            };
            let mut first_unsupported_match = None;
            let trace_terminal = std::env::var_os("FISSION_TRACE_TERMINAL_VERIFY").is_some();
            let mut matched_any_pattern = false;
            let leaf_entries: Vec<CompiledDecisionLeafEntry> = if node.leaf_entries.is_empty() {
                if subtable.sla_subtable_id != 0 || !subtable.constructors_by_sla_id.is_empty() {
                    return None;
                }
                node.leaf_constructor_indexes
                    .iter()
                    .copied()
                    .map(|constructor_index| CompiledDecisionLeafEntry {
                        subtable_id: 0,
                        constructor_id: constructor_index as u32,
                        constructor_index,
                        pattern: always_true_instruction_pattern(),
                    })
                    .collect()
            } else {
                node.leaf_entries.clone()
            };
            for entry in &leaf_entries {
                let Some((constructor_index, constructor)) =
                    resolve_leaf_constructor(subtable, entry)
                else {
                    continue;
                };
                let matched = disjoint_pattern_matches(evaluator, &entry.pattern);
                if trace_terminal {
                    eprintln!(
                        "[terminal-verify] ctor={} sla_subtable={} sla_ctor={} mnemonic={} source={} matched={} pattern={:?}",
                        constructor_index,
                        entry.subtable_id,
                        entry.constructor_id,
                        constructor.mnemonic,
                        constructor.source,
                        matched,
                        entry.pattern
                    );
                }
                if !matched {
                    continue;
                }
                matched_any_pattern = true;
                if constructor_matches(constructor).is_ok() {
                    if constructor.runtime_ready {
                        trace.matched_leaf_pattern = Some(entry.pattern.clone());
                        let constructor_slot = selection_constructor_slot(
                            subtable,
                            constructor,
                            constructor_index,
                            entry.subtable_id,
                        )?;
                        return Some(RuntimeSelection {
                            constructor,
                            constructor_index,
                            subtable_id: entry.subtable_id,
                            constructor_id: entry.constructor_id,
                            constructor_slot,
                            trace,
                        });
                    }
                    if first_unsupported_match.is_none() {
                        first_unsupported_match = Some((
                            constructor,
                            constructor_index,
                            entry.subtable_id,
                            entry.constructor_id,
                        ));
                    }
                }
            }
            if !matched_any_pattern {
                return None;
            }
            first_unsupported_match.and_then(
                |(constructor, constructor_index, subtable_id, constructor_id)| {
                    let constructor_slot = selection_constructor_slot(
                        subtable,
                        constructor,
                        constructor_index,
                        subtable_id,
                    )?;
                    Some(RuntimeSelection {
                        constructor,
                        constructor_index,
                        subtable_id,
                        constructor_id,
                        constructor_slot,
                        trace,
                    })
                },
            )
        }
        probe => {
            let values = evaluator.probe_values(probe).ok()?;
            if trace_walk {
                eprintln!(
                    "[decision-walk] node={} probe={:?} values={:?} branches={}",
                    node_index,
                    probe,
                    values,
                    node.branches.len(),
                );
            }
            for value in values {
                let Some(edge) = node.branches.iter().find(|edge| edge.value == value) else {
                    if trace_walk {
                        eprintln!(
                            "[decision-walk miss] node={} probe={:?} value={} branches={:?}",
                            node_index,
                            probe,
                            value,
                            node.branches.iter().map(|b| b.value).collect::<Vec<_>>(),
                        );
                    }
                    continue;
                };
                if trace_walk {
                    eprintln!(
                        "[decision-walk hit] node={} probe={:?} value={} -> {}",
                        node_index, probe, value, edge.next_node_index,
                    );
                }
                let mut branch_trace = trace.clone();
                branch_trace.probes.push(RuntimeMatchProbe { probe, value });
                if let Some(selection) = walk_decision_tree(
                    subtable,
                    decision_tree,
                    edge.next_node_index,
                    evaluator,
                    constructor_matches,
                    branch_trace,
                ) {
                    return Some(selection);
                }
            }
            None
        }
    }
}

fn selection_constructor_slot(
    subtable: &CompiledSubtableDefinition,
    constructor: &CompiledExecutableConstructor,
    constructor_index: usize,
    entry_subtable_id: u32,
) -> Option<usize> {
    if let Some(identity) = &constructor.sla_identity {
        return Some(identity.constructor_slot);
    }
    (subtable.sla_subtable_id == 0 && entry_subtable_id == 0).then_some(constructor_index)
}

fn resolve_leaf_constructor<'a>(
    subtable: &'a CompiledSubtableDefinition,
    entry: &CompiledDecisionLeafEntry,
) -> Option<(usize, &'a CompiledExecutableConstructor)> {
    let constructor_index = if entry.subtable_id != 0 || !subtable.constructors_by_sla_id.is_empty()
    {
        if entry.subtable_id != 0
            && subtable.sla_subtable_id != 0
            && entry.subtable_id != subtable.sla_subtable_id
        {
            return None;
        }
        subtable
            .constructors_by_sla_id
            .get(&entry.constructor_id)
            .copied()
            .or_else(|| {
                subtable
                    .constructors
                    .iter()
                    .position(|constructor| constructor.constructor_id == entry.constructor_id)
            })?
    } else {
        entry.constructor_index
    };
    subtable
        .constructors
        .get(constructor_index)
        .map(|constructor| (constructor_index, constructor))
}

fn always_true_instruction_pattern() -> CompiledDisjointPattern {
    CompiledDisjointPattern::Instruction(CompiledPatternBlock {
        offset: 0,
        nonzero_size: 0,
        mask_words: Vec::new(),
        value_words: Vec::new(),
    })
}

fn disjoint_pattern_matches<E: DecisionProbeEvaluator>(
    evaluator: &E,
    pattern: &CompiledDisjointPattern,
) -> bool {
    match pattern {
        CompiledDisjointPattern::Instruction(block) => {
            pattern_block_instruction_matches(evaluator, block)
        }
        CompiledDisjointPattern::Context(block) => pattern_block_context_matches(evaluator, block),
        CompiledDisjointPattern::Combine {
            context,
            instruction,
        } => {
            pattern_block_instruction_matches(evaluator, instruction)
                && pattern_block_context_matches(evaluator, context)
        }
        CompiledDisjointPattern::Or(patterns) => patterns
            .iter()
            .any(|pattern| disjoint_pattern_matches(evaluator, pattern)),
    }
}

fn pattern_block_instruction_matches<E: DecisionProbeEvaluator>(
    evaluator: &E,
    block: &CompiledPatternBlock,
) -> bool {
    if block.nonzero_size <= 0 {
        return block.nonzero_size == 0;
    }
    for (index, mask) in block.mask_words.iter().enumerate() {
        let Ok(data) = evaluator.instruction_bytes(block.offset + (index as i32 * 4), 4) else {
            return false;
        };
        let Some(value) = block.value_words.get(index).copied() else {
            return false;
        };
        if (mask & data) != value {
            return false;
        }
    }
    true
}

fn pattern_block_context_matches<E: DecisionProbeEvaluator>(
    evaluator: &E,
    block: &CompiledPatternBlock,
) -> bool {
    if block.nonzero_size <= 0 {
        return block.nonzero_size == 0;
    }
    for (index, mask) in block.mask_words.iter().enumerate() {
        let Ok(data) = evaluator.context_bytes(block.offset + (index as i32 * 4), 4) else {
            return false;
        };
        let Some(value) = block.value_words.get(index).copied() else {
            return false;
        };
        if (mask & data) != value {
            return false;
        }
    }
    true
}
