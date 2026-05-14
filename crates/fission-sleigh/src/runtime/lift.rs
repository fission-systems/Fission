use super::*;
use std::collections::{BTreeMap, HashMap, VecDeque};

fn template_source_evidence_key(source: crate::compiler::CompiledTemplateSource) -> &'static str {
    match source {
        crate::compiler::CompiledTemplateSource::SpecDerived => "sla_construct_tpl",
    }
}

fn internal_byte_offset(entry_address: u64, bytes_len: usize, address: u64) -> Option<usize> {
    let rel = address.checked_sub(entry_address)?;
    let offset = usize::try_from(rel).ok()?;
    (offset < bytes_len).then_some(offset)
}

fn direct_pcode_branch_target(op: &PcodeOp) -> Option<u64> {
    match op.opcode {
        PcodeOpcode::Branch | PcodeOpcode::CBranch => {
            let target = op.inputs.first()?;
            if target.is_constant {
                if target.offset != 0 {
                    Some(target.offset)
                } else if target.constant_val >= 0 {
                    Some(target.constant_val as u64)
                } else {
                    None
                }
            } else if target.offset != 0 {
                Some(target.offset)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn relative_pcode_target_seq(op: &PcodeOp, vn: &Varnode) -> Option<u32> {
    if vn.space_id != 0 || !vn.is_constant {
        return None;
    }
    let raw = if vn.offset != 0 {
        vn.offset as u32
    } else {
        vn.constant_val as u32
    };
    let delta = i32::from_le_bytes(raw.to_le_bytes());
    if delta == 0 {
        return None;
    }
    if delta > 0 {
        op.seq_num.checked_add(delta as u32)
    } else {
        op.seq_num.checked_sub(delta.unsigned_abs())
    }
}

fn instruction_cbranch_exits_to_fallthrough(ops: &[PcodeOp], fallthrough: u64) -> bool {
    let Some(last) = ops.last() else {
        return false;
    };
    if !matches!(last.opcode, PcodeOpcode::Return | PcodeOpcode::BranchInd) {
        return false;
    }
    ops.iter()
        .take(ops.len().saturating_sub(1))
        .filter(|op| op.opcode == PcodeOpcode::CBranch)
        .any(|op| {
            if direct_pcode_branch_target(op) == Some(fallthrough) {
                return true;
            }
            let Some(target) = op.inputs.first() else {
                return false;
            };
            let Some(target_seq) = relative_pcode_target_seq(op, target) else {
                return false;
            };
            !ops.iter()
                .any(|candidate| candidate.address == op.address && candidate.seq_num == target_seq)
        })
}

fn enqueue_internal_target(
    queue: &mut VecDeque<u64>,
    entry_address: u64,
    bytes_len: usize,
    target: u64,
) {
    if internal_byte_offset(entry_address, bytes_len, target).is_some() && !queue.contains(&target)
    {
        queue.push_back(target);
    }
}

fn const_value(vn: &Varnode) -> Option<u64> {
    if !vn.is_constant {
        return None;
    }
    if vn.offset != 0 {
        return Some(vn.offset);
    }
    (vn.constant_val >= 0).then_some(vn.constant_val as u64)
}

fn clears_only_low_pointer_bit(vn: &Varnode) -> bool {
    let width_bits = vn.size.saturating_mul(8).min(64);
    let width_mask = if width_bits == 64 {
        u64::MAX
    } else {
        (1u64 << width_bits) - 1
    };
    const_value(vn).is_some_and(|value| (value & width_mask) | 1 == width_mask)
        || (vn.is_constant && vn.constant_val == -2)
}

fn collect_defs<'a>(
    decoded: &'a BTreeMap<u64, Vec<PcodeOp>>,
    current: &'a [PcodeOp],
) -> HashMap<Varnode, &'a PcodeOp> {
    let mut defs = HashMap::new();
    for op in decoded
        .values()
        .flat_map(|ops| ops.iter())
        .chain(current.iter())
    {
        if let Some(output) = &op.output {
            defs.insert(output.clone(), op);
        }
    }
    defs
}

fn eval_const_expr(vn: &Varnode, defs: &HashMap<Varnode, &PcodeOp>, depth: usize) -> Option<u64> {
    if depth > 12 {
        return None;
    }
    if let Some(value) = const_value(vn) {
        return Some(value);
    }
    let op = defs.get(vn)?;
    match op.opcode {
        PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
            eval_const_expr(op.inputs.first()?, defs, depth + 1)
        }
        PcodeOpcode::IntAdd if op.inputs.len() == 2 => eval_const_expr(
            &op.inputs[0],
            defs,
            depth + 1,
        )?
        .checked_add(eval_const_expr(&op.inputs[1], defs, depth + 1)?),
        PcodeOpcode::IntSub if op.inputs.len() == 2 => eval_const_expr(
            &op.inputs[0],
            defs,
            depth + 1,
        )?
        .checked_sub(eval_const_expr(&op.inputs[1], defs, depth + 1)?),
        PcodeOpcode::IntLeft if op.inputs.len() == 2 => {
            let value = eval_const_expr(&op.inputs[0], defs, depth + 1)?;
            let shift = u32::try_from(eval_const_expr(&op.inputs[1], defs, depth + 1)?).ok()?;
            value.checked_shl(shift)
        }
        PcodeOpcode::IntMult if op.inputs.len() == 2 => eval_const_expr(
            &op.inputs[0],
            defs,
            depth + 1,
        )?
        .checked_mul(eval_const_expr(&op.inputs[1], defs, depth + 1)?),
        PcodeOpcode::IntAnd if op.inputs.len() == 2 => {
            eval_const_expr(&op.inputs[0], defs, depth + 1)
                .zip(eval_const_expr(&op.inputs[1], defs, depth + 1))
                .map(|(lhs, rhs)| lhs & rhs)
        }
        _ => None,
    }
}

fn additive_const_component(
    vn: &Varnode,
    defs: &HashMap<Varnode, &PcodeOp>,
    depth: usize,
) -> Option<u64> {
    if depth > 12 {
        return None;
    }
    if let Some(value) = eval_const_expr(vn, defs, depth + 1) {
        return Some(value);
    }
    let op = defs.get(vn)?;
    match op.opcode {
        PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
            additive_const_component(op.inputs.first()?, defs, depth + 1)
        }
        PcodeOpcode::IntAdd if op.inputs.len() == 2 => {
            let lhs = additive_const_component(&op.inputs[0], defs, depth + 1);
            let rhs = additive_const_component(&op.inputs[1], defs, depth + 1);
            match (lhs, rhs) {
                (Some(lhs), Some(rhs)) => lhs.checked_add(rhs),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            }
        }
        PcodeOpcode::IntSub if op.inputs.len() == 2 => {
            let lhs = additive_const_component(&op.inputs[0], defs, depth + 1);
            let rhs = eval_const_expr(&op.inputs[1], defs, depth + 1);
            match (lhs, rhs) {
                (Some(lhs), Some(rhs)) => lhs.checked_sub(rhs),
                (Some(value), None) => Some(value),
                _ => None,
            }
        }
        _ => None,
    }
}

