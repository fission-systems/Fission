//! Exact-storage scalar SSA construction for the first Heritage phase.
//!
//! This runs on the original lifted CFG, before structuring removes
//! irreducible edges. Register and unique varnodes participate when their
//! storage tuple matches exactly; overlapping/subregister handling and memory
//! SSA are deliberately later phases.

use super::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ScalarSsaValidationError {
    Shape(ScalarSsaShapeError),
    MissingOperationOutput(SsaOpSite),
    MissingOperationInput(SsaUseSite),
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

fn scalar_storage(varnode: &Varnode) -> Option<SsaStorageKey> {
    (!varnode.is_constant
        && varnode.size != 0
        && (is_register_space_id(varnode.space_id) || is_unique_space_id(varnode.space_id)))
    .then_some(SsaStorageKey {
        space_id: varnode.space_id,
        offset: varnode.offset,
        size: varnode.size,
    })
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
    let dominance = Dominance::analyze(successors, predecessors);
    if pcode.blocks.is_empty() || dominance.reachable.is_empty() {
        return NirScalarSsa::default();
    }

    let mut storages = BTreeSet::new();
    let mut definition_blocks: BTreeMap<SsaStorageKey, BTreeSet<usize>> = BTreeMap::new();
    for &block in &dominance.reachable {
        let Some(pcode_block) = pcode.blocks.get(block) else {
            continue;
        };
        for op in &pcode_block.ops {
            for input in &op.inputs {
                if let Some(storage) = scalar_storage(input) {
                    storages.insert(storage);
                }
            }
            if let Some(storage) = op.output.as_ref().and_then(scalar_storage) {
                storages.insert(storage);
                definition_blocks.entry(storage).or_default().insert(block);
            }
        }
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

    let mut ssa = NirScalarSsa::default();
    let mut stacks: BTreeMap<SsaStorageKey, Vec<SsaValueId>> = BTreeMap::new();
    for storage in storages {
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
            let Some(storage) = op.output.as_ref().and_then(scalar_storage) else {
                continue;
            };
            let site = SsaOpSite {
                block: block as u32,
                op: op_index as u32,
            };
            let output = allocate_value(&mut ssa, storage, SsaValueDefinition::Operation(site));
            ssa.operation_outputs.insert(site, output);
        }
    }

    let mut phi_operands: BTreeMap<(usize, SsaStorageKey), Vec<SsaPhiOperand>> = BTreeMap::new();
    rename_block(
        0,
        pcode,
        successors,
        &dominance,
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

    ssa
}

#[allow(clippy::too_many_arguments)]
fn rename_block(
    block: usize,
    pcode: &PcodeFunction,
    successors: &[Vec<usize>],
    dominance: &Dominance,
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
                let Some(storage) = scalar_storage(input) else {
                    continue;
                };
                let value = *stacks
                    .get(&storage)
                    .and_then(|stack| stack.last())
                    .expect("eligible storage has an input value");
                ssa.operation_inputs.insert(
                    SsaUseSite {
                        block: block as u32,
                        op: op_index as u32,
                        input: input_index as u32,
                    },
                    value,
                );
            }

            let site = SsaOpSite {
                block: block as u32,
                op: op_index as u32,
            };
            if let Some(output) = ssa.operation_outputs.get(&site).copied() {
                let storage = ssa.value(output).expect("allocated output").storage;
                stacks
                    .get_mut(&storage)
                    .expect("output storage has an input value")
                    .push(output);
                pushed.push(storage);
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

pub(super) fn validate_scalar_ssa(
    pcode: &PcodeFunction,
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
    ssa: &NirScalarSsa,
) -> Result<(), ScalarSsaValidationError> {
    ssa.validate_shape()
        .map_err(ScalarSsaValidationError::Shape)?;
    let dominance = Dominance::analyze(successors, predecessors);

    for &block in &dominance.reachable {
        let Some(pcode_block) = pcode.blocks.get(block) else {
            continue;
        };
        for (op_index, op) in pcode_block.ops.iter().enumerate() {
            let output_site = SsaOpSite {
                block: block as u32,
                op: op_index as u32,
            };
            if let Some(expected) = op.output.as_ref().and_then(scalar_storage) {
                let value_id = *ssa.operation_outputs.get(&output_site).ok_or(
                    ScalarSsaValidationError::MissingOperationOutput(output_site),
                )?;
                let actual = ssa.value(value_id).expect("shape validated").storage;
                if actual != expected {
                    return Err(ScalarSsaValidationError::OperationStorageMismatch {
                        site: output_site,
                        expected,
                        actual,
                    });
                }
            }

            for (input_index, input) in op.inputs.iter().enumerate() {
                let Some(expected) = scalar_storage(input) else {
                    continue;
                };
                let use_site = SsaUseSite {
                    block: block as u32,
                    op: op_index as u32,
                    input: input_index as u32,
                };
                let value_id = *ssa
                    .operation_inputs
                    .get(&use_site)
                    .ok_or(ScalarSsaValidationError::MissingOperationInput(use_site))?;
                let actual = ssa.value(value_id).expect("shape validated").storage;
                if actual != expected {
                    return Err(ScalarSsaValidationError::UseStorageMismatch {
                        site: use_site,
                        expected,
                        actual,
                    });
                }
                if !value_dominates_use(ssa, &dominance, value_id, use_site) {
                    return Err(ScalarSsaValidationError::NonDominatingUse {
                        site: use_site,
                        value: value_id,
                    });
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
        Varnode {
            space_id: REGISTER_SPACE_ID,
            offset,
            size: 4,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn unique(offset: u64) -> Varnode {
        Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset,
            size: 4,
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
            }],
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
            }],
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
            }],
            ssa.operation_outputs[&SsaOpSite { block: 0, op: 0 }]
        );
        assert_eq!(
            ssa.operation_inputs[&SsaUseSite {
                block: 0,
                op: 3,
                input: 0,
            }],
            ssa.operation_outputs[&SsaOpSite { block: 0, op: 2 }]
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
        let sibling_value = ssa.operation_outputs[&SsaOpSite { block: 1, op: 0 }];
        ssa.operation_inputs.insert(
            SsaUseSite {
                block: 3,
                op: 0,
                input: 0,
            },
            sibling_value,
        );

        assert!(matches!(
            validate_scalar_ssa(&pcode, &successors, &predecessors, &ssa),
            Err(ScalarSsaValidationError::NonDominatingUse { .. })
        ));
    }
}
