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
) -> Result<Option<RuntimeSelection<'a>>>
where
    E: DecisionProbeEvaluator,
    M: FnMut(&CompiledExecutableConstructor) -> Result<()>,
{
    for (table_name, root_node_index) in roots {
        let subtable = compiled
            .subtables
            .get(&table_name)
            .ok_or_else(|| anyhow::anyhow!("missing subtable {table_name} for decision root"))?;
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
        )? {
            return Ok(Some(selection));
        }
    }
    Ok(None)
}

fn walk_decision_tree<'a, E, M>(
    subtable: &'a CompiledSubtableDefinition,
    decision_tree: &'a crate::compiler::CompiledDecisionTree,
    node_index: usize,
    evaluator: &mut E,
    constructor_matches: &mut M,
    mut trace: RuntimeMatchTrace,
) -> Result<Option<RuntimeSelection<'a>>>
where
    E: DecisionProbeEvaluator,
    M: FnMut(&CompiledExecutableConstructor) -> Result<()>,
{
    let node = decision_tree
        .nodes
        .get(node_index)
        .ok_or_else(|| anyhow::anyhow!("decision tree node index {node_index} is out of range"))?;
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
                        pattern: always_true_instruction_pattern(),
                    });
                }
                entries
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
                        )
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "selected constructor {} is missing SLA selection identity",
                                constructor.constructor_id
                            )
                        })?;
                        return Ok(Some(RuntimeSelection {
                            constructor,
                            constructor_index,
                            subtable_id: entry.subtable_id,
                            constructor_id: entry.constructor_id,
                            constructor_slot,
                            trace,
                        }));
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
                return Ok(None);
            }
            first_unsupported_match
                .map(
                    |(constructor, constructor_index, subtable_id, constructor_id)| {
                        let constructor_slot = selection_constructor_slot(
                            subtable,
                            constructor,
                            constructor_index,
                            subtable_id,
                        )
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "selected constructor {} is missing SLA selection identity",
                                constructor.constructor_id
                            )
                        })?;
                        Ok(RuntimeSelection {
                            constructor,
                            constructor_index,
                            subtable_id,
                            constructor_id,
                            constructor_slot,
                            trace,
                        })
                    },
                )
                .transpose()
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
                let mut branch_trace = trace.clone();
                branch_trace.probes.push(RuntimeMatchProbe { probe, value });
                if let Some(selection) = walk_decision_tree(
                    subtable,
                    decision_tree,
                    edge.next_node_index,
                    evaluator,
                    constructor_matches,
                    branch_trace,
                )? {
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
        fn probe_values(&mut self, _probe: CompiledDecisionProbe) -> Result<Vec<u8>> {
            Ok(vec![0])
        }

        fn instruction_bytes(&self, offset: i32, size: u32) -> Result<u32> {
            Self::read(&self.instruction, offset, size)
        }

        fn context_bytes(&self, offset: i32, size: u32) -> Result<u32> {
            Self::read(&self.context, offset, size)
        }
    }

    struct FailingProbeEvaluator;

    impl DecisionProbeEvaluator for FailingProbeEvaluator {
        fn probe_values(&mut self, _probe: CompiledDecisionProbe) -> Result<Vec<u8>> {
            bail!("synthetic probe failure")
        }

        fn instruction_bytes(&self, _offset: i32, _size: u32) -> Result<u32> {
            bail!("unexpected instruction read")
        }

        fn context_bytes(&self, _offset: i32, _size: u32) -> Result<u32> {
            bail!("unexpected context read")
        }
    }

    fn minimal_frontend_with_tree(
        decision_tree: crate::compiler::CompiledDecisionTree,
    ) -> CompiledFrontend {
        let mut subtables = BTreeMap::new();
        subtables.insert(
            "instruction".to_string(),
            CompiledSubtableDefinition {
                name: "instruction".to_string(),
                sla_subtable_id: 0,
                constructors_by_sla_id: BTreeMap::new(),
                constructors: Vec::new(),
                decision_tree,
            },
        );
        CompiledFrontend {
            arch: "test".to_string(),
            default_context: 0,
            default_context_known_mask: 0,
            entry_spec: "test.slaspec".to_string(),
            entry_id: "test".to_string(),
            include_manifest: Vec::new(),
            defines: Vec::new(),
            definitions: Vec::new(),
            macros: Vec::new(),
            constructors: Vec::new(),
            subtables,
            language_layout: crate::compiler::CompiledLanguageLayout {
                address_spaces: Vec::new(),
                registers: Vec::new(),
                token_fields: Vec::new(),
                context_fields: Vec::new(),
                subtables: Vec::new(),
                display_templates: Vec::new(),
            },
            construct_templates: Vec::new(),
            pcode_ops: Vec::new(),
            pattern_nodes: Vec::new(),
            sla_spaces: BTreeMap::new(),
            sla_unique_space_index: 0,
            sla_register_space_index: 0,
            sla_uniqbase: 0,
            sla_uniqmask: u64::MAX,
        }
    }

    #[test]
    fn decision_probe_evaluator_errors_propagate() {
        let compiled = minimal_frontend_with_tree(crate::compiler::CompiledDecisionTree {
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

        let error = select_constructor(
            &compiled,
            [("instruction".to_string(), 0)],
            || FailingProbeEvaluator,
            |_| Ok(()),
        )
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
