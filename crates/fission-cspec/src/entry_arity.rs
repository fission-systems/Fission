//! Entry-block parameter-arity inference from raw p-code and a [`RegisterNamer`].

use crate::RegisterNamer;
use fission_pcode_ir::pcode::Varnode;
use fission_pcode_ir::space::is_register_varnode;

type HashMap<K, V> = std::collections::HashMap<K, V, rustc_hash::FxBuildHasher>;
type HashSet<K> = std::collections::HashSet<K, rustc_hash::FxBuildHasher>;

pub fn infer_entry_register_param_arity(
    pcode: &fission_pcode_ir::pcode::PcodeFunction,
    namer: &RegisterNamer,
) -> Option<usize> {
    use fission_pcode_ir::pcode::PcodeOpcode;
    use std::collections::{HashMap, HashSet, VecDeque};

    let entry = pcode.blocks.first()?;
    let num_params = namer.int_param_offsets.len();
    if num_params == 0 {
        return None;
    }

    let block_map: HashMap<u32, &fission_pcode_ir::pcode::PcodeBasicBlock> =
        pcode.blocks.iter().map(|b| (b.index, b)).collect();

    let has_no_succs = pcode.blocks.iter().all(|b| b.successors.is_empty());
    let mut block_successors: HashMap<u32, Vec<u32>> = HashMap::default();
    if has_no_succs && pcode.blocks.len() > 1 {
        let address_to_index = fission_pcode_ir::block_cfg::build_address_to_index_map(pcode);
        let layout_fallthrough = fission_pcode_ir::block_cfg::build_layout_fallthrough_map(pcode);
        let succ_indices = fission_pcode_ir::block_cfg::build_successor_index_map(
            pcode,
            &address_to_index,
            &layout_fallthrough,
        );
        for (idx, block) in pcode.blocks.iter().enumerate() {
            let succs_for_block = succ_indices[idx]
                .iter()
                .map(|&s_idx| pcode.blocks[s_idx].index)
                .collect::<Vec<u32>>();
            block_successors.insert(block.index, succs_for_block);
        }
    } else {
        for block in &pcode.blocks {
            block_successors.insert(block.index, block.successors.clone());
        }
    }

    let mut detected_params: HashSet<usize> = HashSet::default();
    // A register whose *every* appearance anywhere in the function is as
    // the value operand of a `Store` (i.e. it's written to memory once and
    // never read back, as itself or via that memory location) is far more
    // likely a prologue register-save/stack-alignment-padding spill (e.g.
    // ARM32's `push {r3, lr}`, padding a lone `push {lr}` out to 8 bytes --
    // r3 never carries a real argument there) than a genuine incoming
    // parameter. A real parameter that gets spilled to its home stack slot
    // still gets read back from that slot somewhere when the function
    // actually uses it; one that's spilled and never touched again, in any
    // form, wasn't a parameter reference at all. Tracked function-wide
    // (not just the entry-block scan below) since the "was it ever read
    // back" evidence can live anywhere.
    let mut store_value_only: HashSet<usize> = HashSet::default();
    let mut seen_any_use: HashSet<usize> = HashSet::default();
    for block in &pcode.blocks {
        for op in &block.ops {
            let store_value_idx = matches!(op.opcode, PcodeOpcode::Store)
                .then(|| op.inputs.len().checked_sub(1))
                .flatten();
            for (i, input) in op.inputs.iter().enumerate() {
                if input.is_constant || !is_register_varnode(input) {
                    continue;
                }
                let Some((_, Some(param_index))) =
                    namer.register_name_with_param_owned(input.offset, input.size)
                else {
                    continue;
                };
                let is_store_value = store_value_idx == Some(i);
                if is_store_value {
                    if !seen_any_use.contains(&param_index) {
                        store_value_only.insert(param_index);
                    }
                } else {
                    store_value_only.remove(&param_index);
                }
                seen_any_use.insert(param_index);
            }
        }
    }

    let mut visited_active_params: HashMap<u32, HashSet<usize>> = HashMap::default();
    let mut queue = VecDeque::new();

    // Some instruction idioms mention the destination register as an input
    // even though the result is independent of its incoming value. Counting
    // such an input as entry-parameter evidence turns zeroing instructions
    // like `xor r9d, r9d` into a phantom formal parameter. Keep this rule in
    // the p-code semantic layer: register offsets and ABI slots still come
    // from the register namer, while the operation determines whether the
    // old value is actually consumed.
    let input_is_overwritten_without_read =
        |op: &fission_pcode_ir::pcode::PcodeOp, input: &fission_pcode_ir::pcode::Varnode| -> bool {
            let Some(output) = op.output.as_ref() else {
                return false;
            };
            let same_storage = |lhs: &Varnode, rhs: &Varnode| {
                !lhs.is_constant
                    && !rhs.is_constant
                    && lhs.space_id == rhs.space_id
                    && lhs.offset == rhs.offset
                    && lhs.size == rhs.size
            };
            if !same_storage(output, input) {
                return false;
            }
            match op.opcode {
                PcodeOpcode::IntXor | PcodeOpcode::IntSub => op
                    .inputs
                    .get(1)
                    .is_some_and(|other| same_storage(input, other)),
                PcodeOpcode::IntAnd | PcodeOpcode::IntMult => op
                    .inputs
                    .get(1)
                    .is_some_and(|other| other.is_constant && other.constant_val == 0),
                _ => false,
            }
        };

    let initial_active: HashSet<usize> = (0..num_params).collect();
    visited_active_params.insert(entry.index, initial_active.clone());
    queue.push_back((entry.index, initial_active));

    while let Some((block_idx, active_params)) = queue.pop_front() {
        let Some(block) = block_map.get(&block_idx) else {
            continue;
        };

        let mut current_active = active_params;
        for op in &block.ops {
            // 1. Check inputs first
            for input in &op.inputs {
                if input.is_constant
                    || !is_register_varnode(input)
                    || input_is_overwritten_without_read(op, input)
                {
                    continue;
                }
                if let Some((_, Some(param_index))) =
                    namer.register_name_with_param_owned(input.offset, input.size)
                {
                    if current_active.contains(&param_index) {
                        detected_params.insert(param_index);
                    }
                }
            }

            // 2. If call, clobber scratch/parameter registers (remove from current_active)
            if matches!(op.opcode, PcodeOpcode::Call | PcodeOpcode::CallInd) {
                current_active.clear();
            }

            // 3. Check output to see if it writes/defines a parameter register
            if let Some(output) = &op.output {
                if !output.is_constant && is_register_varnode(output) {
                    if let Some((_, Some(param_index))) =
                        namer.register_name_with_param_owned(output.offset, output.size)
                    {
                        current_active.remove(&param_index);
                    }
                }
            }
        }

        // Propagate current_active to successors
        if let Some(succs) = block_successors.get(&block_idx) {
            for &succ_idx in succs {
                let succ_visited = visited_active_params.entry(succ_idx).or_default();
                let mut new_active = HashSet::default();
                for &param in &current_active {
                    if !succ_visited.contains(&param) {
                        new_active.insert(param);
                    }
                }
                if !new_active.is_empty() {
                    for &param in &new_active {
                        succ_visited.insert(param);
                    }
                    queue.push_back((succ_idx, new_active));
                }
            }
        }
    }

    // Drop indices whose only function-wide evidence is a spill-and-never-
    // read-again store (see `store_value_only` above) before taking the
    // max -- an unused *middle* parameter is completely ordinary (real
    // functions often ignore some of their arguments) and must not be
    // penalized, but an index whose sole appearance is prologue
    // padding/register-save noise shouldn't count as parameter evidence at
    // all.
    detected_params
        .iter()
        .filter(|idx| !store_value_only.contains(idx))
        .max()
        .map(|&index| index + 1)
}
