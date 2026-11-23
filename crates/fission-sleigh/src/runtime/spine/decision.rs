use std::sync::Arc;

use anyhow::Result;
use smallvec::SmallVec;

use crate::compiler::{
    CompiledDecisionLeafEntry, CompiledDecisionProbe, CompiledDisjointPattern,
    CompiledExecutableConstructor, CompiledPatternBlock, CompiledSubtableDefinition,
};

/// Almost every decision probe resolves to exactly one branch value; only
/// `ContextBitSlice` probes over unknown context bits can fan out to more
/// than one candidate. Inline capacity 4 keeps the overwhelmingly common
/// single-value case (and small multi-value fan-outs) off the heap on the
/// per-instruction decode hot path, while still spilling to a heap `Vec`
/// for the rare wide fan-out.
pub type ProbeValues = SmallVec<[u8; 4]>;

pub trait DecisionProbeEvaluator {
    fn probe_values(&mut self, probe: CompiledDecisionProbe) -> Result<ProbeValues>;
    fn instruction_bytes(&self, offset: i32, size: u32) -> Result<u32>;
    fn context_bytes(&self, offset: i32, size: u32) -> Result<u32>;
}

#[derive(Debug, Clone)]
pub struct RuntimeSelection<'a> {
    pub constructor: &'a std::sync::Arc<CompiledExecutableConstructor>,
    pub constructor_index: usize,
    pub subtable_id: u32,
    pub constructor_id: u32,
    pub constructor_slot: usize,
    pub trace: RuntimeMatchTrace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMatchTrace {
    pub root_bucket: Arc<str>,
    pub probes: RuntimeMatchProbes,
    pub leaf_constructor_indexes: Vec<usize>,
    pub matched_leaf_pattern: Option<Arc<CompiledDisjointPattern>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeMatchProbe {
    pub probe: CompiledDecisionProbe,
    pub value: u8,
}

/// The selected probes in root-to-leaf order, shared by clones of the match.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeMatchProbes(Option<Arc<[RuntimeMatchProbe]>>);

impl RuntimeMatchProbes {
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    /// Owned payload estimate including the Arc's reference counts.
    pub(in crate::runtime::spine) fn retained_payload(&self, limit: usize) -> Option<usize> {
        let Some(probes) = self.0.as_deref() else {
            return Some(0);
        };
        if probes.len() > 128 {
            return None;
        }
        let bytes = std::mem::size_of_val(probes) + 2 * std::mem::size_of::<usize>();
        (bytes <= limit).then_some(bytes)
    }
}

// Reused for backtracking. A successful leaf snapshots the path into the trace.
type ProbePath = SmallVec<[RuntimeMatchProbe; 8]>;

pub fn select_constructor<'a, E, M>(
    subtable: &'a CompiledSubtableDefinition,
    table_name: &str,
    mut evaluator: E,
    mut constructor_matches: M,
) -> Result<Option<RuntimeSelection<'a>>>
where
    E: DecisionProbeEvaluator,
    M: FnMut(&CompiledExecutableConstructor) -> Result<()>,
{
    walk_decision_tree(
        subtable,
        table_name,
        subtable.decision_tree.root_node_index,
        &mut evaluator,
        &mut constructor_matches,
        &mut ProbePath::new(),
    )
}

