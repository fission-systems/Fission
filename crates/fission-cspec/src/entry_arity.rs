//! Entry-block parameter-arity inference from raw p-code and a [`RegisterNamer`].

use crate::RegisterNamer;
use fission_pcode_ir::pcode::{PcodeFunction, PcodeOp, PcodeOpcode, Varnode};
use fission_pcode_ir::space::is_register_space_id;
use std::collections::{HashMap as StdHashMap, HashSet as StdHashSet, VecDeque};

type HashMap<K, V> = StdHashMap<K, V, rustc_hash::FxBuildHasher>;
type HashSet<K> = StdHashSet<K, rustc_hash::FxBuildHasher>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum StackOrigin {
    EntrySp,
    FramePtr,
    AlignedSp(u32, usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StackVal {
    Const(i64),
    Exact(StackOrigin, i64),
    Dynamic(StackOrigin),
    NonStack,
}

impl StackVal {
    fn rank(self) -> u8 {
        match self {
            Self::Const(_) | Self::Exact(_, _) => 0,
            Self::Dynamic(_) => 1,
            Self::NonStack => 2,
        }
    }

    fn join(self, other: Self) -> Self {
        if self == other {
            return self;
        }
        match (self, other) {
            (Self::Exact(o1, _) | Self::Dynamic(o1), Self::Exact(o2, _) | Self::Dynamic(o2))
                if o1 == o2 =>
            {
                Self::Dynamic(o1)
            }
            _ => Self::NonStack,
        }
    }

    fn origin(self) -> Option<StackOrigin> {
        match self {
            Self::Exact(orig, _) | Self::Dynamic(orig) => Some(orig),
            Self::Const(_) | Self::NonStack => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum StackSpill {
    Exact {
        origin: StackOrigin,
        offset: i64,
        size: u32,
        param_index: usize,
    },
    Dynamic {
        origin: StackOrigin,
        param_index: usize,
    },
}

impl StackSpill {
    fn origin(self) -> StackOrigin {
        match self {
            Self::Exact { origin, .. } | Self::Dynamic { origin, .. } => origin,
        }
    }

    fn param_index(self) -> usize {
        match self {
            Self::Exact { param_index, .. } | Self::Dynamic { param_index, .. } => param_index,
        }
    }

    fn matches_load(self, load_addr: StackVal, load_size: u32) -> bool {
        match (self, load_addr) {
            (
                Self::Exact {
                    origin: s_orig,
                    offset: s_off,
                    size: s_size,
                    ..
                },
                StackVal::Exact(l_orig, l_off),
            ) => {
                if s_orig != l_orig {
                    return false;
                }
                let s_start = i128::from(s_off);
                let s_end = s_start + i128::from(s_size.max(1));
                let l_start = i128::from(l_off);
                let l_end = l_start + i128::from(load_size.max(1));
                s_start < l_end && l_start < s_end
            }
            (Self::Exact { origin: s_orig, .. }, StackVal::Dynamic(l_orig))
            | (
                Self::Dynamic { origin: s_orig, .. },
                StackVal::Exact(l_orig, _) | StackVal::Dynamic(l_orig),
            ) => s_orig == l_orig,
            _ => false,
        }
    }
}

fn sign_extend(val: u64, size: u32) -> Option<i64> {
    let bits = size.saturating_mul(8);
    if bits == 0 || bits > 64 {
        return None;
    }
    if bits == 64 {
        Some(val as i64)
    } else {
        let shift = 64 - bits;
        Some(((val << shift) as i64) >> shift)
    }
}

fn varnode_key(vn: &Varnode) -> Option<(u64, u64)> {
    (!vn.is_constant).then_some((vn.space_id, vn.offset))
}

fn initial_val_for_key(
    key: (u64, u64),
    sp_keys: &HashSet<(u64, u64)>,
    fp_keys: &HashSet<(u64, u64)>,
) -> StackVal {
    if sp_keys.contains(&key) {
        StackVal::Exact(StackOrigin::EntrySp, 0)
    } else if fp_keys.contains(&key) {
        StackVal::Exact(StackOrigin::FramePtr, 0)
    } else {
        StackVal::NonStack
    }
}

fn eval_varnode(
    env: &HashMap<(u64, u64), StackVal>,
    vn: &Varnode,
    sp_keys: &HashSet<(u64, u64)>,
    fp_keys: &HashSet<(u64, u64)>,
) -> StackVal {
    if vn.is_constant {
        return sign_extend(vn.constant_val as u64, vn.size)
            .map_or(StackVal::NonStack, StackVal::Const);
    }
    let Some(key) = varnode_key(vn) else {
        return StackVal::NonStack;
    };
    env.get(&key)
        .copied()
        .unwrap_or_else(|| initial_val_for_key(key, sp_keys, fp_keys))
}

fn step_stack_env(
    env: &mut HashMap<(u64, u64), StackVal>,
    block_index: u32,
    op_idx: usize,
    op: &PcodeOp,
    sp_keys: &HashSet<(u64, u64)>,
    fp_keys: &HashSet<(u64, u64)>,
) {
    if matches!(op.opcode, PcodeOpcode::Call | PcodeOpcode::CallInd) {
        // Calls clobber scratch/temp state; keep stack & frame pointers intact.
        env.retain(|k, _| sp_keys.contains(k) || fp_keys.contains(k));
    }

    let Some(output) = op.output.as_ref() else {
        return;
    };
    let Some(out_key) = varnode_key(output) else {
        return;
    };

    let next_val = match op.opcode {
        PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
            op.inputs.first().map_or(StackVal::NonStack, |vn| {
                eval_varnode(env, vn, sp_keys, fp_keys)
            })
        }
        PcodeOpcode::IntAdd => match (op.inputs.first(), op.inputs.get(1)) {
            (Some(lhs), Some(rhs)) => {
                match (
                    eval_varnode(env, lhs, sp_keys, fp_keys),
                    eval_varnode(env, rhs, sp_keys, fp_keys),
                ) {
                    (StackVal::Exact(o, off), StackVal::Const(c))
                    | (StackVal::Const(c), StackVal::Exact(o, off)) => off
                        .checked_add(c)
                        .map_or(StackVal::Dynamic(o), |sum| StackVal::Exact(o, sum)),
                    (StackVal::Exact(o, _) | StackVal::Dynamic(o), _)
                    | (_, StackVal::Exact(o, _) | StackVal::Dynamic(o)) => StackVal::Dynamic(o),
                    (StackVal::Const(a), StackVal::Const(b)) => {
                        a.checked_add(b).map_or(StackVal::NonStack, StackVal::Const)
                    }
                    _ => StackVal::NonStack,
                }
            }
            _ => StackVal::NonStack,
        },
        PcodeOpcode::IntSub => match (op.inputs.first(), op.inputs.get(1)) {
            (Some(lhs), Some(rhs)) => {
                match (
                    eval_varnode(env, lhs, sp_keys, fp_keys),
                    eval_varnode(env, rhs, sp_keys, fp_keys),
                ) {
                    (StackVal::Exact(o, off), StackVal::Const(c)) => off
                        .checked_sub(c)
                        .map_or(StackVal::Dynamic(o), |diff| StackVal::Exact(o, diff)),
                    (StackVal::Exact(o, _) | StackVal::Dynamic(o), _) => StackVal::Dynamic(o),
                    (StackVal::Const(a), StackVal::Const(b)) => {
                        a.checked_sub(b).map_or(StackVal::NonStack, StackVal::Const)
                    }
                    _ => StackVal::NonStack,
                }
            }
            _ => StackVal::NonStack,
        },
        PcodeOpcode::IntAnd => match (op.inputs.first(), op.inputs.get(1)) {
            (Some(lhs), Some(rhs)) => {
                match (
                    eval_varnode(env, lhs, sp_keys, fp_keys),
                    eval_varnode(env, rhs, sp_keys, fp_keys),
                ) {
                    (StackVal::Exact(_, _) | StackVal::Dynamic(_), StackVal::Const(c))
                    | (StackVal::Const(c), StackVal::Exact(_, _) | StackVal::Dynamic(_))
                        if c < 0 =>
                    {
                        StackVal::Exact(StackOrigin::AlignedSp(block_index, op_idx), 0)
                    }
                    (StackVal::Const(a), StackVal::Const(b)) => StackVal::Const(a & b),
                    _ => StackVal::NonStack,
                }
            }
            _ => StackVal::NonStack,
        },
        _ => StackVal::NonStack,
    };

    env.insert(out_key, next_val);
}

fn input_is_overwritten_without_read(op: &PcodeOp, input: &Varnode) -> bool {
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
        PcodeOpcode::IntXor | PcodeOpcode::IntSub => {
            op.inputs.len() >= 2 && same_storage(&op.inputs[0], &op.inputs[1])
        }
        PcodeOpcode::IntAnd | PcodeOpcode::IntMult => op
            .inputs
            .iter()
            .any(|other| other.is_constant && other.constant_val == 0),
        _ => false,
    }
}

fn is_dead_epilogue_restore(
    block: &fission_pcode_ir::pcode::PcodeBasicBlock,
    op_idx: usize,
    out: &Varnode,
    param_index: usize,
    block_successors: &HashMap<u32, Vec<u32>>,
    namer: &RegisterNamer,
) -> bool {
    if namer.integer_param_slot_for_varnode(out) != Some(param_index) {
        return false;
    }
    if namer.is_primary_return_register(out) {
        return false;
    }
    if block_successors
        .get(&block.index)
        .is_none_or(|succs| !succs.is_empty())
    {
        return false;
    }

    // In a leaf/exit block (e.g., `pop {r3, pc}`), restoring the register right
    // before return is a callee-save/alignment restore rather than a parameter
    // read as long as no subsequent op calls a subroutine or consumes the register.
    for later in &block.ops[op_idx + 1..] {
        if matches!(later.opcode, PcodeOpcode::Call | PcodeOpcode::CallInd) {
            return false;
        }
        for inp in &later.inputs {
            if !inp.is_constant
                && !input_is_overwritten_without_read(later, inp)
                && namer.integer_param_slot_for_varnode(inp) == Some(param_index)
            {
                return false;
            }
        }
        if let Some(later_out) = &later.output
            && !later_out.is_constant
            && namer.integer_param_slot_for_varnode(later_out) == Some(param_index)
        {
            return true;
        }
    }
    true
}

pub fn infer_entry_register_param_arity(
    pcode: &PcodeFunction,
    namer: &RegisterNamer,
) -> Option<usize> {
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

    // Collect SP and FP storage keys across all varnodes touched in the function,
    // plus architectural non-SP/FP registers for Call/CallInd stack-escape checks.
    let mut sp_keys: HashSet<(u64, u64)> = HashSet::default();
    let mut fp_keys: HashSet<(u64, u64)> = HashSet::default();
    let mut arch_reg_keys: HashSet<(u64, u64)> = HashSet::default();
    for block in &pcode.blocks {
        for op in &block.ops {
            for vn in op.inputs.iter().chain(op.output.iter()) {
                let Some(key) = varnode_key(vn) else {
                    continue;
                };
                if namer.is_stack_pointer_varnode(vn) {
                    sp_keys.insert(key);
                } else if namer.is_frame_pointer_varnode(vn) {
                    fp_keys.insert(key);
                } else if is_register_space_id(vn.space_id)
                    || namer.is_architectural_register_varnode(vn)
                {
                    arch_reg_keys.insert(key);
                }
            }
        }
    }

    // 1. Propagate stack-pointer / frame-pointer affine facts across reachable blocks.
    let mut entry_env: HashMap<(u64, u64), StackVal> = HashMap::default();
    for &k in &sp_keys {
        entry_env.insert(k, StackVal::Exact(StackOrigin::EntrySp, 0));
    }
    for &k in &fp_keys {
        entry_env.insert(k, StackVal::Exact(StackOrigin::FramePtr, 0));
    }

    let mut block_in_envs: HashMap<u32, HashMap<(u64, u64), StackVal>> = HashMap::default();
    block_in_envs.insert(entry.index, entry_env);
    let mut stack_queue = VecDeque::new();
    stack_queue.push_back(entry.index);
    let mut stack_visits: HashMap<u32, usize> = HashMap::default();
    let max_visits_per_block = pcode.blocks.len().saturating_mul(8).max(32);

    while let Some(b_idx) = stack_queue.pop_front() {
        let visits = stack_visits.entry(b_idx).or_insert(0);
        if *visits >= max_visits_per_block {
            continue;
        }
        *visits += 1;

        let Some(block) = block_map.get(&b_idx) else {
            continue;
        };
        let mut env = block_in_envs.get(&b_idx).cloned().unwrap_or_default();
        for (op_idx, op) in block.ops.iter().enumerate() {
            step_stack_env(&mut env, b_idx, op_idx, op, &sp_keys, &fp_keys);
        }

        let Some(succs) = block_successors.get(&b_idx) else {
            continue;
        };
        for &succ_idx in succs {
            if let Some(existing) = block_in_envs.get_mut(&succ_idx) {
                let mut changed = false;
                let keys: Vec<(u64, u64)> = existing
                    .keys()
                    .copied()
                    .chain(env.keys().copied())
                    .collect();
                for k in keys {
                    let old = existing
                        .get(&k)
                        .copied()
                        .unwrap_or_else(|| initial_val_for_key(k, &sp_keys, &fp_keys));
                    let new = env
                        .get(&k)
                        .copied()
                        .unwrap_or_else(|| initial_val_for_key(k, &sp_keys, &fp_keys));
                    let joined = old.join(new);
                    if joined != old && joined.rank() >= old.rank() {
                        existing.insert(k, joined);
                        changed = true;
                    }
                }
                if changed {
                    stack_queue.push_back(succ_idx);
                }
            } else {
                block_in_envs.insert(succ_idx, env.clone());
                stack_queue.push_back(succ_idx);
            }
        }
    }

    // 2. Track live entry parameter registers from entry, distinguishing non-stack stores
    //    (e.g. setter `*p = param_2`, confirmed immediately) from stack spills (recorded for
    //    read-back / escape verification).
    let mut confirmed_params: HashSet<usize> = HashSet::default();
    let mut stack_spills: Vec<StackSpill> = Vec::new();
    let mut visited_active_params: HashMap<u32, HashSet<usize>> = HashMap::default();
    let mut queue = VecDeque::new();

    let initial_active: HashSet<usize> = (0..num_params).collect();
    visited_active_params.insert(entry.index, initial_active.clone());
    queue.push_back((entry.index, initial_active));

    while let Some((block_idx, active_params)) = queue.pop_front() {
        let Some(block) = block_map.get(&block_idx) else {
            continue;
        };

        let mut env = block_in_envs.get(&block_idx).cloned().unwrap_or_default();
        let mut current_active = active_params;
        for (op_idx, op) in block.ops.iter().enumerate() {
            let store_value_idx = matches!(op.opcode, PcodeOpcode::Store)
                .then(|| op.inputs.len().checked_sub(1))
                .flatten();

            for (i, input) in op.inputs.iter().enumerate() {
                if input.is_constant || input_is_overwritten_without_read(op, input) {
                    continue;
                }
                let Some(param_index) = namer.integer_param_slot_for_varnode(input) else {
                    continue;
                };
                if !current_active.contains(&param_index) {
                    continue;
                }
                if store_value_idx == Some(i) {
                    let store_addr = op.inputs.get(1).map_or(StackVal::NonStack, |ptr| {
                        eval_varnode(&env, ptr, &sp_keys, &fp_keys)
                    });
                    match store_addr {
                        StackVal::Exact(origin, offset) => {
                            stack_spills.push(StackSpill::Exact {
                                origin,
                                offset,
                                size: input.size,
                                param_index,
                            });
                        }
                        StackVal::Dynamic(origin) => {
                            stack_spills.push(StackSpill::Dynamic {
                                origin,
                                param_index,
                            });
                        }
                        StackVal::NonStack | StackVal::Const(_) => {
                            confirmed_params.insert(param_index);
                        }
                    }
                } else {
                    confirmed_params.insert(param_index);
                }
            }

            if matches!(op.opcode, PcodeOpcode::Call | PcodeOpcode::CallInd) {
                current_active.clear();
            }

            if let Some(output) = &op.output
                && !output.is_constant
                && let Some(param_index) = namer.integer_param_slot_for_varnode(output)
            {
                current_active.remove(&param_index);
            }

            step_stack_env(&mut env, block_idx, op_idx, op, &sp_keys, &fp_keys);
        }

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

    // 3. Confirm any stack-spilled parameter whose stack slot is read back (excluding
    //    dead epilogue restores like `pop {r3, pc}`) or whose stack address escapes.
    if !stack_spills.is_empty() {
        for (block_idx, in_env) in &block_in_envs {
            let Some(block) = block_map.get(block_idx) else {
                continue;
            };
            let mut env = in_env.clone();
            for (op_idx, op) in block.ops.iter().enumerate() {
                match op.opcode {
                    PcodeOpcode::Load => {
                        if let Some(ptr) = op.inputs.get(1) {
                            let load_addr = eval_varnode(&env, ptr, &sp_keys, &fp_keys);
                            let load_size =
                                op.output.as_ref().map_or(namer.pointer_size, |o| o.size);
                            for &spill in &stack_spills {
                                let idx = spill.param_index();
                                if confirmed_params.contains(&idx)
                                    || !spill.matches_load(load_addr, load_size)
                                {
                                    continue;
                                }
                                let dead_restore = op.output.as_ref().is_some_and(|out| {
                                    is_dead_epilogue_restore(
                                        block,
                                        op_idx,
                                        out,
                                        idx,
                                        &block_successors,
                                        namer,
                                    )
                                });
                                if !dead_restore {
                                    confirmed_params.insert(idx);
                                }
                            }
                        }
                    }
                    PcodeOpcode::Store => {
                        if let Some(val_vn) = op.inputs.last() {
                            let val_stack = eval_varnode(&env, val_vn, &sp_keys, &fp_keys);
                            if let Some(orig) = val_stack.origin() {
                                for &spill in &stack_spills {
                                    if spill.origin() == orig {
                                        confirmed_params.insert(spill.param_index());
                                    }
                                }
                            }
                        }
                    }
                    PcodeOpcode::Call | PcodeOpcode::CallInd => {
                        for (k, v) in &env {
                            if !arch_reg_keys.contains(k) {
                                continue;
                            }
                            if let Some(orig) = v.origin() {
                                for &spill in &stack_spills {
                                    if spill.origin() == orig {
                                        confirmed_params.insert(spill.param_index());
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
                step_stack_env(&mut env, *block_idx, op_idx, op, &sp_keys, &fp_keys);
            }
        }
    }

    confirmed_params.into_iter().max().map(|index| index + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallingConvention, NirRenderOptions, test_maps::sync_preview_cspec};
    use fission_pcode_ir::pcode::{PcodeBasicBlock, PcodeOp};
    use fission_pcode_ir::space::{REGISTER_SPACE_ID, UNIQUE_SPACE_ID};

    fn namer_for(abi: CallingConvention, format: &str, is_64bit: bool) -> RegisterNamer {
        let mut options = NirRenderOptions {
            calling_convention: abi,
            format: format.to_string(),
            is_64bit,
            pointer_size: if is_64bit { 8 } else { 4 },
            ..Default::default()
        };
        sync_preview_cspec(&mut options);
        RegisterNamer::from_options(&options)
    }

    fn reg(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: REGISTER_SPACE_ID,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn uniq(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn cst(val: i64, size: u32) -> Varnode {
        Varnode::constant(val, size)
    }

    fn op(
        seq_num: u32,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
    ) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode,
            address: 0x1000 + u64::from(seq_num) * 4,
            output,
            inputs,
            asm_mnemonic: None,
        }
    }

    #[test]
    fn setter_store_through_pointer_param_preserves_both_params() {
        let namer = namer_for(CallingConvention::SystemVAmd64, "ELF64", true);
        let rdi = reg(0x38, 8);
        let esi = reg(0x30, 4);
        let func = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![],
                ops: vec![
                    op(0, PcodeOpcode::Store, None, vec![cst(0, 4), rdi, esi]),
                    op(1, PcodeOpcode::Return, None, vec![cst(0, 8)]),
                ],
            }],
        };
        assert_eq!(infer_entry_register_param_arity(&func, &namer), Some(2));
    }

    #[test]
    fn stack_spilled_parameters_read_back_in_o0_prologue_are_preserved() {
        let namer = namer_for(CallingConvention::SystemVAmd64, "ELF64", true);
        let rbp = reg(0x28, 8);
        let rdi = reg(0x38, 8);
        let esi = reg(0x30, 4);
        let eax = reg(0x00, 4);
        let t1 = uniq(0x100, 8);
        let t2 = uniq(0x108, 8);
        let t3 = uniq(0x110, 4);
        let func = PcodeFunction {
            blocks: vec![
                PcodeBasicBlock {
                    index: 0,
                    start_address: 0x1000,
                    successors: vec![1],
                    ops: vec![
                        op(
                            0,
                            PcodeOpcode::IntAdd,
                            Some(t1.clone()),
                            vec![rbp.clone(), cst(-8, 8)],
                        ),
                        op(
                            1,
                            PcodeOpcode::Store,
                            None,
                            vec![cst(0, 4), t1.clone(), rdi],
                        ),
                        op(
                            2,
                            PcodeOpcode::IntAdd,
                            Some(t2.clone()),
                            vec![rbp.clone(), cst(-12, 8)],
                        ),
                        op(3, PcodeOpcode::Store, None, vec![cst(0, 4), t2, esi]),
                    ],
                },
                PcodeBasicBlock {
                    index: 1,
                    start_address: 0x1020,
                    successors: vec![],
                    ops: vec![
                        op(
                            4,
                            PcodeOpcode::IntAdd,
                            Some(t1.clone()),
                            vec![rbp, cst(-12, 8)],
                        ),
                        op(5, PcodeOpcode::Load, Some(t3.clone()), vec![cst(0, 4), t1]),
                        op(6, PcodeOpcode::Copy, Some(eax.clone()), vec![t3]),
                        op(7, PcodeOpcode::Return, None, vec![eax]),
                    ],
                },
            ],
        };
        assert_eq!(infer_entry_register_param_arity(&func, &namer), Some(2));
    }

    #[test]
    fn arm32_dead_alignment_spill_with_later_zeroing_or_reuse_does_not_inflate_arity() {
        let namer = namer_for(CallingConvention::Arm32, "ELF", false);
        let sp = reg(0x54, 4);
        let r0 = reg(0x20, 4);
        let r3 = reg(0x2c, 4);
        let lr = reg(0x58, 4);
        let t_lr = uniq(0x100, 4);

        // `push {r3, lr}; movs r0, #1; bl helper; eor r3, r3; add r0, r0, r3; pop {r3, pc}`
        let func = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![],
                ops: vec![
                    op(
                        0,
                        PcodeOpcode::IntSub,
                        Some(sp.clone()),
                        vec![sp.clone(), cst(8, 4)],
                    ),
                    op(
                        1,
                        PcodeOpcode::Store,
                        None,
                        vec![cst(0, 4), sp.clone(), r3.clone()],
                    ),
                    op(
                        2,
                        PcodeOpcode::IntAdd,
                        Some(t_lr.clone()),
                        vec![sp.clone(), cst(4, 4)],
                    ),
                    op(3, PcodeOpcode::Store, None, vec![cst(0, 4), t_lr, lr]),
                    op(4, PcodeOpcode::Call, None, vec![cst(0x2000, 4)]),
                    op(
                        5,
                        PcodeOpcode::IntXor,
                        Some(r3.clone()),
                        vec![r3.clone(), r3.clone()],
                    ),
                    op(
                        6,
                        PcodeOpcode::IntAdd,
                        Some(r0.clone()),
                        vec![r0.clone(), r3.clone()],
                    ),
                    op(7, PcodeOpcode::Load, Some(r3), vec![cst(0, 4), sp.clone()]),
                    op(
                        8,
                        PcodeOpcode::IntAdd,
                        Some(sp.clone()),
                        vec![sp, cst(8, 4)],
                    ),
                    op(9, PcodeOpcode::Return, None, vec![r0]),
                ],
            }],
        };
        assert_eq!(infer_entry_register_param_arity(&func, &namer), None);
    }

    #[test]
    fn zero_in_left_operand_of_int_and_or_mult_does_not_count_param() {
        let namer = namer_for(CallingConvention::WindowsX64, "PE", true);
        let r9d = reg(0x88, 4);
        let r8d = reg(0x80, 4);
        let func = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![],
                ops: vec![
                    op(
                        0,
                        PcodeOpcode::IntAnd,
                        Some(r9d.clone()),
                        vec![cst(0, 4), r9d],
                    ),
                    op(
                        1,
                        PcodeOpcode::IntMult,
                        Some(r8d.clone()),
                        vec![cst(0, 4), r8d],
                    ),
                    op(2, PcodeOpcode::Return, None, vec![cst(0, 8)]),
                ],
            }],
        };
        assert_eq!(infer_entry_register_param_arity(&func, &namer), None);
    }

    #[test]
    fn unique_space_x86_param_varnodes_and_ch_dh_are_recognized() {
        let namer = namer_for(CallingConvention::WindowsX64, "PE", true);
        // Legacy UNIQUE_SPACE_ID alias for RDX (`X86_REG_BASE + 2 * 8`) -> param 2 (index 1).
        let unique_rdx = uniq(fission_pcode_ir::arch::x86::X86_REG_BASE + 2 * 8, 8);
        let rax = reg(0x00, 8);
        let func = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x1000,
                successors: vec![],
                ops: vec![
                    op(0, PcodeOpcode::Copy, Some(rax.clone()), vec![unique_rdx]),
                    op(1, PcodeOpcode::Return, None, vec![rax]),
                ],
            }],
        };
        assert_eq!(infer_entry_register_param_arity(&func, &namer), Some(2));
    }
}
