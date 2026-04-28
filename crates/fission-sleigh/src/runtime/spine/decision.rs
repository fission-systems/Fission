use anyhow::Result;

use crate::compiler::{
    CompiledDecisionLeafEntry, CompiledDecisionProbe, CompiledDisjointPattern,
    CompiledExecutableConstructor, CompiledFrontend, CompiledPatternBlock,
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
            &subtable.decision_tree,
            &subtable.constructors,
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
    decision_tree: &'a crate::compiler::CompiledDecisionTree,
    constructors: &'a [CompiledExecutableConstructor],
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
            let mut unsupported_fallback = None;
            let trace_terminal = std::env::var_os("FISSION_TRACE_TERMINAL_VERIFY").is_some();
            let mut matched_any_pattern = false;
            let leaf_entries: Vec<CompiledDecisionLeafEntry> = if node.leaf_entries.is_empty() {
                node.leaf_constructor_indexes
                    .iter()
                    .copied()
                    .map(|constructor_index| CompiledDecisionLeafEntry {
                        constructor_index,
                        pattern: always_true_instruction_pattern(),
                    })
                    .collect()
            } else {
                node.leaf_entries.clone()
            };
            for entry in &leaf_entries {
                let matched = disjoint_pattern_matches(evaluator, &entry.pattern);
                if trace_terminal {
                    if let Some(constructor) = constructors.get(entry.constructor_index) {
                        eprintln!(
                            "[terminal-verify] ctor={} mnemonic={} source={} matched={} pattern={:?}",
                            entry.constructor_index,
                            constructor.mnemonic,
                            constructor.source,
                            matched,
                            entry.pattern
                        );
                    }
                }
                if !matched {
                    continue;
                }
                matched_any_pattern = true;
                let constructor = constructors.get(entry.constructor_index)?;
                if constructor_matches(constructor).is_ok() {
                    if constructor.runtime_ready {
                        trace.matched_leaf_pattern = Some(entry.pattern.clone());
                        return Some(RuntimeSelection {
                            constructor,
                            constructor_index: entry.constructor_index,
                            trace,
                        });
                    }
                    if unsupported_fallback.is_none() {
                        unsupported_fallback = Some((constructor, entry.constructor_index));
                    }
                }
            }
            if !matched_any_pattern {
                return None;
            }
            unsupported_fallback.map(|(constructor, constructor_index)| RuntimeSelection {
                constructor,
                constructor_index,
                trace,
            })
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
                    decision_tree,
                    constructors,
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
        let value = block.value_words.get(index).copied().unwrap_or_default();
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
        let value = block.value_words.get(index).copied().unwrap_or_default();
        if (mask & data) != value {
            return false;
        }
    }
    true
}