fn walk_decision_tree<'a, E, M>(
    subtable: &'a CompiledSubtableDefinition,
    root_bucket: &str,
    node_index: usize,
    evaluator: &mut E,
    constructor_matches: &mut M,
    path: &mut ProbePath,
) -> Result<Option<RuntimeSelection<'a>>>
where
    E: DecisionProbeEvaluator,
    M: FnMut(&CompiledExecutableConstructor) -> Result<()>,
{
    let node = subtable
        .decision_tree
        .nodes
        .get(node_index)
        .ok_or_else(|| anyhow::anyhow!("decision tree node index {node_index} is out of range"))?;
    let trace_walk = crate::runtime::diagnostics::terminal_reselect_trace_enabled();
    match node.probe {
        CompiledDecisionProbe::Terminal => {
            let trace_terminal = crate::runtime::diagnostics::terminal_verify_trace_enabled();
            // `Cow` rather than an unconditional `.clone()`: this terminal
            // branch runs on every decision-tree leaf visit (i.e. every
            // constructor selection at every subtable level of every
            // instruction decode), and the common case -- `node.leaf_entries`
            // already populated -- doesn't need an owned copy at all, only
            // the one-time immutable iteration below. Only the synthesized
            // (empty-`leaf_entries`) branch genuinely produces new data that
            // has nowhere to be borrowed from.
            let leaf_entries: std::borrow::Cow<'_, [CompiledDecisionLeafEntry]> =
                if node.leaf_entries.is_empty() {
                    if subtable.sla_subtable_id != 0 || !subtable.constructors_by_sla_id.is_empty()
                    {
                        return Ok(None);
                    }
                    let mut entries = Vec::new();
                    for constructor_index in node.leaf_constructor_indexes.iter().copied() {
                        let Ok(constructor_id) = u32::try_from(constructor_index) else {
                            return Ok(None);
                        };
                        entries.push(CompiledDecisionLeafEntry {
                            subtable_id: 0,
                            constructor_id,
                            constructor_index,
                            pattern: always_true_instruction_pattern().into(),
                        });
                    }
                    std::borrow::Cow::Owned(entries)
                } else {
                    std::borrow::Cow::Borrowed(node.leaf_entries.as_slice())
                };
            let mut selected = None;
            for entry in leaf_entries.as_ref() {
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
                if !matched || constructor_matches(constructor).is_err() {
                    continue;
                }
                let candidate = (constructor_index, constructor, entry);
                if constructor.runtime_ready {
                    selected = Some(candidate);
                    break;
                }
                selected.get_or_insert(candidate);
            }
            let Some((constructor_index, constructor, entry)) = selected else {
                return Ok(None);
            };
            let constructor_slot = selection_constructor_slot(
                subtable,
                constructor,
                constructor_index,
                entry.subtable_id,
            )
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "selected constructor {} is missing SLA selection identity",
                    constructor.constructor_id
                )
            })?;
            Ok(Some(RuntimeSelection {
                constructor,
                constructor_index,
                subtable_id: entry.subtable_id,
                constructor_id: entry.constructor_id,
                constructor_slot,
                trace: RuntimeMatchTrace {
                    root_bucket: root_bucket.into(),
                    probes: RuntimeMatchProbes(
                        (!path.is_empty()).then(|| Arc::from(path.as_slice())),
                    ),
                    leaf_constructor_indexes: leaf_entries
                        .iter()
                        .map(|entry| entry.constructor_index)
                        .collect(),
                    matched_leaf_pattern: constructor.runtime_ready.then(|| entry.pattern.clone()),
                },
            }))
        }
        probe => {
            let values = evaluator.probe_values(probe)?;
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
                path.push(RuntimeMatchProbe { probe, value });
                let result = walk_decision_tree(
                    subtable,
                    root_bucket,
                    edge.next_node_index,
                    evaluator,
                    constructor_matches,
                    path,
                );
                path.pop();
                if let Some(selection) = result? {
                    return Ok(Some(selection));
                }
            }
            Ok(None)
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
) -> Option<(usize, &'a std::sync::Arc<CompiledExecutableConstructor>)> {
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
    pattern_block_matches(block, |offset, size| {
        evaluator.instruction_bytes(offset, size)
    })
}

fn pattern_block_context_matches<E: DecisionProbeEvaluator>(
    evaluator: &E,
    block: &CompiledPatternBlock,
) -> bool {
    pattern_block_matches(block, |offset, size| evaluator.context_bytes(offset, size))
}

