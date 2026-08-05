//! Guarded scalar SSA and conservative out-of-SSA construction.
//!
//! This runs on the original lifted CFG, before structuring removes
//! irreducible edges. Register and unique varnodes are refined into disjoint
//! overlapping-storage pieces. Typed dynamic effects feed exact RAM/stack
//! promotion, and block/instruction covers guard speculative HighVariable
//! COPY/CAST congruence.

use super::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ScalarSsaValidationError {
    Shape(ScalarSsaShapeError),
    AddressSpaceGuardsMismatch,
    StoragePartitionsMismatch,
    MissingOperationOutput(SsaOpSite),
    MissingOperationInput(SsaUseSite),
    OperationPieceCount {
        site: SsaOpSite,
        expected: usize,
        actual: usize,
    },
    UsePieceCount {
        site: SsaUseSite,
        expected: usize,
        actual: usize,
    },
    OperationStorageMismatch {
        site: SsaOpSite,
        expected: SsaStorageKey,
        actual: SsaStorageKey,
    },
    UseStorageMismatch {
        site: SsaUseSite,
        expected: SsaStorageKey,
        actual: SsaStorageKey,
    },
    PhiPredecessors {
        block: u32,
        expected: Vec<u32>,
        actual: Vec<u32>,
    },
    PhiStorageMismatch {
        block: u32,
        predecessor: u32,
        expected: SsaStorageKey,
        actual: SsaStorageKey,
    },
    NonDominatingUse {
        site: SsaUseSite,
        value: SsaValueId,
    },
    NonDominatingPhiOperand {
        block: u32,
        predecessor: u32,
        value: SsaValueId,
    },
    DynamicGuardsMismatch,
    OutOfSsaCopiesMismatch,
    HighVariablesMismatch,
}

#[derive(Debug)]
struct Dominance {
    reachable: BTreeSet<usize>,
    dominators: Vec<BTreeSet<usize>>,
    children: Vec<Vec<usize>>,
    frontier: Vec<BTreeSet<usize>>,
}