fn branchind_load_table_base(
    vn: &Varnode,
    defs: &HashMap<Varnode, &PcodeOp>,
    depth: usize,
) -> Option<(u64, usize)> {
    if depth > 12 {
        return None;
    }
    let op = defs.get(vn)?;
    match op.opcode {
        PcodeOpcode::Copy | PcodeOpcode::Cast | PcodeOpcode::IntZExt | PcodeOpcode::IntSExt => {
            branchind_load_table_base(op.inputs.first()?, defs, depth + 1)
        }
        PcodeOpcode::IntAnd if op.inputs.len() == 2 => {
            if clears_only_low_pointer_bit(&op.inputs[0]) {
                return branchind_load_table_base(&op.inputs[1], defs, depth + 1);
            }
            if clears_only_low_pointer_bit(&op.inputs[1]) {
                return branchind_load_table_base(&op.inputs[0], defs, depth + 1);
            }
            None
        }
        PcodeOpcode::Load if op.inputs.len() == 2 => {
            let table_base = additive_const_component(&op.inputs[1], defs, depth + 1)?;
            let width = op.output.as_ref().map_or(vn.size, |out| out.size);
            Some((table_base, width.clamp(4, 8) as usize))
        }
        _ => None,
    }
}

fn read_unsigned_entry(bytes: &[u8], little_endian: bool) -> Option<u64> {
    match bytes.len() {
        4 => {
            let raw = [bytes[0], bytes[1], bytes[2], bytes[3]];
            Some(if little_endian {
                u32::from_le_bytes(raw) as u64
            } else {
                u32::from_be_bytes(raw) as u64
            })
        }
        8 => {
            let raw = [
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ];
            Some(if little_endian {
                u64::from_le_bytes(raw)
            } else {
                u64::from_be_bytes(raw)
            })
        }
        _ => None,
    }
}