fn pattern_block_matches(
    block: &CompiledPatternBlock,
    mut read_bytes: impl FnMut(i32, u32) -> Result<u32>,
) -> bool {
    if block.nonzero_size <= 0 {
        return block.nonzero_size == 0;
    }
    let Ok(mut remaining) = u32::try_from(block.nonzero_size) else {
        return false;
    };
    for (index, mask) in block.mask_words.iter().enumerate() {
        if remaining == 0 {
            break;
        }
        let chunk_size = remaining.min(4);
        let value_index = index;
        let Ok(word_index) = i32::try_from(index) else {
            return false;
        };
        let Some(offset_delta) = word_index.checked_mul(4) else {
            return false;
        };
        let Some(offset) = block.offset.checked_add(offset_delta) else {
            return false;
        };
        let Ok(mut data) = read_bytes(offset, chunk_size) else {
            return false;
        };
        if chunk_size < 4 {
            data <<= (4 - chunk_size) * 8;
        }
        let Some(value) = block.value_words.get(value_index).copied() else {
            return false;
        };
        if (mask & data) != value {
            return false;
        }
        remaining -= chunk_size;
    }
    remaining == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{anyhow, bail};
    use std::collections::BTreeMap;

    struct BytesEvaluator {
        instruction: Vec<u8>,
        context: Vec<u8>,
    }

    impl BytesEvaluator {
        fn read(buf: &[u8], offset: i32, size: u32) -> Result<u32> {
            if offset < 0 || size == 0 || size > 4 {
                bail!("invalid read");
            }
            let start = usize::try_from(offset).map_err(|_| anyhow!("negative offset"))?;
            let size = usize::try_from(size).map_err(|_| anyhow!("size overflow"))?;
            let end = start.checked_add(size).ok_or_else(|| anyhow!("overflow"))?;
            let bytes = buf.get(start..end).ok_or_else(|| anyhow!("out of range"))?;
            Ok(bytes
                .iter()
                .fold(0u32, |word, byte| (word << 8) | u32::from(*byte)))
        }
    }

    impl DecisionProbeEvaluator for BytesEvaluator {
        fn probe_values(&mut self, probe: CompiledDecisionProbe) -> Result<ProbeValues> {
            match probe {
                CompiledDecisionProbe::ContextBitSlice { .. } => {
                    Ok(ProbeValues::from_slice(&[0, 1]))
                }
                _ => Ok(ProbeValues::from_slice(&[0])),
            }
        }

        fn instruction_bytes(&self, offset: i32, size: u32) -> Result<u32> {
            Self::read(&self.instruction, offset, size)
        }

        fn context_bytes(&self, offset: i32, size: u32) -> Result<u32> {
            Self::read(&self.context, offset, size)
        }
    }

    #[test]
    fn decision_trace_keeps_only_the_selected_branch() {
        use crate::compiler::{CompiledDecisionEdge, CompiledDecisionNode, CompiledDecisionTree};

        let frontend = crate::compiler::compile_x86_64_frontend().expect("compile x86-64 frontend");
        let constructor = frontend.subtables["instruction"]
            .constructors
            .iter()
            .find(|constructor| constructor.runtime_ready)
            .expect("executable constructor");
        let probe = CompiledDecisionProbe::ContextBitSlice {
            offset: 0,
            mask: 1,
            shift: 0,
        };
        let miss = CompiledDecisionNode {
            probe: CompiledDecisionProbe::Terminal,
            branches: Vec::new(),
            leaf_constructor_indexes: Vec::new(),
            leaf_entries: Vec::new(),
        };
        let mut subtable = subtable_with_tree(CompiledDecisionTree {
            root_node_index: 0,
            root_buckets: Vec::new(),
            decision_node_count: 3,
            nodes: vec![
                CompiledDecisionNode {
                    probe,
                    branches: vec![
                        CompiledDecisionEdge {
                            value: 0,
                            next_node_index: 1,
                        },
                        CompiledDecisionEdge {
                            value: 1,
                            next_node_index: 2,
                        },
                    ],
                    ..miss.clone()
                },
                miss.clone(),
                CompiledDecisionNode {
                    leaf_constructor_indexes: vec![0],
                    ..miss
                },
            ],
        });
        subtable.constructors.push(constructor.clone());

        let selection = select_constructor(
            &subtable,
            "instruction",
            BytesEvaluator {
                instruction: Vec::new(),
                context: Vec::new(),
            },
            |_| Ok(()),
        )
        .expect("decision walk")
        .expect("second branch match");

        assert_eq!(
            selection.trace.probes.0.as_deref(),
            Some(&[RuntimeMatchProbe { probe, value: 1 }][..])
        );
        assert_eq!(selection.trace.leaf_constructor_indexes, [0]);
    }

    struct FailingProbeEvaluator;

    impl DecisionProbeEvaluator for FailingProbeEvaluator {
        fn probe_values(&mut self, _probe: CompiledDecisionProbe) -> Result<ProbeValues> {
            bail!("synthetic probe failure")
        }

        fn instruction_bytes(&self, _offset: i32, _size: u32) -> Result<u32> {
            bail!("unexpected instruction read")
        }

        fn context_bytes(&self, _offset: i32, _size: u32) -> Result<u32> {
            bail!("unexpected context read")
        }
    }

    fn subtable_with_tree(
        decision_tree: crate::compiler::CompiledDecisionTree,
    ) -> CompiledSubtableDefinition {
        CompiledSubtableDefinition {
            name: "instruction".to_string(),
            sla_subtable_id: 0,
            constructors_by_sla_id: BTreeMap::new(),
            constructors: Vec::new(),
            decision_tree,
        }
    }

    #[test]
    fn decision_probe_evaluator_errors_propagate() {
        let subtable = subtable_with_tree(crate::compiler::CompiledDecisionTree {
            root_node_index: 0,
            root_buckets: Vec::new(),
            nodes: vec![crate::compiler::CompiledDecisionNode {
                probe: CompiledDecisionProbe::InstructionBitSlice {
                    offset: 0,
                    mask: 0xff,
                    shift: 0,
                },
                branches: Vec::new(),
                leaf_constructor_indexes: Vec::new(),
                leaf_entries: Vec::new(),
            }],
            decision_node_count: 1,
        });

        let error = select_constructor(&subtable, "instruction", FailingProbeEvaluator, |_| Ok(()))
            .expect_err("decision probe evaluator errors must fail closed");

        assert!(
            error.to_string().contains("synthetic probe failure"),
            "{error:#}"
        );
    }

    #[test]
    fn terminal_pattern_matches_short_instruction_word_prefix() {
        let evaluator = BytesEvaluator {
            instruction: vec![0xc3],
            context: Vec::new(),
        };
        let pattern = CompiledPatternBlock {
            offset: 0,
            nonzero_size: 1,
            mask_words: vec![0xff00_0000],
            value_words: vec![0xc300_0000],
        };

        assert!(pattern_block_instruction_matches(&evaluator, &pattern));
    }

    #[test]
    fn terminal_pattern_matches_partial_final_word_prefix() {
        let evaluator = BytesEvaluator {
            instruction: vec![0x48, 0x8d, 0x84, 0x24, 0x80, 0xff],
            context: Vec::new(),
        };
        let pattern = CompiledPatternBlock {
            offset: 0,
            nonzero_size: 6,
            mask_words: vec![0xffff_ffff, 0xffff_0000],
            value_words: vec![0x488d_8424, 0x80ff_0000],
        };

        assert!(pattern_block_instruction_matches(&evaluator, &pattern));
    }
}