impl Dominance {
    fn analyze(successors: &[Vec<usize>], predecessors: &[Vec<usize>]) -> Self {
        let block_count = successors.len();
        let mut reachable = BTreeSet::new();
        if block_count != 0 {
            let mut queue = VecDeque::from([0]);
            while let Some(block) = queue.pop_front() {
                if block >= block_count || !reachable.insert(block) {
                    continue;
                }
                for &successor in &successors[block] {
                    if successor < block_count {
                        queue.push_back(successor);
                    }
                }
            }
        }

        let mut dominators = vec![BTreeSet::new(); block_count];
        for &block in &reachable {
            if block == 0 {
                dominators[block].insert(block);
            } else {
                dominators[block] = reachable.clone();
            }
        }

        loop {
            let mut changed = false;
            for &block in reachable.iter().filter(|&&block| block != 0) {
                let reachable_predecessors: Vec<usize> = predecessors
                    .get(block)
                    .into_iter()
                    .flatten()
                    .copied()
                    .filter(|predecessor| reachable.contains(predecessor))
                    .collect();
                let mut next = if let Some((&first, rest)) = reachable_predecessors.split_first() {
                    let mut intersection = dominators[first].clone();
                    for predecessor in rest {
                        intersection
                            .retain(|candidate| dominators[*predecessor].contains(candidate));
                    }
                    intersection
                } else {
                    BTreeSet::new()
                };
                next.insert(block);
                if next != dominators[block] {
                    dominators[block] = next;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        let mut idom = vec![None; block_count];
        for &block in reachable.iter().filter(|&&block| block != 0) {
            idom[block] = dominators[block]
                .iter()
                .copied()
                .filter(|candidate| *candidate != block)
                .max_by_key(|candidate| (dominators[*candidate].len(), *candidate));
        }

        let mut children = vec![Vec::new(); block_count];
        for (block, parent) in idom.iter().copied().enumerate() {
            if let Some(parent) = parent {
                children[parent].push(block);
            }
        }
        for block_children in &mut children {
            block_children.sort_unstable();
        }

        let mut frontier = vec![BTreeSet::new(); block_count];
        for &join in &reachable {
            let join_predecessors: Vec<usize> = predecessors
                .get(join)
                .into_iter()
                .flatten()
                .copied()
                .filter(|predecessor| reachable.contains(predecessor))
                .collect();
            if join_predecessors.len() < 2 {
                continue;
            }
            for predecessor in join_predecessors {
                let mut runner = Some(predecessor);
                while runner != idom[join] {
                    let Some(block) = runner else {
                        break;
                    };
                    frontier[block].insert(join);
                    runner = idom[block];
                }
            }
        }

        Self {
            reachable,
            dominators,
            children,
            frontier,
        }
    }

    fn dominates(&self, definition: usize, use_block: usize) -> bool {
        self.dominators
            .get(use_block)
            .is_some_and(|dominators| dominators.contains(&definition))
    }
}

#[derive(Debug, Clone, Copy)]
struct ScalarRange {
    kind: SsaAddressSpaceKind,
    space_id: u64,
    start: u64,
    end_exclusive: u64,
}

fn guarded_scalar_range(varnode: &Varnode) -> Option<ScalarRange> {
    if varnode.is_constant || varnode.size == 0 {
        return None;
    }
    let kind = if is_register_space_id(varnode.space_id) {
        SsaAddressSpaceKind::Register
    } else if is_unique_space_id(varnode.space_id) {
        SsaAddressSpaceKind::Unique
    } else {
        return None;
    };
    let end_exclusive = varnode.offset.checked_add(u64::from(varnode.size))?;
    Some(ScalarRange {
        kind,
        space_id: varnode.space_id,
        start: varnode.offset,
        end_exclusive,
    })
}

#[derive(Debug, Default)]
struct StorageLayout {
    guards: BTreeMap<u64, SsaAddressSpaceGuard>,
    partitions: BTreeMap<u64, Vec<SsaStorageKey>>,
}

impl StorageLayout {
    fn build(pcode: &PcodeFunction, reachable: &BTreeSet<usize>) -> Self {
        let mut ranges: BTreeMap<u64, (SsaAddressSpaceKind, Vec<(u64, u64)>)> = BTreeMap::new();
        for &block in reachable {
            let Some(pcode_block) = pcode.blocks.get(block) else {
                continue;
            };
            for op in &pcode_block.ops {
                for varnode in op.inputs.iter().chain(op.output.iter()) {
                    let Some(range) = guarded_scalar_range(varnode) else {
                        continue;
                    };
                    let entry = ranges
                        .entry(range.space_id)
                        .or_insert_with(|| (range.kind, Vec::new()));
                    debug_assert_eq!(entry.0, range.kind);
                    entry.1.push((range.start, range.end_exclusive));
                }
            }
        }

        let mut layout = Self::default();
        for (space_id, (kind, mut space_ranges)) in ranges {
            space_ranges.sort_unstable();
            let observed_start = space_ranges
                .iter()
                .map(|(start, _)| *start)
                .min()
                .expect("space has a range");
            let observed_end_exclusive = space_ranges
                .iter()
                .map(|(_, end)| *end)
                .max()
                .expect("space has a range");
            layout.guards.insert(
                space_id,
                SsaAddressSpaceGuard {
                    kind,
                    observed_start,
                    observed_end_exclusive,
                },
            );

            let mut boundaries = BTreeSet::new();
            for &(start, end) in &space_ranges {
                boundaries.insert(start);
                boundaries.insert(end);
            }
            let boundaries = boundaries.into_iter().collect::<Vec<_>>();
            let mut covered_ranges: Vec<(u64, u64)> = Vec::new();
            for &(start, end) in &space_ranges {
                if let Some((_, covered_end)) = covered_ranges.last_mut()
                    && start <= *covered_end
                {
                    *covered_end = (*covered_end).max(end);
                } else {
                    covered_ranges.push((start, end));
                }
            }
            let mut partitions = Vec::new();
            let mut covered_index = 0;
            for boundary_pair in boundaries.windows(2) {
                let start = boundary_pair[0];
                let end = boundary_pair[1];
                while covered_index < covered_ranges.len()
                    && covered_ranges[covered_index].1 <= start
                {
                    covered_index += 1;
                }
                if covered_index == covered_ranges.len()
                    || covered_ranges[covered_index].0 > start
                    || end > covered_ranges[covered_index].1
                {
                    continue;
                }
                let size = u32::try_from(end - start)
                    .expect("partition is bounded by an observed u32-sized varnode");
                partitions.push(SsaStorageKey {
                    space_id,
                    offset: start,
                    size,
                });
            }
            layout.partitions.insert(space_id, partitions);
        }
        layout
    }

    fn storages(&self) -> impl Iterator<Item = SsaStorageKey> + '_ {
        self.partitions.values().flatten().copied()
    }

    fn access_pieces(&self, varnode: &Varnode) -> Vec<(u32, SsaStorageKey)> {
        let Some(range) = guarded_scalar_range(varnode) else {
            return Vec::new();
        };
        let Some(partitions) = self.partitions.get(&range.space_id) else {
            return Vec::new();
        };
        let start_index = partitions.partition_point(|storage| storage.offset < range.start);
        partitions[start_index..]
            .iter()
            .take_while(|storage| storage.offset < range.end_exclusive)
            .filter(|storage| {
                let end = storage.offset + u64::from(storage.size);
                range.start <= storage.offset && end <= range.end_exclusive
            })
            .map(|storage| {
                (
                    u32::try_from(storage.offset - range.start)
                        .expect("piece lies within a u32-sized varnode"),
                    *storage,
                )
            })
            .collect()
    }
}

fn allocate_value(
    ssa: &mut NirScalarSsa,
    storage: SsaStorageKey,
    definition: SsaValueDefinition,
) -> SsaValueId {
    let id = SsaValueId(ssa.values.len() as u32);
    ssa.values.push(SsaValue {
        id,
        storage,
        definition,
    });
    id
}

pub(super) fn build_scalar_ssa(
    pcode: &PcodeFunction,
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
) -> NirScalarSsa {
    build_scalar_ssa_with_context(
        pcode,
        successors,
        predecessors,
        &MlilPreviewOptions::default(),
        None,
    )
}

pub(super) fn build_scalar_ssa_with_context(
    pcode: &PcodeFunction,
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
    options: &MlilPreviewOptions,
    type_context: Option<&PreviewTypeContext>,
) -> NirScalarSsa {
    let dominance = Dominance::analyze(successors, predecessors);
    if pcode.blocks.is_empty() || dominance.reachable.is_empty() {
        return NirScalarSsa::default();
    }

    let layout = StorageLayout::build(pcode, &dominance.reachable);
    let mut definition_blocks: BTreeMap<SsaStorageKey, BTreeSet<usize>> = BTreeMap::new();
    for &block in &dominance.reachable {
        let Some(pcode_block) = pcode.blocks.get(block) else {
            continue;
        };
        for op in &pcode_block.ops {
            if let Some(output) = op.output.as_ref() {
                for (_, storage) in layout.access_pieces(output) {
                    definition_blocks.entry(storage).or_default().insert(block);
                }
            }
        }
    }
    // Formal inputs are definitions at entry. They must participate in phi
    // placement when only one branch arm overwrites a partition.
    for storage in layout.storages() {
        definition_blocks.entry(storage).or_default().insert(0);
    }

    let mut phi_plan: BTreeMap<usize, BTreeSet<SsaStorageKey>> = BTreeMap::new();
    for (&storage, defining_blocks) in &definition_blocks {
        let mut work = defining_blocks.clone();
        let mut placed = BTreeSet::new();
        while let Some(block) = work.pop_first() {
            for &frontier_block in &dominance.frontier[block] {
                if !placed.insert(frontier_block) {
                    continue;
                }
                phi_plan.entry(frontier_block).or_default().insert(storage);
                if !defining_blocks.contains(&frontier_block) {
                    work.insert(frontier_block);
                }
            }
        }
    }

    let mut ssa = NirScalarSsa {
        address_spaces: layout.guards.clone(),
        ..NirScalarSsa::default()
    };
    let mut stacks: BTreeMap<SsaStorageKey, Vec<SsaValueId>> = BTreeMap::new();
    for storage in layout.storages() {
        let input = allocate_value(&mut ssa, storage, SsaValueDefinition::Input);
        ssa.inputs.insert(storage, input);
        stacks.insert(storage, vec![input]);
    }

    let mut phi_outputs = BTreeMap::new();
    for (&block, block_storages) in &phi_plan {
        for &storage in block_storages {
            let output = allocate_value(
                &mut ssa,
                storage,
                SsaValueDefinition::Phi {
                    block: block as u32,
                },
            );
            phi_outputs.insert((block, storage), output);
        }
    }

    for &block in &dominance.reachable {
        let Some(pcode_block) = pcode.blocks.get(block) else {
            continue;
        };
        for (op_index, op) in pcode_block.ops.iter().enumerate() {
            let Some(output_varnode) = op.output.as_ref() else {
                continue;
            };
            let output_storages = layout.access_pieces(output_varnode);
            if output_storages.is_empty() {
                continue;
            }
            let site = SsaOpSite {
                block: block as u32,
                op: op_index as u32,
            };
            let mut pieces = Vec::with_capacity(output_storages.len());
            for (byte_offset, storage) in output_storages {
                let output = allocate_value(&mut ssa, storage, SsaValueDefinition::Operation(site));
                pieces.push(SsaAccessPiece {
                    byte_offset,
                    value: output,
                });
            }
            ssa.operation_outputs.insert(site, pieces);
        }
    }

    let mut phi_operands: BTreeMap<(usize, SsaStorageKey), Vec<SsaPhiOperand>> = BTreeMap::new();
    rename_block(
        0,
        pcode,
        successors,
        &dominance,
        &layout,
        &phi_plan,
        &phi_outputs,
        &mut stacks,
        &mut ssa,
        &mut phi_operands,
    );

    for (block, block_storages) in phi_plan {
        let mut phis = Vec::with_capacity(block_storages.len());
        for storage in block_storages {
            let mut operands = phi_operands.remove(&(block, storage)).unwrap_or_default();
            operands.sort_unstable_by_key(|operand| operand.predecessor);
            phis.push(NirPhiNode {
                storage,
                output: phi_outputs[&(block, storage)],
                operands,
            });
        }
        ssa.phis.insert(block as u32, phis);
    }

    ssa.dynamic_guards =
        build_dynamic_guards(pcode, &dominance.reachable, &ssa, options, type_context);
    build_memory_ssa(pcode, successors, &dominance, &mut ssa);
    let (out_of_ssa_copies, high_variables, value_high_variables) =
        build_out_of_ssa_facts(pcode, &ssa, successors, predecessors, &dominance);
    ssa.out_of_ssa_copies = out_of_ssa_copies;
    ssa.high_variables = high_variables;
    ssa.value_high_variables = value_high_variables;
    let (memory_high_variables, value_memory_high_variables) =
        build_memory_out_of_ssa_facts(pcode, &ssa, predecessors, &dominance);
    ssa.memory_high_variables = memory_high_variables;
    ssa.value_memory_high_variables = value_memory_high_variables;

    ssa
}

#[allow(clippy::too_many_arguments)]
fn rename_block(
    block: usize,
    pcode: &PcodeFunction,
    successors: &[Vec<usize>],
    dominance: &Dominance,
    layout: &StorageLayout,
    phi_plan: &BTreeMap<usize, BTreeSet<SsaStorageKey>>,
    phi_outputs: &BTreeMap<(usize, SsaStorageKey), SsaValueId>,
    stacks: &mut BTreeMap<SsaStorageKey, Vec<SsaValueId>>,
    ssa: &mut NirScalarSsa,
    phi_operands: &mut BTreeMap<(usize, SsaStorageKey), Vec<SsaPhiOperand>>,
) {
    let mut pushed = Vec::new();
    if let Some(block_storages) = phi_plan.get(&block) {
        for &storage in block_storages {
            stacks
                .get_mut(&storage)
                .expect("phi storage has an input value")
                .push(phi_outputs[&(block, storage)]);
            pushed.push(storage);
        }
    }

    if let Some(pcode_block) = pcode.blocks.get(block) {
        for (op_index, op) in pcode_block.ops.iter().enumerate() {
            for (input_index, input) in op.inputs.iter().enumerate() {
                let input_storages = layout.access_pieces(input);
                if input_storages.is_empty() {
                    continue;
                }
                let mut pieces = Vec::with_capacity(input_storages.len());
                for (byte_offset, storage) in input_storages {
                    let value = *stacks
                        .get(&storage)
                        .and_then(|stack| stack.last())
                        .expect("eligible storage has an input value");
                    pieces.push(SsaAccessPiece { byte_offset, value });
                }
                ssa.operation_inputs.insert(
                    SsaUseSite {
                        block: block as u32,
                        op: op_index as u32,
                        input: input_index as u32,
                    },
                    pieces,
                );
            }

            let site = SsaOpSite {
                block: block as u32,
                op: op_index as u32,
            };
            if let Some(outputs) = ssa.operation_outputs.get(&site).cloned() {
                for piece in outputs {
                    let storage = ssa.value(piece.value).expect("allocated output").storage;
                    stacks
                        .get_mut(&storage)
                        .expect("output storage has an input value")
                        .push(piece.value);
                    pushed.push(storage);
                }
            }
        }
    }

    for &successor in successors.get(block).into_iter().flatten() {
        let Some(successor_storages) = phi_plan.get(&successor) else {
            continue;
        };
        for &storage in successor_storages {
            let value = *stacks
                .get(&storage)
                .and_then(|stack| stack.last())
                .expect("phi storage has a reaching value");
            phi_operands
                .entry((successor, storage))
                .or_default()
                .push(SsaPhiOperand {
                    predecessor: block as u32,
                    value,
                });
        }
    }

    for &child in &dominance.children[block] {
        rename_block(
            child,
            pcode,
            successors,
            dominance,
            layout,
            phi_plan,
            phi_outputs,
            stacks,
            ssa,
            phi_operands,
        );
    }

    for storage in pushed.into_iter().rev() {
        let popped = stacks
            .get_mut(&storage)
            .expect("pushed storage remains present")
            .pop();
        debug_assert!(popped.is_some());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PointerRange {
    region: SsaMemoryRegion,
    minimum: i128,
    maximum: i128,
    step: u64,
    exact: bool,
}

impl PointerRange {
    fn exact(region: SsaMemoryRegion, offset: i128) -> Self {
        Self {
            region,
            minimum: offset,
            maximum: offset,
            step: 0,
            exact: true,
        }
    }

    fn shifted(self, delta: i64) -> Option<Self> {
        let shift = |value: i128| value.checked_add(i128::from(delta));
        Some(Self {
            minimum: shift(self.minimum)?,
            maximum: shift(self.maximum)?,
            ..self
        })
    }

    fn union(self, other: Self) -> Option<Self> {
        if self.region != other.region {
            return None;
        }
        if self.exact && other.exact {
            let minimum = self.minimum.min(other.minimum);
            let maximum = self.maximum.max(other.maximum);
            Some(Self {
                region: self.region,
                minimum,
                maximum,
                step: u64::try_from(maximum - minimum).unwrap_or(0),
                exact: minimum == maximum,
            })
        } else {
            Some(Self {
                region: self.region,
                minimum: self.minimum.min(other.minimum),
                maximum: self.maximum.max(other.maximum),
                step: 0,
                exact: false,
            })
        }
    }
}

fn build_dynamic_guards(
    pcode: &PcodeFunction,
    reachable: &BTreeSet<usize>,
    ssa: &NirScalarSsa,
    options: &MlilPreviewOptions,
    type_context: Option<&PreviewTypeContext>,
) -> BTreeMap<SsaOpSite, SsaDynamicGuard> {
    let mut guards = BTreeMap::new();
    for &block in reachable {
        let Some(pcode_block) = pcode.blocks.get(block) else {
            continue;
        };
        for (op_index, op) in pcode_block.ops.iter().enumerate() {
            let site = SsaOpSite {
                block: block as u32,
                op: op_index as u32,
            };
            let guard = match op.opcode {
                PcodeOpcode::Load if op.inputs.len() >= 2 => memory_guard(
                    pcode,
                    ssa,
                    options,
                    site,
                    SsaDynamicGuardKind::Load,
                    op.inputs.first().and_then(encoded_space_id),
                    1,
                    op.output.as_ref().map_or(0, |output| output.size),
                ),
                PcodeOpcode::Store if op.inputs.len() >= 2 => {
                    let (space_id, pointer_input) = if op.inputs.len() >= 3 {
                        (op.inputs.first().and_then(encoded_space_id), 1)
                    } else {
                        (None, 0)
                    };
                    let width = op.inputs.last().map_or(0, |value| value.size);
                    memory_guard(
                        pcode,
                        ssa,
                        options,
                        site,
                        SsaDynamicGuardKind::Store,
                        space_id,
                        pointer_input,
                        width,
                    )
                }
                PcodeOpcode::Call | PcodeOpcode::CallInd => {
                    call_guard(op, site, options, type_context)
                }
                _ => continue,
            };
            guards.insert(site, guard);
        }
    }
    guards
}

fn call_guard(
    op: &PcodeOp,
    site: SsaOpSite,
    options: &MlilPreviewOptions,
    type_context: Option<&PreviewTypeContext>,
) -> SsaDynamicGuard {
    let target = resolve_direct_call_target(op, options, type_context);
    let summary = target
        .as_ref()
        .and_then(|target| type_context?.call_effect_summaries.get(target));
    let effect = if summary.is_some_and(|summary| summary.may_call_unknown == Some(false)) {
        let summary = summary.expect("closed summary was checked");
        match (
            summary.reads_memory.unwrap_or(true),
            summary.writes_memory.unwrap_or(true),
        ) {
            (false, false) => SsaMemoryEffect::None,
            (true, false) => SsaMemoryEffect::Read,
            (false, true) => SsaMemoryEffect::Write,
            (true, true) => SsaMemoryEffect::ReadWrite,
        }
    } else {
        SsaMemoryEffect::ReadWrite
    };
    SsaDynamicGuard {
        site,
        kind: SsaDynamicGuardKind::Call,
        effect,
        region: None,
        space_id: None,
        minimum_offset: 0,
        maximum_offset_exclusive: None,
        step: 0,
        precision: SsaGuardRangePrecision::Unknown,
        call_target: target,
        call_effect_source: summary.and_then(|summary| summary.source.clone()),
    }
}

fn resolve_direct_call_target(
    op: &PcodeOp,
    options: &MlilPreviewOptions,
    type_context: Option<&PreviewTypeContext>,
) -> Option<String> {
    if let Some(symbol) = options.relocation_names.get(&op.address) {
        return Some(symbol.clone());
    }
    let address = op
        .inputs
        .first()
        .filter(|target| target.is_constant)?
        .offset;
    let context = type_context?;
    if context.ambiguous_call_targets.contains(&address) {
        return None;
    }
    context
        .call_target_refs
        .get(&address)
        .or_else(|| context.iat_target_refs.get(&address))
        .map(|target| target.symbol.clone())
        .or_else(|| context.call_targets.get(&address).cloned())
}

fn encoded_space_id(varnode: &Varnode) -> Option<u64> {
    varnode.is_constant.then_some(varnode.offset)
}

fn memory_guard(
    pcode: &PcodeFunction,
    ssa: &NirScalarSsa,
    options: &MlilPreviewOptions,
    site: SsaOpSite,
    kind: SsaDynamicGuardKind,
    space_id: Option<u64>,
    pointer_input: usize,
    width: u32,
) -> SsaDynamicGuard {
    let effect = match kind {
        SsaDynamicGuardKind::Load => SsaMemoryEffect::Read,
        SsaDynamicGuardKind::Store => SsaMemoryEffect::Write,
        SsaDynamicGuardKind::Call => unreachable!("calls do not have pointer ranges"),
    };
    let resolved = if width == 0 || space_id.is_none() {
        None
    } else {
        resolve_pointer_input(
            pcode,
            ssa,
            options,
            site,
            pointer_input,
            &mut BTreeSet::new(),
            64,
        )
    };
    let Some(range) = resolved else {
        return SsaDynamicGuard {
            site,
            kind,
            effect,
            region: None,
            space_id,
            minimum_offset: 0,
            maximum_offset_exclusive: None,
            step: 0,
            precision: SsaGuardRangePrecision::Unknown,
            call_target: None,
            call_effect_source: None,
        };
    };
    let Some(maximum_offset_exclusive) = range.maximum.checked_add(i128::from(width)) else {
        return SsaDynamicGuard {
            site,
            kind,
            effect,
            region: None,
            space_id,
            minimum_offset: 0,
            maximum_offset_exclusive: None,
            step: 0,
            precision: SsaGuardRangePrecision::Unknown,
            call_target: None,
            call_effect_source: None,
        };
    };
    SsaDynamicGuard {
        site,
        kind,
        effect,
        region: Some(range.region),
        space_id,
        minimum_offset: range.minimum,
        maximum_offset_exclusive: Some(maximum_offset_exclusive),
        step: range.step,
        precision: if range.exact {
            SsaGuardRangePrecision::Exact
        } else {
            SsaGuardRangePrecision::Bounded
        },
        call_target: None,
        call_effect_source: None,
    }
}

fn resolve_pointer_input(
    pcode: &PcodeFunction,
    ssa: &NirScalarSsa,
    options: &MlilPreviewOptions,
    site: SsaOpSite,
    input_index: usize,
    visiting: &mut BTreeSet<SsaValueId>,
    budget: usize,
) -> Option<PointerRange> {
    if budget == 0 {
        return None;
    }
    let op = pcode
        .blocks
        .get(site.block as usize)?
        .ops
        .get(site.op as usize)?;
    let input = op.inputs.get(input_index)?;
    if input.is_constant {
        return Some(PointerRange::exact(
            SsaMemoryRegion::Ram,
            i128::from(input.offset),
        ));
    }
    let pieces = ssa.operation_inputs.get(&SsaUseSite {
        block: site.block,
        op: site.op,
        input: input_index as u32,
    })?;
    if pieces.len() != 1 {
        return None;
    }
    let value = ssa.value(pieces[0].value)?;
    if value.storage.size != input.size {
        return None;
    }
    resolve_pointer_value(pcode, ssa, options, value.id, visiting, budget - 1)
}

fn resolve_pointer_value(
    pcode: &PcodeFunction,
    ssa: &NirScalarSsa,
    options: &MlilPreviewOptions,
    value_id: SsaValueId,
    visiting: &mut BTreeSet<SsaValueId>,
    budget: usize,
) -> Option<PointerRange> {
    if budget == 0 || !visiting.insert(value_id) {
        return None;
    }
    let value = ssa.value(value_id)?;
    let resolved = match value.definition {
        SsaValueDefinition::Input => (is_register_space_id(value.storage.space_id)
            && (options.cspec_stack_pointer_offset == Some(value.storage.offset)
                || options
                    .calling_convention
                    .native_stack_pointer_register_offset(options.is_64bit)
                    == Some(value.storage.offset)))
        .then(|| PointerRange::exact(SsaMemoryRegion::Stack, 0)),
        SsaValueDefinition::Phi { block } => {
            let phi = ssa
                .phis
                .get(&block)?
                .iter()
                .find(|phi| phi.output == value_id)?;
            let mut range: Option<PointerRange> = None;
            for operand in &phi.operands {
                let operand_range = resolve_pointer_value(
                    pcode,
                    ssa,
                    options,
                    operand.value,
                    visiting,
                    budget - 1,
                )?;
                range = Some(match range {
                    None => operand_range,
                    Some(current) => current.union(operand_range)?,
                });
            }
            range
        }
        SsaValueDefinition::Operation(site) => {
            let op = pcode
                .blocks
                .get(site.block as usize)?
                .ops
                .get(site.op as usize)?;
            let outputs = ssa.operation_outputs.get(&site)?;
            if outputs.len() != 1 || outputs[0].value != value_id {
                None
            } else {
                match op.opcode {
                    PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt => {
                        resolve_pointer_input(pcode, ssa, options, site, 0, visiting, budget - 1)
                    }
                    PcodeOpcode::IntAdd if op.inputs.len() == 2 => {
                        if let Some(delta) = signed_constant_delta(&op.inputs[0]) {
                            resolve_pointer_input(
                                pcode,
                                ssa,
                                options,
                                site,
                                1,
                                visiting,
                                budget - 1,
                            )?
                            .shifted(delta)
                        } else if let Some(delta) = signed_constant_delta(&op.inputs[1]) {
                            resolve_pointer_input(
                                pcode,
                                ssa,
                                options,
                                site,
                                0,
                                visiting,
                                budget - 1,
                            )?
                            .shifted(delta)
                        } else {
                            None
                        }
                    }
                    PcodeOpcode::IntSub if op.inputs.len() == 2 => {
                        let delta = signed_constant_delta(&op.inputs[1])?;
                        resolve_pointer_input(pcode, ssa, options, site, 0, visiting, budget - 1)?
                            .shifted(delta.checked_neg()?)
                    }
                    _ => None,
                }
            }
        }
    };
    visiting.remove(&value_id);
    resolved
}

fn signed_constant_delta(varnode: &Varnode) -> Option<i64> {
    if !varnode.is_constant {
        return None;
    }
    let bits = varnode.size.checked_mul(8)?;
    match bits {
        1..=63 => {
            let mask = (1_u64 << bits) - 1;
            let value = varnode.offset & mask;
            let sign_bit = 1_u64 << (bits - 1);
            if value & sign_bit == 0 {
                i64::try_from(value).ok()
            } else {
                Some((value | !mask) as i64)
            }
        }
        64 => Some(varnode.offset as i64),
        _ => None,
    }
}

#[derive(Debug, Default)]
struct MemoryLayout {
    partitions: BTreeMap<(SsaMemoryRegion, u64), Vec<SsaMemoryStorageKey>>,
}

impl MemoryLayout {
    fn build(guards: &BTreeMap<SsaOpSite, SsaDynamicGuard>) -> Self {
        let mut ranges = BTreeMap::<(SsaMemoryRegion, u64), Vec<(i128, i128)>>::new();
        for guard in guards.values() {
            if !matches!(
                guard.kind,
                SsaDynamicGuardKind::Load | SsaDynamicGuardKind::Store
            ) || guard.precision != SsaGuardRangePrecision::Exact
            {
                continue;
            }
            let (Some(region), Some(space_id), Some(end)) =
                (guard.region, guard.space_id, guard.maximum_offset_exclusive)
            else {
                continue;
            };
            ranges
                .entry((region, space_id))
                .or_default()
                .push((guard.minimum_offset, end));
        }

        let mut layout = Self::default();
        for (identity, space_ranges) in ranges {
            let mut boundaries = BTreeSet::new();
            for &(start, end) in &space_ranges {
                boundaries.insert(start);
                boundaries.insert(end);
            }
            let mut partitions = Vec::new();
            let boundaries = boundaries.into_iter().collect::<Vec<_>>();
            for pair in boundaries.windows(2) {
                let start = pair[0];
                let end = pair[1];
                if !space_ranges
                    .iter()
                    .any(|(range_start, range_end)| *range_start <= start && end <= *range_end)
                {
                    continue;
                }
                let Ok(size) = u32::try_from(end - start) else {
                    continue;
                };
                partitions.push(SsaMemoryStorageKey {
                    region: identity.0,
                    space_id: identity.1,
                    offset: start,
                    size,
                });
            }
            layout.partitions.insert(identity, partitions);
        }
        layout
    }

    fn storages(&self) -> impl Iterator<Item = SsaMemoryStorageKey> + '_ {
        self.partitions.values().flatten().copied()
    }

    fn exact_access(&self, guard: &SsaDynamicGuard) -> Vec<(u32, SsaMemoryStorageKey)> {
        if guard.precision != SsaGuardRangePrecision::Exact {
            return Vec::new();
        }
        let (Some(region), Some(space_id), Some(end)) =
            (guard.region, guard.space_id, guard.maximum_offset_exclusive)
        else {
            return Vec::new();
        };
        self.partitions
            .get(&(region, space_id))
            .into_iter()
            .flatten()
            .filter(|storage| {
                let storage_end = storage.offset + i128::from(storage.size);
                guard.minimum_offset <= storage.offset && storage_end <= end
            })
            .filter_map(|storage| {
                u32::try_from(storage.offset - guard.minimum_offset)
                    .ok()
                    .map(|offset| (offset, *storage))
            })
            .collect()
    }

    fn write_effect(&self, guard: &SsaDynamicGuard) -> Vec<SsaMemoryStorageKey> {
        if !guard.effect.writes() {
            return Vec::new();
        }
        if guard.kind == SsaDynamicGuardKind::Call {
            return self.storages().collect();
        }
        let Some(space_id) = guard.space_id else {
            return self.storages().collect();
        };
        let candidates = match guard.region {
            Some(region) => self
                .partitions
                .get(&(region, space_id))
                .into_iter()
                .flatten()
                .copied()
                .collect::<Vec<_>>(),
            None => self
                .partitions
                .iter()
                .filter(|((_, candidate_space), _)| *candidate_space == space_id)
                .flat_map(|(_, storages)| storages.iter().copied())
                .collect::<Vec<_>>(),
        };
        let Some(end) = guard.maximum_offset_exclusive else {
            return candidates;
        };
        candidates
            .into_iter()
            .filter(|storage| {
                let storage_end = storage.offset + i128::from(storage.size);
                storage.offset < end && guard.minimum_offset < storage_end
            })
            .collect()
    }

    fn read_effect(&self, guard: &SsaDynamicGuard) -> Vec<(u32, SsaMemoryStorageKey)> {
        if !guard.effect.reads() {
            return Vec::new();
        }
        if guard.kind == SsaDynamicGuardKind::Call {
            return self.storages().map(|storage| (0, storage)).collect();
        }
        self.exact_access(guard)
    }
}

fn allocate_memory_value(
    ssa: &mut NirScalarSsa,
    storage: SsaMemoryStorageKey,
    definition: SsaValueDefinition,
) -> SsaMemoryValueId {
    let id = SsaMemoryValueId(ssa.memory_values.len() as u32);
    ssa.memory_values.push(SsaMemoryValue {
        id,
        storage,
        definition,
    });
    id
}

fn build_memory_ssa(
    pcode: &PcodeFunction,
    successors: &[Vec<usize>],
    dominance: &Dominance,
    ssa: &mut NirScalarSsa,
) {
    let layout = MemoryLayout::build(&ssa.dynamic_guards);
    if layout.partitions.is_empty() {
        return;
    }

    let mut definition_blocks = BTreeMap::<SsaMemoryStorageKey, BTreeSet<usize>>::new();
    for storage in layout.storages() {
        definition_blocks.entry(storage).or_default().insert(0);
    }
    for (site, guard) in &ssa.dynamic_guards {
        if !dominance.reachable.contains(&(site.block as usize)) {
            continue;
        }
        for storage in layout.write_effect(guard) {
            definition_blocks
                .entry(storage)
                .or_default()
                .insert(site.block as usize);
        }
    }

    let mut phi_plan = BTreeMap::<usize, BTreeSet<SsaMemoryStorageKey>>::new();
    for (&storage, defining_blocks) in &definition_blocks {
        let mut work = defining_blocks.clone();
        let mut placed = BTreeSet::new();
        while let Some(block) = work.pop_first() {
            for &frontier in &dominance.frontier[block] {
                if !placed.insert(frontier) {
                    continue;
                }
                phi_plan.entry(frontier).or_default().insert(storage);
                if !defining_blocks.contains(&frontier) {
                    work.insert(frontier);
                }
            }
        }
    }

    let mut stacks = BTreeMap::<SsaMemoryStorageKey, Vec<SsaMemoryValueId>>::new();
    for storage in layout.storages() {
        let input = allocate_memory_value(ssa, storage, SsaValueDefinition::Input);
        ssa.memory_inputs.insert(storage, input);
        stacks.insert(storage, vec![input]);
    }

    let mut phi_outputs = BTreeMap::new();
    for (&block, storages) in &phi_plan {
        for &storage in storages {
            let output = allocate_memory_value(
                ssa,
                storage,
                SsaValueDefinition::Phi {
                    block: block as u32,
                },
            );
            phi_outputs.insert((block, storage), output);
        }
    }

    let write_sites = ssa
        .dynamic_guards
        .iter()
        .map(|(site, guard)| (*site, layout.write_effect(guard)))
        .filter(|(_, storages)| !storages.is_empty())
        .collect::<Vec<_>>();
    for (site, storages) in write_sites {
        let pieces = storages
            .into_iter()
            .map(|storage| SsaMemoryAccessPiece {
                byte_offset: 0,
                value: allocate_memory_value(ssa, storage, SsaValueDefinition::Operation(site)),
            })
            .collect();
        ssa.memory_operation_outputs.insert(site, pieces);
    }

    let mut phi_operands =
        BTreeMap::<(usize, SsaMemoryStorageKey), Vec<SsaMemoryPhiOperand>>::new();
    rename_memory_block(
        0,
        pcode,
        successors,
        dominance,
        &layout,
        &phi_plan,
        &phi_outputs,
        &mut stacks,
        ssa,
        &mut phi_operands,
    );
    for (block, storages) in phi_plan {
        let phis = storages
            .into_iter()
            .map(|storage| {
                let mut operands = phi_operands.remove(&(block, storage)).unwrap_or_default();
                operands.sort_unstable_by_key(|operand| operand.predecessor);
                SsaMemoryPhiNode {
                    storage,
                    output: phi_outputs[&(block, storage)],
                    operands,
                }
            })
            .collect();
        ssa.memory_phis.insert(block as u32, phis);
    }
}

#[allow(clippy::too_many_arguments)]
fn rename_memory_block(
    block: usize,
    pcode: &PcodeFunction,
    successors: &[Vec<usize>],
    dominance: &Dominance,
    layout: &MemoryLayout,
    phi_plan: &BTreeMap<usize, BTreeSet<SsaMemoryStorageKey>>,
    phi_outputs: &BTreeMap<(usize, SsaMemoryStorageKey), SsaMemoryValueId>,
    stacks: &mut BTreeMap<SsaMemoryStorageKey, Vec<SsaMemoryValueId>>,
    ssa: &mut NirScalarSsa,
    phi_operands: &mut BTreeMap<(usize, SsaMemoryStorageKey), Vec<SsaMemoryPhiOperand>>,
) {
    let mut pushed = Vec::new();
    if let Some(storages) = phi_plan.get(&block) {
        for &storage in storages {
            stacks
                .get_mut(&storage)
                .expect("memory phi storage has an input")
                .push(phi_outputs[&(block, storage)]);
            pushed.push(storage);
        }
    }

    if let Some(pcode_block) = pcode.blocks.get(block) {
        for (op, _) in pcode_block.ops.iter().enumerate() {
            let site = SsaOpSite {
                block: block as u32,
                op: op as u32,
            };
            if let Some(guard) = ssa.dynamic_guards.get(&site) {
                let pieces = layout
                    .read_effect(guard)
                    .into_iter()
                    .map(|(byte_offset, storage)| SsaMemoryAccessPiece {
                        byte_offset,
                        value: *stacks
                            .get(&storage)
                            .and_then(|stack| stack.last())
                            .expect("promoted memory has a reaching value"),
                    })
                    .collect::<Vec<_>>();
                if !pieces.is_empty() {
                    ssa.memory_operation_inputs.insert(site, pieces);
                }
            }
            if let Some(outputs) = ssa.memory_operation_outputs.get(&site).cloned() {
                for piece in outputs {
                    let storage = ssa.memory_values[piece.value.0 as usize].storage;
                    stacks
                        .get_mut(&storage)
                        .expect("memory output storage has an input")
                        .push(piece.value);
                    pushed.push(storage);
                }
            }
        }
    }

    for &successor in successors.get(block).into_iter().flatten() {
        let Some(storages) = phi_plan.get(&successor) else {
            continue;
        };
        for &storage in storages {
            phi_operands
                .entry((successor, storage))
                .or_default()
                .push(SsaMemoryPhiOperand {
                    predecessor: block as u32,
                    value: *stacks
                        .get(&storage)
                        .and_then(|stack| stack.last())
                        .expect("memory phi has a reaching value"),
                });
        }
    }
    for &child in &dominance.children[block] {
        rename_memory_block(
            child,
            pcode,
            successors,
            dominance,
            layout,
            phi_plan,
            phi_outputs,
            stacks,
            ssa,
            phi_operands,
        );
    }
    for storage in pushed.into_iter().rev() {
        stacks
            .get_mut(&storage)
            .expect("pushed memory storage remains present")
            .pop();
    }
}

fn build_out_of_ssa_facts(
    pcode: &PcodeFunction,
    ssa: &NirScalarSsa,
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
    dominance: &Dominance,
) -> (
    Vec<SsaOutOfSsaCopy>,
    Vec<SsaHighVariable>,
    Vec<SsaHighVariableId>,
) {
    let mut copies = Vec::new();
    let mut parents = (0..ssa.values.len()).collect::<Vec<_>>();
    for (&successor, phis) in &ssa.phis {
        for phi in phis {
            for operand in &phi.operands {
                copies.push(SsaOutOfSsaCopy {
                    predecessor: operand.predecessor,
                    successor,
                    storage: phi.storage,
                    source: operand.value,
                    destination: phi.output,
                });
                union_values(
                    &mut parents,
                    phi.output.0 as usize,
                    operand.value.0 as usize,
                );
            }
        }
    }
    copies.sort_unstable();
    copies.dedup();

    let uses = collect_value_uses(ssa);
    let value_covers = (0..ssa.values.len())
        .map(|index| {
            build_value_cover(
                pcode,
                ssa,
                SsaValueId(index as u32),
                uses.get(&SsaValueId(index as u32))
                    .map_or(&[], Vec::as_slice),
                predecessors,
                dominance,
            )
        })
        .collect::<Vec<_>>();
    for (site, op) in pcode
        .blocks
        .iter()
        .enumerate()
        .flat_map(|(block, block_data)| {
            block_data
                .ops
                .iter()
                .enumerate()
                .map(move |(op, operation)| {
                    (
                        SsaOpSite {
                            block: block as u32,
                            op: op as u32,
                        },
                        operation,
                    )
                })
        })
    {
        if !matches!(op.opcode, PcodeOpcode::Copy | PcodeOpcode::Cast) {
            continue;
        }
        let Some(outputs) = ssa.operation_outputs.get(&site) else {
            continue;
        };
        let Some(inputs) = ssa.operation_inputs.get(&SsaUseSite {
            block: site.block,
            op: site.op,
            input: 0,
        }) else {
            continue;
        };
        if outputs.len() != inputs.len() {
            continue;
        }
        for (output, input) in outputs.iter().zip(inputs) {
            let output_value = ssa.value(output.value).expect("known output");
            let input_value = ssa.value(input.value).expect("known input");
            if output.byte_offset != input.byte_offset
                || output_value.storage.size != input_value.storage.size
                || matches!(input_value.definition, SsaValueDefinition::Input)
            {
                continue;
            }
            let left = find_root(&mut parents, output.value.0 as usize);
            let right = find_root(&mut parents, input.value.0 as usize);
            if left == right || groups_interfere(&mut parents, left, right, &value_covers) {
                continue;
            }
            union_values(&mut parents, left, right);
        }
    }

    let mut grouped = BTreeMap::<usize, Vec<SsaValueId>>::new();
    for index in 0..ssa.values.len() {
        let root = find_root(&mut parents, index);
        grouped
            .entry(root)
            .or_default()
            .push(SsaValueId(index as u32));
    }
    let mut member_groups = grouped.into_values().collect::<Vec<_>>();
    member_groups.sort_unstable_by_key(|members| members[0]);

    let reachability = guard_reachability(ssa, successors);
    let mut high_variables = Vec::with_capacity(member_groups.len());
    let mut value_high_variables = vec![SsaHighVariableId(0); ssa.values.len()];
    for members in member_groups {
        let id = SsaHighVariableId(high_variables.len() as u32);
        let mut storage_family = members
            .iter()
            .map(|member| ssa.value(*member).expect("known member").storage)
            .collect::<Vec<_>>();
        storage_family.sort_unstable();
        storage_family.dedup();
        let crossing_guards = ssa
            .dynamic_guards
            .keys()
            .copied()
            .filter(|guard| {
                members.iter().any(|member| {
                    value_crosses_guard(
                        ssa,
                        dominance,
                        *member,
                        *guard,
                        uses.get(member).map_or(&[], Vec::as_slice),
                        reachability
                            .get(&guard.block)
                            .expect("every guard block has a reachability set"),
                    )
                })
            })
            .collect();
        let cover = merge_covers(
            members
                .iter()
                .flat_map(|member| value_covers[member.0 as usize].iter().copied()),
        );
        for member in &members {
            value_high_variables[member.0 as usize] = id;
        }
        high_variables.push(SsaHighVariable {
            id,
            members,
            storage_family,
            crossing_guards,
            cover,
        });
    }

    (copies, high_variables, value_high_variables)
}

fn groups_interfere(
    parents: &mut [usize],
    left: usize,
    right: usize,
    covers: &[Vec<SsaCoverBlock>],
) -> bool {
    let left_cover = merge_covers(
        (0..parents.len())
            .filter(|&index| find_root(parents, index) == left)
            .flat_map(|index| covers[index].iter().copied()),
    );
    let right_cover = merge_covers(
        (0..parents.len())
            .filter(|&index| find_root(parents, index) == right)
            .flat_map(|index| covers[index].iter().copied()),
    );
    left_cover.iter().any(|left| {
        right_cover.iter().any(|right| {
            left.block == right.block
                && left.start < right.end_exclusive
                && right.start < left.end_exclusive
        })
    })
}

fn merge_covers(cover: impl IntoIterator<Item = SsaCoverBlock>) -> Vec<SsaCoverBlock> {
    let mut by_block = BTreeMap::<u32, (u32, u32)>::new();
    for interval in cover {
        by_block
            .entry(interval.block)
            .and_modify(|(start, end)| {
                *start = (*start).min(interval.start);
                *end = (*end).max(interval.end_exclusive);
            })
            .or_insert((interval.start, interval.end_exclusive));
    }
    by_block
        .into_iter()
        .map(|(block, (start, end_exclusive))| SsaCoverBlock {
            block,
            start,
            end_exclusive,
        })
        .collect()
}

fn build_value_cover(
    pcode: &PcodeFunction,
    ssa: &NirScalarSsa,
    value_id: SsaValueId,
    uses: &[ValueUsePoint],
    predecessors: &[Vec<usize>],
    dominance: &Dominance,
) -> Vec<SsaCoverBlock> {
    let value = ssa.value(value_id).expect("known value");
    let (definition_block, definition_start) = match value.definition {
        SsaValueDefinition::Input => (0, 0),
        SsaValueDefinition::Phi { block } => (block, 0),
        SsaValueDefinition::Operation(site) => {
            (site.block, site.op.saturating_mul(2).saturating_add(2))
        }
    };
    let block_end = |block: u32| {
        pcode.blocks.get(block as usize).map_or(1, |block| {
            (block.ops.len() as u32).saturating_mul(2).saturating_add(3)
        })
    };
    if uses.is_empty() {
        return vec![SsaCoverBlock {
            block: definition_block,
            start: definition_start,
            end_exclusive: definition_start.saturating_add(1),
        }];
    }

    let mut cover = Vec::new();
    for use_point in uses {
        let use_end = use_point.op.map_or_else(
            || block_end(use_point.block),
            |op| op.saturating_mul(2).saturating_add(2),
        );
        let mut queue = VecDeque::from([(use_point.block, use_end)]);
        let mut visited = BTreeSet::new();
        while let Some((block, end)) = queue.pop_front() {
            if !visited.insert(block) {
                continue;
            }
            if block == definition_block {
                if definition_start < end {
                    cover.push(SsaCoverBlock {
                        block,
                        start: definition_start,
                        end_exclusive: end,
                    });
                }
                continue;
            }
            let definition_can_reach = matches!(value.definition, SsaValueDefinition::Input)
                || dominance.dominates(definition_block as usize, block as usize);
            if !definition_can_reach {
                continue;
            }
            cover.push(SsaCoverBlock {
                block,
                start: 0,
                end_exclusive: end,
            });
            for &predecessor in predecessors.get(block as usize).into_iter().flatten() {
                queue.push_back((predecessor as u32, block_end(predecessor as u32)));
            }
        }
    }
    merge_covers(cover)
}

fn find_root(parents: &mut [usize], value: usize) -> usize {
    if parents[value] != value {
        parents[value] = find_root(parents, parents[value]);
    }
    parents[value]
}

fn union_values(parents: &mut [usize], left: usize, right: usize) {
    let left_root = find_root(parents, left);
    let right_root = find_root(parents, right);
    if left_root == right_root {
        return;
    }
    let (minimum, maximum) = if left_root < right_root {
        (left_root, right_root)
    } else {
        (right_root, left_root)
    };
    parents[maximum] = minimum;
}

#[derive(Debug, Clone, Copy)]
struct ValueUsePoint {
    block: u32,
    op: Option<u32>,
}

fn collect_value_uses(ssa: &NirScalarSsa) -> BTreeMap<SsaValueId, Vec<ValueUsePoint>> {
    let mut uses = BTreeMap::<SsaValueId, Vec<ValueUsePoint>>::new();
    for (site, pieces) in &ssa.operation_inputs {
        for piece in pieces {
            uses.entry(piece.value).or_default().push(ValueUsePoint {
                block: site.block,
                op: Some(site.op),
            });
        }
    }
    for phis in ssa.phis.values() {
        for phi in phis {
            for operand in &phi.operands {
                uses.entry(operand.value).or_default().push(ValueUsePoint {
                    block: operand.predecessor,
                    op: None,
                });
            }
        }
    }
    uses
}

/// Memory-side analog of `build_out_of_ssa_facts`. Unlike the scalar case,
/// there is no speculative-merge step: a `Store` never reads a prior value
/// at the address it writes, so there is no memory analog of a scalar
/// `Copy`/`Cast` congruence. `memory_phis` (control-flow-join merges) is the
/// *only* source of forced congruence between two `SsaMemoryValueId`s.
fn build_memory_out_of_ssa_facts(
    pcode: &PcodeFunction,
    ssa: &NirScalarSsa,
    predecessors: &[Vec<usize>],
    dominance: &Dominance,
) -> (Vec<SsaMemoryHighVariable>, Vec<SsaMemoryHighVariableId>) {
    let mut parents: Vec<usize> = (0..ssa.memory_values.len()).collect();
    for phis in ssa.memory_phis.values() {
        for phi in phis {
            for operand in &phi.operands {
                union_values(
                    &mut parents,
                    phi.output.0 as usize,
                    operand.value.0 as usize,
                );
            }
        }
    }

    let uses = collect_memory_value_uses(ssa);
    let empty_uses: Vec<MemoryValueUsePoint> = Vec::new();
    let covers: Vec<Vec<SsaCoverBlock>> = ssa
        .memory_values
        .iter()
        .map(|value| {
            let value_uses = uses.get(&value.id).unwrap_or(&empty_uses);
            build_memory_value_cover(pcode, ssa, value.id, value_uses, predecessors, dominance)
        })
        .collect();

    let mut member_groups = BTreeMap::<usize, Vec<usize>>::new();
    for index in 0..ssa.memory_values.len() {
        let root = find_root(&mut parents, index);
        member_groups.entry(root).or_default().push(index);
    }

    let mut high_variables = Vec::with_capacity(member_groups.len());
    let mut value_high_variables =
        vec![SsaMemoryHighVariableId(0); ssa.memory_values.len()];
    for (id, members) in member_groups.into_values().enumerate() {
        let hv_id = SsaMemoryHighVariableId(id as u32);
        let cover = merge_covers(
            members
                .iter()
                .flat_map(|&member| covers[member].iter().copied()),
        );
        for &member in &members {
            value_high_variables[member] = hv_id;
        }
        high_variables.push(SsaMemoryHighVariable {
            id: hv_id,
            members: members
                .into_iter()
                .map(|index| ssa.memory_values[index].id)
                .collect(),
            cover,
        });
    }

    (high_variables, value_high_variables)
}

fn build_memory_value_cover(
    pcode: &PcodeFunction,
    ssa: &NirScalarSsa,
    value_id: SsaMemoryValueId,
    uses: &[MemoryValueUsePoint],
    predecessors: &[Vec<usize>],
    dominance: &Dominance,
) -> Vec<SsaCoverBlock> {
    let value = ssa.memory_value(value_id).expect("known memory value");
    let (definition_block, definition_start) = match value.definition {
        SsaValueDefinition::Input => (0, 0),
        SsaValueDefinition::Phi { block } => (block, 0),
        SsaValueDefinition::Operation(site) => {
            (site.block, site.op.saturating_mul(2).saturating_add(2))
        }
    };
    let block_end = |block: u32| {
        pcode.blocks.get(block as usize).map_or(1, |block| {
            (block.ops.len() as u32).saturating_mul(2).saturating_add(3)
        })
    };
    if uses.is_empty() {
        return vec![SsaCoverBlock {
            block: definition_block,
            start: definition_start,
            end_exclusive: definition_start.saturating_add(1),
        }];
    }

    let mut cover = Vec::new();
    for use_point in uses {
        let use_end = use_point.op.map_or_else(
            || block_end(use_point.block),
            |op| op.saturating_mul(2).saturating_add(2),
        );
        let mut queue = VecDeque::from([(use_point.block, use_end)]);
        let mut visited = BTreeSet::new();
        while let Some((block, end)) = queue.pop_front() {
            if !visited.insert(block) {
                continue;
            }
            if block == definition_block {
                if definition_start < end {
                    cover.push(SsaCoverBlock {
                        block,
                        start: definition_start,
                        end_exclusive: end,
                    });
                }
                continue;
            }
            let definition_can_reach = matches!(value.definition, SsaValueDefinition::Input)
                || dominance.dominates(definition_block as usize, block as usize);
            if !definition_can_reach {
                continue;
            }
            cover.push(SsaCoverBlock {
                block,
                start: 0,
                end_exclusive: end,
            });
            for &predecessor in predecessors.get(block as usize).into_iter().flatten() {
                queue.push_back((predecessor as u32, block_end(predecessor as u32)));
            }
        }
    }
    merge_covers(cover)
}

#[derive(Debug, Clone, Copy)]
struct MemoryValueUsePoint {
    block: u32,
    op: Option<u32>,
}

fn collect_memory_value_uses(
    ssa: &NirScalarSsa,
) -> BTreeMap<SsaMemoryValueId, Vec<MemoryValueUsePoint>> {
    let mut uses = BTreeMap::<SsaMemoryValueId, Vec<MemoryValueUsePoint>>::new();
    for (site, pieces) in &ssa.memory_operation_inputs {
        for piece in pieces {
            uses.entry(piece.value)
                .or_default()
                .push(MemoryValueUsePoint {
                    block: site.block,
                    op: Some(site.op),
                });
        }
    }
    for phis in ssa.memory_phis.values() {
        for phi in phis {
            for operand in &phi.operands {
                uses.entry(operand.value)
                    .or_default()
                    .push(MemoryValueUsePoint {
                        block: operand.predecessor,
                        op: None,
                    });
            }
        }
    }
    uses
}

fn guard_reachability(
    ssa: &NirScalarSsa,
    successors: &[Vec<usize>],
) -> BTreeMap<u32, BTreeSet<usize>> {
    let mut result = BTreeMap::new();
    for guard_block in ssa.dynamic_guards.keys().map(|site| site.block) {
        result.entry(guard_block).or_insert_with(|| {
            let mut reachable = BTreeSet::new();
            let mut queue = VecDeque::new();
            for successor in successors.get(guard_block as usize).into_iter().flatten() {
                queue.push_back(*successor);
            }
            while let Some(block) = queue.pop_front() {
                if !reachable.insert(block) {
                    continue;
                }
                for successor in successors.get(block).into_iter().flatten() {
                    queue.push_back(*successor);
                }
            }
            reachable
        });
    }
    result
}

fn value_crosses_guard(
    ssa: &NirScalarSsa,
    dominance: &Dominance,
    value_id: SsaValueId,
    guard: SsaOpSite,
    uses: &[ValueUsePoint],
    reachable_after_guard: &BTreeSet<usize>,
) -> bool {
    let definition_reaches_guard = match ssa.value(value_id).expect("known value").definition {
        SsaValueDefinition::Input => true,
        SsaValueDefinition::Operation(definition) => {
            if definition.block == guard.block {
                definition.op < guard.op
            } else {
                dominance.dominates(definition.block as usize, guard.block as usize)
            }
        }
        SsaValueDefinition::Phi { block } => {
            block == guard.block || dominance.dominates(block as usize, guard.block as usize)
        }
    };
    definition_reaches_guard
        && uses.iter().any(|use_point| {
            if use_point.block == guard.block {
                use_point.op.is_none_or(|op| op > guard.op)
                    || reachable_after_guard.contains(&(guard.block as usize))
            } else {
                reachable_after_guard.contains(&(use_point.block as usize))
            }
        })
}

pub(super) fn validate_scalar_ssa(
    pcode: &PcodeFunction,
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
    ssa: &NirScalarSsa,
) -> Result<(), ScalarSsaValidationError> {
    validate_scalar_ssa_with_context(
        pcode,
        successors,
        predecessors,
        ssa,
        &MlilPreviewOptions::default(),
        None,
    )
}

pub(super) fn validate_scalar_ssa_with_context(
    pcode: &PcodeFunction,
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
    ssa: &NirScalarSsa,
    options: &MlilPreviewOptions,
    type_context: Option<&PreviewTypeContext>,
) -> Result<(), ScalarSsaValidationError> {
    ssa.validate_shape()
        .map_err(ScalarSsaValidationError::Shape)?;
    let dominance = Dominance::analyze(successors, predecessors);
    let layout = StorageLayout::build(pcode, &dominance.reachable);
    if ssa.address_spaces != layout.guards {
        return Err(ScalarSsaValidationError::AddressSpaceGuardsMismatch);
    }
    if ssa.inputs.keys().copied().collect::<Vec<_>>() != layout.storages().collect::<Vec<_>>() {
        return Err(ScalarSsaValidationError::StoragePartitionsMismatch);
    }

    for &block in &dominance.reachable {
        let Some(pcode_block) = pcode.blocks.get(block) else {
            continue;
        };
        for (op_index, op) in pcode_block.ops.iter().enumerate() {
            let output_site = SsaOpSite {
                block: block as u32,
                op: op_index as u32,
            };
            if let Some(output) = op.output.as_ref() {
                let expected_pieces = layout.access_pieces(output);
                if !expected_pieces.is_empty() {
                    let actual_pieces = ssa.operation_outputs.get(&output_site).ok_or(
                        ScalarSsaValidationError::MissingOperationOutput(output_site),
                    )?;
                    if actual_pieces.len() != expected_pieces.len() {
                        return Err(ScalarSsaValidationError::OperationPieceCount {
                            site: output_site,
                            expected: expected_pieces.len(),
                            actual: actual_pieces.len(),
                        });
                    }
                    for ((expected_offset, expected), actual_piece) in
                        expected_pieces.into_iter().zip(actual_pieces)
                    {
                        let actual = ssa
                            .value(actual_piece.value)
                            .expect("shape validated")
                            .storage;
                        if actual != expected || actual_piece.byte_offset != expected_offset {
                            return Err(ScalarSsaValidationError::OperationStorageMismatch {
                                site: output_site,
                                expected,
                                actual,
                            });
                        }
                    }
                }
            }

            for (input_index, input) in op.inputs.iter().enumerate() {
                let expected_pieces = layout.access_pieces(input);
                if expected_pieces.is_empty() {
                    continue;
                }
                let use_site = SsaUseSite {
                    block: block as u32,
                    op: op_index as u32,
                    input: input_index as u32,
                };
                let actual_pieces = ssa
                    .operation_inputs
                    .get(&use_site)
                    .ok_or(ScalarSsaValidationError::MissingOperationInput(use_site))?;
                if actual_pieces.len() != expected_pieces.len() {
                    return Err(ScalarSsaValidationError::UsePieceCount {
                        site: use_site,
                        expected: expected_pieces.len(),
                        actual: actual_pieces.len(),
                    });
                }
                for ((expected_offset, expected), actual_piece) in
                    expected_pieces.into_iter().zip(actual_pieces)
                {
                    let actual = ssa
                        .value(actual_piece.value)
                        .expect("shape validated")
                        .storage;
                    if actual != expected || actual_piece.byte_offset != expected_offset {
                        return Err(ScalarSsaValidationError::UseStorageMismatch {
                            site: use_site,
                            expected,
                            actual,
                        });
                    }
                    if !value_dominates_use(ssa, &dominance, actual_piece.value, use_site) {
                        return Err(ScalarSsaValidationError::NonDominatingUse {
                            site: use_site,
                            value: actual_piece.value,
                        });
                    }
                }
            }
        }
    }

    for (&block, phis) in &ssa.phis {
        let block_index = block as usize;
        let mut expected: Vec<u32> = predecessors
            .get(block_index)
            .into_iter()
            .flatten()
            .copied()
            .filter(|predecessor| dominance.reachable.contains(predecessor))
            .map(|predecessor| predecessor as u32)
            .collect();
        expected.sort_unstable();
        expected.dedup();
        for phi in phis {
            let actual: Vec<u32> = phi
                .operands
                .iter()
                .map(|operand| operand.predecessor)
                .collect();
            if actual != expected {
                return Err(ScalarSsaValidationError::PhiPredecessors {
                    block,
                    expected: expected.clone(),
                    actual,
                });
            }
            for operand in &phi.operands {
                let value = ssa.value(operand.value).expect("shape validated");
                if value.storage != phi.storage {
                    return Err(ScalarSsaValidationError::PhiStorageMismatch {
                        block,
                        predecessor: operand.predecessor,
                        expected: phi.storage,
                        actual: value.storage,
                    });
                }
                if !value_dominates_block_end(
                    ssa,
                    &dominance,
                    operand.value,
                    operand.predecessor as usize,
                ) {
                    return Err(ScalarSsaValidationError::NonDominatingPhiOperand {
                        block,
                        predecessor: operand.predecessor,
                        value: operand.value,
                    });
                }
            }
        }
    }

    if ssa.dynamic_guards
        != build_dynamic_guards(pcode, &dominance.reachable, ssa, options, type_context)
    {
        return Err(ScalarSsaValidationError::DynamicGuardsMismatch);
    }
    let mut expected_memory = NirScalarSsa {
        dynamic_guards: ssa.dynamic_guards.clone(),
        ..NirScalarSsa::default()
    };
    build_memory_ssa(pcode, successors, &dominance, &mut expected_memory);
    if ssa.memory_values != expected_memory.memory_values
        || ssa.memory_inputs != expected_memory.memory_inputs
        || ssa.memory_operation_inputs != expected_memory.memory_operation_inputs
        || ssa.memory_operation_outputs != expected_memory.memory_operation_outputs
        || ssa.memory_phis != expected_memory.memory_phis
    {
        return Err(ScalarSsaValidationError::StoragePartitionsMismatch);
    }
    let (out_of_ssa_copies, high_variables, value_high_variables) =
        build_out_of_ssa_facts(pcode, ssa, successors, predecessors, &dominance);
    if ssa.out_of_ssa_copies != out_of_ssa_copies {
        return Err(ScalarSsaValidationError::OutOfSsaCopiesMismatch);
    }
    if ssa.high_variables != high_variables || ssa.value_high_variables != value_high_variables {
        return Err(ScalarSsaValidationError::HighVariablesMismatch);
    }

    Ok(())
}

fn value_dominates_use(
    ssa: &NirScalarSsa,
    dominance: &Dominance,
    value_id: SsaValueId,
    use_site: SsaUseSite,
) -> bool {
    match ssa.value(value_id).expect("shape validated").definition {
        SsaValueDefinition::Input => true,
        SsaValueDefinition::Operation(definition) => {
            if definition.block == use_site.block {
                definition.op < use_site.op
            } else {
                dominance.dominates(definition.block as usize, use_site.block as usize)
            }
        }
        SsaValueDefinition::Phi { block } => {
            block == use_site.block || dominance.dominates(block as usize, use_site.block as usize)
        }
    }
}

fn value_dominates_block_end(
    ssa: &NirScalarSsa,
    dominance: &Dominance,
    value_id: SsaValueId,
    use_block: usize,
) -> bool {
    match ssa.value(value_id).expect("shape validated").definition {
        SsaValueDefinition::Input => true,
        SsaValueDefinition::Operation(definition) => {
            definition.block as usize == use_block
                || dominance.dominates(definition.block as usize, use_block)
        }
        SsaValueDefinition::Phi { block } => {
            block as usize == use_block || dominance.dominates(block as usize, use_block)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pcode::PcodeBasicBlock;

    fn register(offset: u64) -> Varnode {
        register_sized(offset, 4)
    }

    fn register_sized(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: REGISTER_SPACE_ID,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn unique(offset: u64) -> Varnode {
        unique_sized(offset, 4)
    }

    fn unique_sized(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn copy(address: u64, output: Varnode, input: Varnode) -> PcodeOp {
        PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::Copy,
            address,
            output: Some(output),
            inputs: vec![input],
            asm_mnemonic: None,
        }
    }

    fn op(
        address: u64,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
    ) -> PcodeOp {
        PcodeOp {
            seq_num: 0,
            opcode,
            address,
            output,
            inputs,
            asm_mnemonic: None,
        }
    }

    fn function(ops: Vec<Vec<PcodeOp>>) -> PcodeFunction {
        PcodeFunction {
            blocks: ops
                .into_iter()
                .enumerate()
                .map(|(index, ops)| PcodeBasicBlock {
                    index: index as u32,
                    start_address: 0x1000 + index as u64 * 0x10,
                    successors: Vec::new(),
                    ops,
                })
                .collect(),
        }
    }

    #[test]
    fn diamond_places_phi_and_renames_join_use() {
        let pcode = function(vec![
            vec![],
            vec![copy(0x1010, register(0), Varnode::constant(1, 4))],
            vec![copy(0x1020, register(0), Varnode::constant(2, 4))],
            vec![copy(0x1030, unique(0), register(0))],
        ]);
        let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let predecessors = vec![vec![], vec![0], vec![0], vec![1, 2]];

        let ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa).unwrap();

        let phi = &ssa.phis[&3][0];
        assert_eq!(
            phi.operands
                .iter()
                .map(|operand| operand.predecessor)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(
            ssa.operation_inputs[&SsaUseSite {
                block: 3,
                op: 0,
                input: 0,
            }][0]
                .value,
            phi.output
        );
    }

    #[test]
    fn loop_header_phi_receives_entry_and_latch_values() {
        let pcode = function(vec![
            vec![copy(0x1000, register(0), Varnode::constant(0, 4))],
            vec![copy(0x1010, unique(0), register(0))],
            vec![copy(0x1020, register(0), Varnode::constant(1, 4))],
            vec![],
        ]);
        let successors = vec![vec![1], vec![2, 3], vec![1], vec![]];
        let predecessors = vec![vec![], vec![0, 2], vec![1], vec![1]];

        let ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa).unwrap();

        let phi = &ssa.phis[&1][0];
        assert_eq!(
            phi.operands
                .iter()
                .map(|operand| operand.predecessor)
                .collect::<Vec<_>>(),
            vec![0, 2]
        );
        assert_eq!(
            ssa.operation_inputs[&SsaUseSite {
                block: 1,
                op: 0,
                input: 0,
            }][0]
                .value,
            phi.output
        );
    }

    #[test]
    fn same_block_uses_follow_each_redefinition() {
        let pcode = function(vec![vec![
            copy(0x1000, register(0), Varnode::constant(1, 4)),
            copy(0x1001, unique(0), register(0)),
            copy(0x1002, register(0), Varnode::constant(2, 4)),
            copy(0x1003, unique(4), register(0)),
        ]]);
        let successors = vec![vec![]];
        let predecessors = vec![vec![]];

        let ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa).unwrap();

        assert_eq!(
            ssa.operation_inputs[&SsaUseSite {
                block: 0,
                op: 1,
                input: 0,
            }][0]
                .value,
            ssa.operation_outputs[&SsaOpSite { block: 0, op: 0 }][0].value
        );
        assert_eq!(
            ssa.operation_inputs[&SsaUseSite {
                block: 0,
                op: 3,
                input: 0,
            }][0]
                .value,
            ssa.operation_outputs[&SsaOpSite { block: 0, op: 2 }][0].value
        );
    }

    #[test]
    fn unreachable_blocks_do_not_create_ssa_values() {
        let pcode = function(vec![
            vec![],
            vec![copy(0x1010, register(0), Varnode::constant(1, 4))],
        ]);
        let successors = vec![vec![], vec![]];
        let predecessors = vec![vec![], vec![]];

        let ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa).unwrap();

        assert!(ssa.values.is_empty());
        assert!(ssa.operation_outputs.is_empty());
    }

    #[test]
    fn scalar_ssa_is_deterministic() {
        let pcode = function(vec![
            vec![],
            vec![copy(0x1010, register(0), Varnode::constant(1, 4))],
            vec![copy(0x1020, register(0), Varnode::constant(2, 4))],
            vec![copy(0x1030, unique(0), register(0))],
        ]);
        let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let predecessors = vec![vec![], vec![0], vec![0], vec![1, 2]];

        assert_eq!(
            build_scalar_ssa(&pcode, &successors, &predecessors),
            build_scalar_ssa(&pcode, &successors, &predecessors)
        );
    }

    #[test]
    fn validator_rejects_missing_phi_predecessor() {
        let pcode = function(vec![
            vec![],
            vec![copy(0x1010, register(0), Varnode::constant(1, 4))],
            vec![copy(0x1020, register(0), Varnode::constant(2, 4))],
            vec![copy(0x1030, unique(0), register(0))],
        ]);
        let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let predecessors = vec![vec![], vec![0], vec![0], vec![1, 2]];
        let mut ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        ssa.phis.get_mut(&3).unwrap()[0].operands.pop();

        assert!(matches!(
            validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa),
            Err(ScalarSsaValidationError::PhiPredecessors { .. })
        ));
    }

    #[test]
    fn validator_rejects_non_dominating_use() {
        let pcode = function(vec![
            vec![],
            vec![copy(0x1010, register(0), Varnode::constant(1, 4))],
            vec![copy(0x1020, register(0), Varnode::constant(2, 4))],
            vec![copy(0x1030, unique(0), register(0))],
        ]);
        let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let predecessors = vec![vec![], vec![0], vec![0], vec![1, 2]];
        let mut ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        let sibling_value = ssa.operation_outputs[&SsaOpSite { block: 1, op: 0 }][0].value;
        ssa.operation_inputs.insert(
            SsaUseSite {
                block: 3,
                op: 0,
                input: 0,
            },
            vec![SsaAccessPiece {
                byte_offset: 0,
                value: sibling_value,
            }],
        );

        assert!(matches!(
            validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa),
            Err(ScalarSsaValidationError::NonDominatingUse { .. })
        ));
    }

    #[test]
    fn partial_register_write_replaces_only_overlapping_piece() {
        let pcode = function(vec![vec![
            copy(0x1000, register_sized(0, 8), Varnode::constant(1, 8)),
            copy(0x1001, register_sized(0, 4), Varnode::constant(2, 4)),
            copy(0x1002, unique_sized(0, 8), register_sized(0, 8)),
        ]]);
        let successors = vec![vec![]];
        let predecessors = vec![vec![]];

        let ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa).unwrap();

        let wide_definition = &ssa.operation_outputs[&SsaOpSite { block: 0, op: 0 }];
        let low_definition = &ssa.operation_outputs[&SsaOpSite { block: 0, op: 1 }];
        let wide_read = &ssa.operation_inputs[&SsaUseSite {
            block: 0,
            op: 2,
            input: 0,
        }];
        assert_eq!(wide_definition.len(), 2);
        assert_eq!(low_definition.len(), 1);
        assert_eq!(wide_read.len(), 2);
        assert_eq!(
            ssa.address_spaces[&REGISTER_SPACE_ID].kind,
            SsaAddressSpaceKind::Register
        );
        assert_eq!(
            ssa.address_spaces[&UNIQUE_SPACE_ID].kind,
            SsaAddressSpaceKind::Unique
        );
        assert_eq!(wide_read[0].value, low_definition[0].value);
        assert_eq!(wide_read[1].value, wide_definition[1].value);
        assert_eq!(
            wide_read
                .iter()
                .map(|piece| ssa.value(piece.value).unwrap().storage)
                .collect::<Vec<_>>(),
            vec![
                SsaStorageKey {
                    space_id: REGISTER_SPACE_ID,
                    offset: 0,
                    size: 4,
                },
                SsaStorageKey {
                    space_id: REGISTER_SPACE_ID,
                    offset: 4,
                    size: 4,
                },
            ]
        );
    }

