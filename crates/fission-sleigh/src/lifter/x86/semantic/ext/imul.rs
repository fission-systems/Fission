use super::*;

pub(super) fn decode_imul_r_rm(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, size, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let dst = x86_reg(decoded.reg_index, size);
    let lhs = dst.clone();
    let rhs = materialize_rm_value(&decoded.rm, size, address, &mut ops, temp, seq);
    emit_signed_imul_with_cf_of(lhs, rhs, dst, size, address, &mut ops, temp, seq, "IMUL");

    ops
}

pub(super) fn decode_imul_r_rm_imm(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    is_imm8: bool,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx, prefix, size, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let imm = match decode_immediate(
        insn,
        decoded.next_idx,
        if is_imm8 { 1 } else { immediate_bytes_for_operand(size) },
        size,
        is_imm8 || size == 8,
    ) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let lhs = materialize_rm_value(&decoded.rm, size, address, &mut ops, temp, seq);
    let dst = x86_reg(decoded.reg_index, size);
    emit_signed_imul_with_cf_of(lhs, imm, dst, size, address, &mut ops, temp, seq, "IMUL_IMM");

    ops
}

fn emit_signed_imul_with_cf_of(
    lhs: Varnode,
    rhs: Varnode,
    dst: Varnode,
    size: u32,
    address: u64,
    ops: &mut Vec<PcodeOp>,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    tag: &str,
) {
    let full_size = size.saturating_mul(2);
    let lhs_ext = temp.alloc(full_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSExt,
        address,
        output: Some(lhs_ext.clone()),
        inputs: vec![lhs],
        asm_mnemonic: Some(format!("{tag}_LHS_SEXT")),
    });
    let rhs_ext = temp.alloc(full_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSExt,
        address,
        output: Some(rhs_ext.clone()),
        inputs: vec![rhs],
        asm_mnemonic: Some(format!("{tag}_RHS_SEXT")),
    });
    let full = temp.alloc(full_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntMult,
        address,
        output: Some(full.clone()),
        inputs: vec![lhs_ext, rhs_ext],
        asm_mnemonic: Some(tag.to_string()),
    });
    let low = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::SubPiece,
        address,
        output: Some(low.clone()),
        inputs: vec![full.clone(), const_u64(0, 4)],
        asm_mnemonic: Some(format!("{tag}_LOW")),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(dst),
        inputs: vec![low.clone()],
        asm_mnemonic: Some(format!("{tag}_WRITE")),
    });

    let low_sext = temp.alloc(full_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSExt,
        address,
        output: Some(low_sext.clone()),
        inputs: vec![low],
        asm_mnemonic: Some(format!("{tag}_LOW_SEXT")),
    });
    let overflow = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntNotEqual,
        address,
        output: Some(overflow.clone()),
        inputs: vec![full, low_sext],
        asm_mnemonic: Some(format!("{tag}_CF_OF")),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_flag_cf()),
        inputs: vec![overflow.clone()],
        asm_mnemonic: Some(format!("{tag}_CF_WRITE")),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_flag_of()),
        inputs: vec![overflow],
        asm_mnemonic: Some(format!("{tag}_OF_WRITE")),
    });
}