fn read_signed_entry(bytes: &[u8], little_endian: bool) -> Option<i128> {
    match bytes.len() {
        4 => {
            let raw = [bytes[0], bytes[1], bytes[2], bytes[3]];
            Some(i128::from(if little_endian {
                i32::from_le_bytes(raw)
            } else {
                i32::from_be_bytes(raw)
            }))
        }
        8 => {
            let raw = [
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ];
            Some(i128::from(if little_endian {
                i64::from_le_bytes(raw)
            } else {
                i64::from_be_bytes(raw)
            }))
        }
        _ => None,
    }
}

fn add_signed_base(base: u64, displacement: i128) -> Option<u64> {
    let target = i128::from(base) + displacement;
    (0..=i128::from(u64::MAX))
        .contains(&target)
        .then_some(target as u64)
}

fn infer_branchind_jump_table_targets(
    branch_target: &Varnode,
    decoded: &BTreeMap<u64, Vec<PcodeOp>>,
    current_ops: &[PcodeOp],
    entry_address: u64,
    bytes: &[u8],
    memory_context: &DecodeMemoryContext,
    little_endian: bool,
) -> Vec<u64> {
    const MAX_JUMP_TABLE_CASES: u64 = 256;

    let defs = collect_defs(decoded, current_ops);
    let Some((table_base, entry_width)) = branchind_load_table_base(branch_target, &defs, 0) else {
        return Vec::new();
    };
    if entry_width != 4 && entry_width != 8 {
        return Vec::new();
    }
    if internal_byte_offset(entry_address, bytes.len(), table_base).is_none() {
        return Vec::new();
    }

    let mut mode_targets = Vec::<Vec<u64>>::new();
    let mut mode_bases = vec![None, Some(table_base)];
    for base in &memory_context.relative_address_bases {
        if !mode_bases.contains(&Some(*base)) {
            mode_bases.push(Some(*base));
        }
    }

    for base in mode_bases {
        let mut targets = Vec::new();
        for ordinal in 0..MAX_JUMP_TABLE_CASES {
            let Some(entry_addr) =
                table_base.checked_add(ordinal.saturating_mul(entry_width as u64))
            else {
                break;
            };
            let Some(offset) = internal_byte_offset(entry_address, bytes.len(), entry_addr) else {
                break;
            };
            let end = offset.saturating_add(entry_width);
            if end > bytes.len() {
                break;
            }
            let raw = &bytes[offset..end];
            let target = if let Some(base) = base {
                read_signed_entry(raw, little_endian).and_then(|disp| add_signed_base(base, disp))
            } else {
                read_unsigned_entry(raw, little_endian)
            };
            let Some(target) = target else {
                break;
            };
            if internal_byte_offset(entry_address, bytes.len(), target).is_none() {
                break;
            }
            if (table_base..entry_addr + entry_width as u64).contains(&target) {
                break;
            }
            if !targets.contains(&target) {
                targets.push(target);
            }
        }
        if targets.len() >= 2 {
            mode_targets.push(targets);
        }
    }

    mode_targets
        .into_iter()
        .max_by_key(|targets| targets.len())
        .unwrap_or_default()
}