    #[test]
    fn one_arm_partial_write_merges_with_entry_piece() {
        let pcode = function(vec![
            vec![],
            vec![copy(0x1010, register_sized(0, 4), Varnode::constant(1, 4))],
            vec![],
            vec![copy(0x1030, unique_sized(0, 8), register_sized(0, 8))],
        ]);
        let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let predecessors = vec![vec![], vec![0], vec![0], vec![1, 2]];

        let ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa).unwrap();

        let phis = &ssa.phis[&3];
        assert_eq!(phis.len(), 1, "only the overwritten low piece needs a phi");
        assert_eq!(phis[0].storage.offset, 0);
        assert_eq!(phis[0].storage.size, 4);
        let wide_read = &ssa.operation_inputs[&SsaUseSite {
            block: 3,
            op: 0,
            input: 0,
        }];
        assert_eq!(wide_read[0].value, phis[0].output);
        assert_eq!(
            wide_read[1].value,
            ssa.inputs[&SsaStorageKey {
                space_id: REGISTER_SPACE_ID,
                offset: 4,
                size: 4,
            }]
        );
    }

    #[test]
    fn shifted_overlaps_form_ordered_disjoint_cover() {
        let pcode = function(vec![vec![
            copy(0x1000, register_sized(0, 4), Varnode::constant(1, 4)),
            copy(0x1001, register_sized(2, 4), Varnode::constant(2, 4)),
            copy(0x1002, unique_sized(0, 6), register_sized(0, 6)),
        ]]);
        let successors = vec![vec![]];
        let predecessors = vec![vec![]];

        let ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa).unwrap();

        let wide_read = &ssa.operation_inputs[&SsaUseSite {
            block: 0,
            op: 2,
            input: 0,
        }];
        assert_eq!(
            wide_read
                .iter()
                .map(|piece| {
                    let storage = ssa.value(piece.value).unwrap().storage;
                    (piece.byte_offset, storage.offset, storage.size)
                })
                .collect::<Vec<_>>(),
            vec![(0, 0, 2), (2, 2, 2), (4, 4, 2)]
        );
    }

    #[test]
    fn disjoint_ranges_do_not_create_partition_across_hole() {
        let pcode = function(vec![vec![
            copy(0x1000, register_sized(0, 4), Varnode::constant(1, 4)),
            copy(0x1001, register_sized(8, 4), Varnode::constant(2, 4)),
        ]]);
        let successors = vec![vec![]];
        let predecessors = vec![vec![]];

        let ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa).unwrap();

        assert_eq!(
            ssa.inputs.keys().copied().collect::<Vec<_>>(),
            vec![
                SsaStorageKey {
                    space_id: REGISTER_SPACE_ID,
                    offset: 0,
                    size: 4,
                },
                SsaStorageKey {
                    space_id: REGISTER_SPACE_ID,
                    offset: 8,
                    size: 4,
                },
            ]
        );
    }

    #[test]
    fn address_space_guard_excludes_unsupported_and_invalid_ranges() {
        let unsupported = Varnode {
            space_id: 9,
            offset: 0x1000,
            size: 4,
            is_constant: false,
            constant_val: 0,
        };
        let zero_sized = register_sized(0, 0);
        let overflowing = register_sized(u64::MAX - 1, 4);
        let pcode = function(vec![vec![
            copy(0x1000, unsupported, Varnode::constant(1, 4)),
            copy(0x1001, zero_sized, Varnode::constant(2, 4)),
            copy(0x1002, overflowing, Varnode::constant(3, 4)),
        ]]);
        let successors = vec![vec![]];
        let predecessors = vec![vec![]];

        let ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa).unwrap();

        assert!(ssa.address_spaces.is_empty());
        assert!(ssa.values.is_empty());
        assert!(ssa.operation_outputs.is_empty());
    }

    #[test]
    fn affine_load_pointer_produces_exact_dynamic_guard() {
        let pointer = unique_sized(0, 8);
        let adjusted_pointer = unique_sized(8, 8);
        let pcode = function(vec![vec![
            copy(0x1000, pointer.clone(), Varnode::constant(0x2000, 8)),
            op(
                0x1001,
                PcodeOpcode::IntAdd,
                Some(adjusted_pointer.clone()),
                vec![pointer, Varnode::constant(0x10, 8)],
            ),
            op(
                0x1002,
                PcodeOpcode::Load,
                Some(unique_sized(0x20, 4)),
                vec![Varnode::constant(3, 8), adjusted_pointer],
            ),
        ]]);
        let successors = vec![vec![]];
        let predecessors = vec![vec![]];

        let ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa).unwrap();

        assert_eq!(
            ssa.dynamic_guards[&SsaOpSite { block: 0, op: 2 }],
            SsaDynamicGuard {
                site: SsaOpSite { block: 0, op: 2 },
                kind: SsaDynamicGuardKind::Load,
                effect: SsaMemoryEffect::Read,
                region: Some(SsaMemoryRegion::Ram),
                space_id: Some(3),
                minimum_offset: 0x2010,
                maximum_offset_exclusive: Some(0x2014),
                step: 0,
                precision: SsaGuardRangePrecision::Exact,
                call_target: None,
                call_effect_source: None,
            }
        );
    }

    #[test]
    fn phi_pointer_produces_bounded_guard_and_high_variable() {
        let pointer = register_sized(0, 8);
        let pcode = function(vec![
            vec![],
            vec![copy(0x1010, pointer.clone(), Varnode::constant(0x1000, 8))],
            vec![copy(0x1020, pointer.clone(), Varnode::constant(0x1100, 8))],
            vec![op(
                0x1030,
                PcodeOpcode::Load,
                Some(unique_sized(0, 4)),
                vec![Varnode::constant(3, 8), pointer],
            )],
        ]);
        let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let predecessors = vec![vec![], vec![0], vec![0], vec![1, 2]];

        let ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa).unwrap();

        let guard = &ssa.dynamic_guards[&SsaOpSite { block: 3, op: 0 }];
        assert_eq!(guard.precision, SsaGuardRangePrecision::Bounded);
        assert_eq!(guard.minimum_offset, 0x1000);
        assert_eq!(guard.maximum_offset_exclusive, Some(0x1104));
        assert_eq!(guard.step, 0x100);

        let phi = &ssa.phis[&3][0];
        assert_eq!(ssa.out_of_ssa_copies.len(), 2);
        let high = ssa.value_high_variables[phi.output.0 as usize];
        assert!(
            phi.operands
                .iter()
                .all(|operand| { ssa.value_high_variables[operand.value.0 as usize] == high })
        );
        assert_eq!(ssa.high_variables[high.0 as usize].members.len(), 3);
    }

    #[test]
    fn unknown_store_and_call_guard_live_high_variable() {
        let value = register_sized(0, 4);
        let unknown_pointer = register_sized(8, 8);
        let pcode = function(vec![vec![
            copy(0x1000, value.clone(), Varnode::constant(7, 4)),
            op(
                0x1001,
                PcodeOpcode::Store,
                None,
                vec![Varnode::constant(3, 8), unknown_pointer, value.clone()],
            ),
            op(
                0x1002,
                PcodeOpcode::Call,
                None,
                vec![Varnode::constant(0x4000, 8)],
            ),
            copy(0x1003, unique_sized(0, 4), value),
        ]]);
        let successors = vec![vec![]];
        let predecessors = vec![vec![]];

        let ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa).unwrap();

        let store_site = SsaOpSite { block: 0, op: 1 };
        let call_site = SsaOpSite { block: 0, op: 2 };
        assert_eq!(
            ssa.dynamic_guards[&store_site].precision,
            SsaGuardRangePrecision::Unknown
        );
        assert_eq!(
            ssa.dynamic_guards[&call_site],
            SsaDynamicGuard {
                site: call_site,
                kind: SsaDynamicGuardKind::Call,
                effect: SsaMemoryEffect::ReadWrite,
                region: None,
                space_id: None,
                minimum_offset: 0,
                maximum_offset_exclusive: None,
                step: 0,
                precision: SsaGuardRangePrecision::Unknown,
                call_target: None,
                call_effect_source: None,
            }
        );
        let definition = ssa.operation_outputs[&SsaOpSite { block: 0, op: 0 }][0].value;
        let high = ssa.value_high_variables[definition.0 as usize];
        assert_eq!(
            ssa.high_variables[high.0 as usize].crossing_guards,
            vec![store_site, call_site]
        );
    }

    #[test]
    fn validator_rejects_malformed_dynamic_guard() {
        let pcode = function(vec![vec![op(
            0x1000,
            PcodeOpcode::Call,
            None,
            vec![Varnode::constant(0x4000, 8)],
        )]]);
        let successors = vec![vec![]];
        let predecessors = vec![vec![]];
        let mut ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        ssa.dynamic_guards
            .get_mut(&SsaOpSite { block: 0, op: 0 })
            .unwrap()
            .region = Some(SsaMemoryRegion::Ram);

        assert!(matches!(
            validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa),
            Err(ScalarSsaValidationError::Shape(
                ScalarSsaShapeError::MalformedDynamicGuard { .. }
            ))
        ));
    }

    #[test]
    fn validator_rejects_malformed_high_variable_map() {
        let pcode = function(vec![vec![copy(
            0x1000,
            register(0),
            Varnode::constant(1, 4),
        )]]);
        let successors = vec![vec![]];
        let predecessors = vec![vec![]];
        let mut ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        ssa.value_high_variables.pop();

        assert!(matches!(
            validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa),
            Err(ScalarSsaValidationError::Shape(
                ScalarSsaShapeError::HighVariableMapLength { .. }
            ))
        ));
    }

    #[test]
    fn validator_rejects_reordered_access_pieces() {
        let pcode = function(vec![vec![
            copy(0x1000, register_sized(0, 8), Varnode::constant(1, 8)),
            copy(0x1001, register_sized(0, 4), Varnode::constant(2, 4)),
            copy(0x1002, unique_sized(0, 8), register_sized(0, 8)),
        ]]);
        let successors = vec![vec![]];
        let predecessors = vec![vec![]];
        let mut ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        ssa.operation_inputs
            .get_mut(&SsaUseSite {
                block: 0,
                op: 2,
                input: 0,
            })
            .unwrap()
            .reverse();

        assert!(matches!(
            validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa),
            Err(ScalarSsaValidationError::Shape(
                ScalarSsaShapeError::MalformedOperationInputPieces { .. }
            ))
        ));
    }

    #[test]
    fn typed_pure_call_has_no_memory_effect() {
        let pcode = function(vec![vec![op(
            0x1000,
            PcodeOpcode::Call,
            None,
            vec![Varnode::constant(0x4000, 8)],
        )]]);
        let successors = vec![vec![]];
        let predecessors = vec![vec![]];
        let mut context = PreviewTypeContext::default();
        context.call_targets.insert(0x4000, "pure_fn".to_string());
        context.call_effect_summaries.insert(
            "pure_fn".to_string(),
            NirCallEffectSummary {
                reads_memory: Some(false),
                writes_memory: Some(false),
                may_call_unknown: Some(false),
                source: Some(CallEffectSummarySource::FactStore),
                ..NirCallEffectSummary::default()
            },
        );
        let options = MlilPreviewOptions::default();

        let ssa = build_scalar_ssa_with_context(
            &pcode,
            &successors,
            &predecessors,
            &options,
            Some(&context),
        );
        validate_scalar_ssa_with_context(
            &pcode,
            &successors,
            &predecessors,
            &ssa,
            &options,
            Some(&context),
        )
        .unwrap();

        let guard = &ssa.dynamic_guards[&SsaOpSite { block: 0, op: 0 }];
        assert_eq!(guard.effect, SsaMemoryEffect::None);
        assert_eq!(guard.call_target.as_deref(), Some("pure_fn"));
        assert_eq!(
            guard.call_effect_source,
            Some(CallEffectSummarySource::FactStore)
        );
    }

    #[test]
    fn closed_call_summaries_narrow_read_and_write_effects() {
        let pcode = function(vec![vec![
            op(
                0x1000,
                PcodeOpcode::Call,
                None,
                vec![Varnode::constant(0x4000, 8)],
            ),
            op(
                0x1001,
                PcodeOpcode::Call,
                None,
                vec![Varnode::constant(0x4010, 8)],
            ),
            op(
                0x1002,
                PcodeOpcode::Call,
                None,
                vec![Varnode::constant(0x4020, 8)],
            ),
        ]]);
        let successors = vec![vec![]];
        let predecessors = vec![vec![]];
        let mut context = PreviewTypeContext::default();
        for (address, target) in [
            (0x4000, "reader"),
            (0x4010, "writer"),
            (0x4020, "open_world"),
        ] {
            context.call_targets.insert(address, target.to_string());
        }
        context.call_effect_summaries.insert(
            "reader".to_string(),
            NirCallEffectSummary {
                reads_memory: Some(true),
                writes_memory: Some(false),
                may_call_unknown: Some(false),
                ..NirCallEffectSummary::default()
            },
        );
        context.call_effect_summaries.insert(
            "writer".to_string(),
            NirCallEffectSummary {
                reads_memory: Some(false),
                writes_memory: Some(true),
                may_call_unknown: Some(false),
                ..NirCallEffectSummary::default()
            },
        );
        context.call_effect_summaries.insert(
            "open_world".to_string(),
            NirCallEffectSummary {
                reads_memory: Some(false),
                writes_memory: Some(false),
                may_call_unknown: None,
                ..NirCallEffectSummary::default()
            },
        );
        let ssa = build_scalar_ssa_with_context(
            &pcode,
            &successors,
            &predecessors,
            &MlilPreviewOptions::default(),
            Some(&context),
        );

        assert_eq!(
            ssa.dynamic_guards[&SsaOpSite { block: 0, op: 0 }].effect,
            SsaMemoryEffect::Read
        );
        assert_eq!(
            ssa.dynamic_guards[&SsaOpSite { block: 0, op: 1 }].effect,
            SsaMemoryEffect::Write
        );
        assert_eq!(
            ssa.dynamic_guards[&SsaOpSite { block: 0, op: 2 }].effect,
            SsaMemoryEffect::ReadWrite
        );
    }

    #[test]
    fn exact_ram_store_is_promoted_across_read_only_call_to_load() {
        let pcode = function(vec![vec![
            op(
                0x1000,
                PcodeOpcode::Store,
                None,
                vec![
                    Varnode::constant(3, 8),
                    Varnode::constant(0x2000, 8),
                    Varnode::constant(7, 4),
                ],
            ),
            op(
                0x1001,
                PcodeOpcode::Call,
                None,
                vec![Varnode::constant(0x4000, 8)],
            ),
            op(
                0x1002,
                PcodeOpcode::Load,
                Some(unique_sized(0, 4)),
                vec![Varnode::constant(3, 8), Varnode::constant(0x2000, 8)],
            ),
        ]]);
        let successors = vec![vec![]];
        let predecessors = vec![vec![]];
        let mut context = PreviewTypeContext::default();
        context.call_targets.insert(0x4000, "reader".to_string());
        context.call_effect_summaries.insert(
            "reader".to_string(),
            NirCallEffectSummary {
                reads_memory: Some(true),
                writes_memory: Some(false),
                may_call_unknown: Some(false),
                ..NirCallEffectSummary::default()
            },
        );
        let options = MlilPreviewOptions::default();
        let ssa = build_scalar_ssa_with_context(
            &pcode,
            &successors,
            &predecessors,
            &options,
            Some(&context),
        );

        let stored = ssa.memory_operation_outputs[&SsaOpSite { block: 0, op: 0 }][0].value;
        let loaded = ssa.memory_operation_inputs[&SsaOpSite { block: 0, op: 2 }][0].value;
        assert_eq!(stored, loaded);
        assert!(
            !ssa.memory_operation_outputs
                .contains_key(&SsaOpSite { block: 0, op: 1 })
        );
        assert_eq!(
            ssa.memory_values[stored.0 as usize].storage,
            SsaMemoryStorageKey {
                region: SsaMemoryRegion::Ram,
                space_id: 3,
                offset: 0x2000,
                size: 4,
            }
        );
    }

    #[test]
    fn unknown_store_kills_existing_promoted_location_without_inventing_one() {
        let pcode = function(vec![vec![
            op(
                0x1000,
                PcodeOpcode::Store,
                None,
                vec![
                    Varnode::constant(3, 8),
                    Varnode::constant(0x2000, 8),
                    Varnode::constant(1, 4),
                ],
            ),
            op(
                0x1001,
                PcodeOpcode::Store,
                None,
                vec![
                    Varnode::constant(3, 8),
                    register_sized(0, 8),
                    Varnode::constant(2, 4),
                ],
            ),
            op(
                0x1002,
                PcodeOpcode::Load,
                Some(unique(0)),
                vec![Varnode::constant(3, 8), Varnode::constant(0x2000, 8)],
            ),
        ]]);
        let successors = vec![vec![]];
        let predecessors = vec![vec![]];
        let ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa).unwrap();

        let unknown_store = SsaOpSite { block: 0, op: 1 };
        assert_eq!(
            ssa.dynamic_guards[&unknown_store].precision,
            SsaGuardRangePrecision::Unknown
        );
        assert_eq!(ssa.memory_inputs.len(), 1);
        assert_eq!(ssa.memory_operation_outputs[&unknown_store].len(), 1);
        assert_eq!(
            ssa.memory_operation_inputs[&SsaOpSite { block: 0, op: 2 }][0].value,
            ssa.memory_operation_outputs[&unknown_store][0].value
        );
    }

    #[test]
    fn signed_stack_location_is_promoted_and_unknown_call_kills_it() {
        let stack_pointer = register_sized(0x20, 8);
        let address = unique_sized(0, 8);
        let pcode = function(vec![vec![
            op(
                0x1000,
                PcodeOpcode::IntAdd,
                Some(address.clone()),
                vec![stack_pointer, Varnode::constant(-8, 8)],
            ),
            op(
                0x1001,
                PcodeOpcode::Store,
                None,
                vec![
                    Varnode::constant(3, 8),
                    address.clone(),
                    Varnode::constant(7, 4),
                ],
            ),
            op(
                0x1002,
                PcodeOpcode::CallInd,
                None,
                vec![register_sized(0x30, 8)],
            ),
            op(
                0x1003,
                PcodeOpcode::Load,
                Some(unique_sized(0x10, 4)),
                vec![Varnode::constant(3, 8), address],
            ),
        ]]);
        let successors = vec![vec![]];
        let predecessors = vec![vec![]];
        let options = MlilPreviewOptions {
            cspec_stack_pointer_offset: Some(0x20),
            ..MlilPreviewOptions::default()
        };
        let ssa = build_scalar_ssa_with_context(&pcode, &successors, &predecessors, &options, None);
        validate_scalar_ssa_with_context(&pcode, &successors, &predecessors, &ssa, &options, None)
            .unwrap();

        let store = ssa.memory_operation_outputs[&SsaOpSite { block: 0, op: 1 }][0].value;
        let call = ssa.memory_operation_outputs[&SsaOpSite { block: 0, op: 2 }][0].value;
        let load = ssa.memory_operation_inputs[&SsaOpSite { block: 0, op: 3 }][0].value;
        assert_ne!(store, call);
        assert_eq!(call, load);
        assert_eq!(
            ssa.memory_values[load.0 as usize].storage,
            SsaMemoryStorageKey {
                region: SsaMemoryRegion::Stack,
                space_id: 3,
                offset: -8,
                size: 4,
            }
        );
    }

    #[test]
    fn bounded_store_kills_only_an_existing_overlapping_ram_partition() {
        let pointer = register_sized(0, 8);
        let pcode = function(vec![
            vec![op(
                0x1000,
                PcodeOpcode::Store,
                None,
                vec![
                    Varnode::constant(3, 8),
                    Varnode::constant(0x2000, 8),
                    Varnode::constant(1, 4),
                ],
            )],
            vec![copy(0x1010, pointer.clone(), Varnode::constant(0x2000, 8))],
            vec![copy(0x1020, pointer.clone(), Varnode::constant(0x2010, 8))],
            vec![
                op(
                    0x1030,
                    PcodeOpcode::Store,
                    None,
                    vec![Varnode::constant(3, 8), pointer, Varnode::constant(2, 4)],
                ),
                op(
                    0x1031,
                    PcodeOpcode::Load,
                    Some(unique(0)),
                    vec![Varnode::constant(3, 8), Varnode::constant(0x2000, 8)],
                ),
            ],
        ]);
        let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let predecessors = vec![vec![], vec![0], vec![0], vec![1, 2]];
        let ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa).unwrap();

        let bounded_site = SsaOpSite { block: 3, op: 0 };
        assert_eq!(
            ssa.dynamic_guards[&bounded_site].precision,
            SsaGuardRangePrecision::Bounded
        );
        assert_eq!(ssa.memory_operation_outputs[&bounded_site].len(), 1);
        assert_eq!(
            ssa.memory_operation_inputs[&SsaOpSite { block: 3, op: 1 }][0].value,
            ssa.memory_operation_outputs[&bounded_site][0].value
        );
        assert_eq!(ssa.memory_inputs.len(), 1);
    }

    #[test]
    fn promoted_memory_receives_phi_at_cfg_join() {
        let pcode = function(vec![
            vec![],
            vec![op(
                0x1010,
                PcodeOpcode::Store,
                None,
                vec![
                    Varnode::constant(3, 8),
                    Varnode::constant(0x2000, 8),
                    Varnode::constant(1, 4),
                ],
            )],
            vec![],
            vec![op(
                0x1030,
                PcodeOpcode::Load,
                Some(unique(0)),
                vec![Varnode::constant(3, 8), Varnode::constant(0x2000, 8)],
            )],
        ]);
        let successors = vec![vec![1, 2], vec![3], vec![3], vec![]];
        let predecessors = vec![vec![], vec![0], vec![0], vec![1, 2]];
        let ssa = build_scalar_ssa(&pcode, &successors, &predecessors);
        validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa).unwrap();

        let phi = &ssa.memory_phis[&3][0];
        assert_eq!(phi.operands.len(), 2);
        assert_eq!(
            ssa.memory_operation_inputs[&SsaOpSite { block: 3, op: 0 }][0].value,
            phi.output
        );
    }

    #[test]
    fn copy_coalescing_requires_nonintersecting_value_covers() {
        let source = register(0);
        let destination = register(4);
        let noninterfering = function(vec![vec![
            copy(0x1000, source.clone(), Varnode::constant(1, 4)),
            copy(0x1001, destination.clone(), source.clone()),
            copy(0x1002, unique(0), destination.clone()),
        ]]);
        let successors = vec![vec![]];
        let predecessors = vec![vec![]];
        let merged = build_scalar_ssa(&noninterfering, &successors, &predecessors);
        let source_value = merged.operation_outputs[&SsaOpSite { block: 0, op: 0 }][0].value;
        let destination_value = merged.operation_outputs[&SsaOpSite { block: 0, op: 1 }][0].value;
        assert_eq!(
            merged.value_high_variables[source_value.0 as usize],
            merged.value_high_variables[destination_value.0 as usize]
        );

        let interfering = function(vec![vec![
            copy(0x1000, source.clone(), Varnode::constant(1, 4)),
            copy(0x1001, destination.clone(), source.clone()),
            copy(0x1002, unique(0), source),
            copy(0x1003, unique(4), destination),
        ]]);
        let separated = build_scalar_ssa(&interfering, &successors, &predecessors);
        let source_value = separated.operation_outputs[&SsaOpSite { block: 0, op: 0 }][0].value;
        let destination_value =
            separated.operation_outputs[&SsaOpSite { block: 0, op: 1 }][0].value;
        assert_ne!(
            separated.value_high_variables[source_value.0 as usize],
            separated.value_high_variables[destination_value.0 as usize]
        );
    }
}