fn attach_inferred_indirect_edges(
    function: &mut PcodeFunction,
    inferred_edges: &BTreeMap<u64, Vec<u64>>,
) {
    if inferred_edges.is_empty() {
        return;
    }

    let block_start_to_index = function
        .blocks
        .iter()
        .map(|block| (block.start_address, block.index))
        .collect::<BTreeMap<_, _>>();
    let source_to_block_index = function
        .blocks
        .iter()
        .filter(|block| {
            block
                .ops
                .last()
                .is_some_and(|op| op.opcode == PcodeOpcode::BranchInd)
        })
        .flat_map(|block| block.ops.iter().map(move |op| (op.address, block.index)))
        .collect::<BTreeMap<_, _>>();

    for (source, targets) in inferred_edges {
        let Some(source_idx) = source_to_block_index.get(source).copied() else {
            continue;
        };
        let Some(block) = function
            .blocks
            .iter_mut()
            .find(|block| block.index == source_idx)
        else {
            continue;
        };
        for target in targets {
            let Some(target_idx) = block_start_to_index.get(target).copied() else {
                continue;
            };
            if !block.successors.contains(&target_idx) {
                block.successors.push(target_idx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn var(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: 1,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn op(
        seq_num: u32,
        address: u64,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
    ) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode,
            address,
            output,
            inputs,
            asm_mnemonic: None,
        }
    }

    #[test]
    fn conditional_terminal_instruction_preserves_instruction_fallthrough() {
        let ops = vec![
            op(
                0,
                0x1000,
                PcodeOpcode::CBranch,
                None,
                vec![Varnode::constant(2, 8), var(0x20, 1)],
            ),
            op(1, 0x1000, PcodeOpcode::Return, None, vec![var(0x10, 4)]),
        ];

        assert!(instruction_cbranch_exits_to_fallthrough(&ops, 0x1004));
    }

    #[test]
    fn conditional_terminal_instruction_keeps_internal_branch_local() {
        let ops = vec![
            op(
                0,
                0x1000,
                PcodeOpcode::CBranch,
                None,
                vec![Varnode::constant(1, 8), var(0x20, 1)],
            ),
            op(1, 0x1000, PcodeOpcode::Return, None, vec![var(0x10, 4)]),
        ];

        assert!(!instruction_cbranch_exits_to_fallthrough(&ops, 0x1004));
    }

    #[test]
    fn aarch64_madd_lift_preserves_addend_dataflow() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("AARCH64").expect("AARCH64 frontend");
        let bytes = [0x00, 0x20, 0x0a, 0x1b];
        let (ops, len) = frontend
            .decode_and_lift_with_len(&bytes, 0x100034)
            .expect("decode madd");

        assert_eq!(len, 4);
        assert!(
            ops.iter().any(|op| op.opcode == PcodeOpcode::IntMult),
            "MADD must multiply the first two operands"
        );
        assert!(
            ops.iter().any(|op| op.opcode == PcodeOpcode::IntAdd),
            "MADD must add the accumulator operand"
        );
    }

    #[test]
    fn aarch64_udiv_madd_function_lift_preserves_accumulator_path() {
        let frontend =
            RuntimeSleighFrontend::new_for_language("AARCH64").expect("AARCH64 frontend");
        let bytes = [
            0xa8, 0x99, 0x99, 0x52, 0x49, 0x01, 0x80, 0x52, 0x2a, 0xa7, 0x80, 0x52, 0x88, 0x99,
            0xb9, 0x72, 0x08, 0x7c, 0xa8, 0x9b, 0x08, 0xfd, 0x63, 0xd3, 0x08, 0x81, 0x09, 0x1b,
            0xe9, 0xdd, 0x97, 0x52, 0xa9, 0xd5, 0xbb, 0x72, 0x09, 0x00, 0x09, 0x4a, 0x08, 0x05,
            0x00, 0x11, 0x28, 0x09, 0xc8, 0x1a, 0x09, 0x6c, 0x89, 0x13, 0x08, 0x20, 0x0a, 0x1b,
            0xea, 0x1d, 0x80, 0x52, 0xaa, 0x15, 0xa0, 0x72, 0x0a, 0x00, 0x0a, 0x4a, 0x29, 0x01,
            0x0a, 0x0b, 0x08, 0x01, 0x00, 0x4a, 0x00, 0x7d, 0x09, 0x1b, 0x08, 0x00, 0x00, 0x90,
            0x00, 0x01, 0x00, 0xb9, 0xc0, 0x03, 0x5f, 0xd6,
        ];
        let function = frontend
            .lift_raw_pcode_function(&bytes, 0x100000)
            .expect("lift function");

        assert!(function.blocks.iter().any(|block| {
            block
                .ops
                .iter()
                .any(|op| op.address == 0x100034 && op.opcode == PcodeOpcode::IntAdd)
        }));
    }

    #[test]
    fn template_source_evidence_key_names_sla_construct_tpl() {
        assert_eq!(
            template_source_evidence_key(crate::compiler::CompiledTemplateSource::SpecDerived),
            "sla_construct_tpl"
        );
    }
}

impl RuntimeSleighFrontend {
    pub fn lift_raw_pcode_function(
        &self,
        bytes: &[u8],
        entry_address: u64,
    ) -> Result<PcodeFunction> {
        Ok(self
            .lift_raw_pcode_function_with_contract(
                bytes,
                entry_address,
                DEFAULT_FUNCTION_INSTRUCTION_LIMIT,
            )?
            .function)
    }

    pub fn lift_raw_pcode_function_with_contract(
        &self,
        bytes: &[u8],
        entry_address: u64,
        instruction_limit: usize,
    ) -> Result<DecodedPcodeFunction> {
        self.lift_raw_pcode_function_with_decode_contract(
            bytes,
            entry_address,
            DecodeContract::strict_function(instruction_limit),
        )
    }

    pub fn lift_raw_pcode_function_with_decode_contract(
        &self,
        bytes: &[u8],
        entry_address: u64,
        contract: DecodeContract,
    ) -> Result<DecodedPcodeFunction> {
        self.lift_raw_pcode_function_with_decode_contract_and_memory_context(
            bytes,
            entry_address,
            contract,
            &DecodeMemoryContext::default(),
        )
    }

    pub fn lift_raw_pcode_function_with_decode_contract_and_memory_context(
        &self,
        bytes: &[u8],
        entry_address: u64,
        contract: DecodeContract,
        memory_context: &DecodeMemoryContext,
    ) -> Result<DecodedPcodeFunction> {
        self.lift_raw_pcode_function_with_context_and_memory_context(
            bytes,
            entry_address,
            contract,
            memory_context,
            None,
        )
    }

    pub fn lift_raw_pcode_function_with_context_and_memory_context(
        &self,
        bytes: &[u8],
        entry_address: u64,
        contract: DecodeContract,
        memory_context: &DecodeMemoryContext,
        initial_context_override: Option<PackedContextOverride>,
    ) -> Result<DecodedPcodeFunction> {
        if bytes.is_empty() {
            bail!("No function bytes available at 0x{:x}", entry_address);
        }
        if contract.instruction_limit == 0 {
            bail!("instruction_limit must be > 0");
        }

        let mut decoded = BTreeMap::<u64, Vec<PcodeOp>>::new();
        let mut inferred_indirect_edges = BTreeMap::<u64, Vec<u64>>::new();
        let mut template_source_counts = BTreeMap::<String, usize>::new();
        let base_context_override = initial_context_override;
        let mut context_overrides = BTreeMap::<u64, PackedContextOverride>::new();
        if let Some(override_bits) = initial_context_override {
            context_overrides.insert(entry_address, override_bits);
        }
        let mut queue = VecDeque::from([entry_address]);
        let mut stop_reason = DecodeStopReason::InputExhausted;

        while let Some(current) = queue.pop_front() {
            if decoded.contains_key(&current) {
                continue;
            }
            if decoded.len() >= contract.instruction_limit {
                stop_reason = DecodeStopReason::InstructionLimit;
                break;
            }
            let Some(offset) = internal_byte_offset(entry_address, bytes.len(), current) else {
                continue;
            };
            let remaining = &bytes[offset..];
            let context_override = match (
                base_context_override,
                context_overrides.get(&current).copied(),
            ) {
                (Some(base), Some(pending)) => Some(base.merge_override(pending)),
                (Some(base), None) => Some(base),
                (None, Some(pending)) => Some(pending),
                (None, None) => None,
            };
            let (mut ins_ops, decoded_len, details) = self
                .decode_and_lift_with_context_override(remaining, current, context_override)
                .map_err(|err| anyhow!("decode failed at 0x{:x}: {:#}", current, err))?;
            if let Some(source) = details.template_source {
                *template_source_counts
                    .entry(template_source_evidence_key(source).to_string())
                    .or_insert(0) += 1;
            }

            if decoded_len == 0 {
                bail!("decoder returned zero length at 0x{:x}", current);
            }
            let step = usize::try_from(decoded_len)?;
            if step > remaining.len() {
                bail!(
                    "decoded length {} exceeds available bytes {} at 0x{:x}",
                    step,
                    remaining.len(),
                    current
                );
            }

            let terminal = ins_ops
                .last()
                .map(|op| contract.is_terminal_control_flow(op.opcode))
                .unwrap_or(false);
            let last_opcode = ins_ops.last().map(|op| op.opcode);
            let direct_target = ins_ops.last().and_then(direct_pcode_branch_target);
            let fallthrough = checked_instruction_fallthrough(current, decoded_len)?;

            let cbranch_exits_to_fallthrough =
                instruction_cbranch_exits_to_fallthrough(&ins_ops, fallthrough);
            let little_endian = !matches!(
                registry::runtime_variant_for_entry(&self.entry)
                    .ok()
                    .map(|variant| variant.endian),
                Some(RuntimeEndian::Big)
            );

            for (target_addr, word_index, mask, value) in &details.pending_context_commits {
                let entry = context_overrides.entry(*target_addr).or_default();
                entry.merge_commit_word(*word_index, *mask, *value)?;
            }

            match last_opcode {
                Some(PcodeOpcode::Branch) => {
                    if let Some(target) = direct_target {
                        enqueue_internal_target(&mut queue, entry_address, bytes.len(), target);
                    }
                }
                Some(PcodeOpcode::CBranch) => {
                    if let Some(target) = direct_target {
                        enqueue_internal_target(&mut queue, entry_address, bytes.len(), target);
                    }
                    enqueue_internal_target(&mut queue, entry_address, bytes.len(), fallthrough);
                }
                Some(PcodeOpcode::Return) => {
                    if cbranch_exits_to_fallthrough {
                        enqueue_internal_target(
                            &mut queue,
                            entry_address,
                            bytes.len(),
                            fallthrough,
                        );
                    } else {
                        stop_reason = DecodeStopReason::TerminalControlFlow;
                    }
                }
                Some(PcodeOpcode::BranchInd) => {
                    if contract.stop_at_indirect_branch {
                        if cbranch_exits_to_fallthrough {
                            enqueue_internal_target(
                                &mut queue,
                                entry_address,
                                bytes.len(),
                                fallthrough,
                            );
                        } else {
                            stop_reason = DecodeStopReason::TerminalControlFlow;
                        }
                    } else if let Some(branch_target) =
                        ins_ops.last().and_then(|op| op.inputs.first())
                    {
                        let inferred_targets = infer_branchind_jump_table_targets(
                            branch_target,
                            &decoded,
                            &ins_ops,
                            entry_address,
                            bytes,
                            memory_context,
                            little_endian,
                        );
                        if !inferred_targets.is_empty() {
                            inferred_indirect_edges.insert(current, inferred_targets.clone());
                        }
                        for target in inferred_targets {
                            enqueue_internal_target(&mut queue, entry_address, bytes.len(), target);
                        }
                    }
                }
                _ if !terminal => {
                    enqueue_internal_target(&mut queue, entry_address, bytes.len(), fallthrough);
                }
                _ => {}
            }

            decoded.insert(current, std::mem::take(&mut ins_ops));
        }

        let instruction_count = decoded.len();
        let mut ops = Vec::new();
        let mut global_seq = 0u32;
        for mut ins_ops in decoded.into_values() {
            for op in &mut ins_ops {
                op.seq_num = global_seq;
                global_seq = global_seq
                    .checked_add(1)
                    .ok_or_else(|| anyhow!("p-code seq_num overflowed"))?;
            }
            ops.extend(ins_ops);
        }

        if ops.is_empty() {
            bail!("failed to decode any instruction at 0x{:x}", entry_address);
        }

        let mut function = PcodeFunction {
            blocks: build_cfg_blocks(entry_address, ops),
        };
        attach_inferred_indirect_edges(&mut function, &inferred_indirect_edges);
        function
            .validate()
            .map_err(|err| RuntimeSleighError::InvalidPcodeShape {
                language: self.entry.entry_id.clone(),
                reason: err.to_string(),
            })?;

        Ok(DecodedPcodeFunction {
            function,
            decoded_instructions: instruction_count,
            stop_reason,
            template_source_counts,
        })
    }
}
