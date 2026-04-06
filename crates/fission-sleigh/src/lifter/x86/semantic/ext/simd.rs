use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SimdMandatoryPrefix {
    None,
    P66,
    F2,
    F3,
}

fn classify_simd_prefix(prefix: &PrefixState) -> SimdMandatoryPrefix {
    match prefix.rep_prefix {
        Some(RepPrefix::Repne) => SimdMandatoryPrefix::F2,
        Some(RepPrefix::Rep) => SimdMandatoryPrefix::F3,
        None => {
            if prefix.operand_size_override {
                SimdMandatoryPrefix::P66
            } else {
                SimdMandatoryPrefix::None
            }
        }
    }
}

fn simd_intrinsic_policy_id(prefix: SimdMandatoryPrefix, ext: u8) -> u64 {
    let prefix_tag = match prefix {
        SimdMandatoryPrefix::None => 0u64,
        SimdMandatoryPrefix::P66 => 1u64,
        SimdMandatoryPrefix::F2 => 2u64,
        SimdMandatoryPrefix::F3 => 3u64,
    };
    X86_SIMD_SCALAR_INTRINSIC_BASE_ID + (prefix_tag << 8) + u64::from(ext)
}

pub(super) fn decode_simd_semantic(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    ext: u8,
) -> Vec<PcodeOp> {
    let mandatory = classify_simd_prefix(prefix);

    match (mandatory, ext) {
        (SimdMandatoryPrefix::F2, 0x10) => {
            decode_two_byte_scalar_mov_load(insn, op_idx, prefix, address, temp, seq, 8, "MOVSD")
        }
        (SimdMandatoryPrefix::F3, 0x10) => {
            decode_two_byte_scalar_mov_load(insn, op_idx, prefix, address, temp, seq, 4, "MOVSS")
        }
        (SimdMandatoryPrefix::F2, 0x11) => {
            decode_two_byte_scalar_mov_store(insn, op_idx, prefix, address, temp, seq, 8, "MOVSD")
        }
        (SimdMandatoryPrefix::F3, 0x11) => {
            decode_two_byte_scalar_mov_store(insn, op_idx, prefix, address, temp, seq, 4, "MOVSS")
        }
        (SimdMandatoryPrefix::F2, 0x58) => {
            decode_two_byte_scalar_binop(insn, op_idx, prefix, address, temp, seq, 8, "ADDSD")
        }
        (SimdMandatoryPrefix::F3, 0x58) => {
            decode_two_byte_scalar_binop(insn, op_idx, prefix, address, temp, seq, 4, "ADDSS")
        }
        (SimdMandatoryPrefix::F2, 0x59) => {
            decode_two_byte_scalar_binop(insn, op_idx, prefix, address, temp, seq, 8, "MULSD")
        }
        (SimdMandatoryPrefix::F3, 0x59) => {
            decode_two_byte_scalar_binop(insn, op_idx, prefix, address, temp, seq, 4, "MULSS")
        }
        (SimdMandatoryPrefix::F2, 0x5C) => {
            decode_two_byte_scalar_binop(insn, op_idx, prefix, address, temp, seq, 8, "SUBSD")
        }
        (SimdMandatoryPrefix::F3, 0x5C) => {
            decode_two_byte_scalar_binop(insn, op_idx, prefix, address, temp, seq, 4, "SUBSS")
        }
        (SimdMandatoryPrefix::F2, 0x2E) => {
            decode_two_byte_scalar_cmp(insn, op_idx, prefix, address, temp, seq, 8, "UCOMISD")
        }
        (SimdMandatoryPrefix::F3, 0x2E) => {
            decode_two_byte_scalar_cmp(insn, op_idx, prefix, address, temp, seq, 4, "UCOMISS")
        }
        (SimdMandatoryPrefix::F2, 0x2A) => {
            decode_two_byte_cvtsi_to_scalar(insn, op_idx, prefix, size, address, temp, seq, 8, "CVTSI2SD")
        }
        (SimdMandatoryPrefix::F3, 0x2A) => {
            decode_two_byte_cvtsi_to_scalar(insn, op_idx, prefix, size, address, temp, seq, 4, "CVTSI2SS")
        }
        (SimdMandatoryPrefix::F2, 0x2C) => {
            decode_two_byte_cvtt_scalar_to_si(insn, op_idx, prefix, size, address, temp, seq, 8, "CVTTSD2SI")
        }
        (SimdMandatoryPrefix::F3, 0x2C) => {
            decode_two_byte_cvtt_scalar_to_si(insn, op_idx, prefix, size, address, temp, seq, 4, "CVTTSS2SI")
        }
        (SimdMandatoryPrefix::F2, 0x2D) => {
            decode_two_byte_cvtt_scalar_to_si(insn, op_idx, prefix, size, address, temp, seq, 8, "CVTSD2SI")
        }
        (SimdMandatoryPrefix::F3, 0x2D) => {
            decode_two_byte_cvtt_scalar_to_si(insn, op_idx, prefix, size, address, temp, seq, 4, "CVTSS2SI")
        }
        _ => decode_simd_policy(address, seq, ext),
    }
}

pub(super) fn decode_simd_policy(address: u64, seq: &mut u32, ext: u8) -> Vec<PcodeOp> {
    vec![PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: None,
        inputs: vec![const_u64(X86_SIMD_POLICY_BASE_ID + u64::from(ext), 8)],
        asm_mnemonic: Some("SIMD_POLICY".to_string()),
    }]
}

pub(super) fn decode_two_byte_scalar_mov_load(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    scalar_size: u32,
    tag: &str,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, scalar_size, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let modrm = match insn.get(op_idx + 2) {
        Some(v) => *v,
        None => return Vec::new(),
    };
    let mode = (modrm >> 6) & 0x3;
    let rm_index = u32::from(modrm & 0x7) + rex_b(prefix);

    let src = if mode == 0x3 {
        x86_xmm_reg(rm_index, 16)
    } else {
        materialize_rm_value(&decoded.rm, scalar_size, address, &mut ops, temp, seq)
    };
    let dst = x86_xmm_reg(decoded.reg_index, 16);
    let out = temp.alloc(16);
    let ext = insn[op_idx + 1];
    let policy_id = simd_intrinsic_policy_id(classify_simd_prefix(prefix), ext);

    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: Some(out.clone()),
        inputs: vec![const_u64(policy_id, 8), src],
        asm_mnemonic: Some(format!("{tag}_INTRINSIC")),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(dst),
        inputs: vec![out],
        asm_mnemonic: Some(format!("{tag}_WRITE")),
    });
    ops
}

pub(super) fn decode_two_byte_scalar_mov_store(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    scalar_size: u32,
    tag: &str,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, scalar_size, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let modrm = match insn.get(op_idx + 2) {
        Some(v) => *v,
        None => return Vec::new(),
    };
    let mode = (modrm >> 6) & 0x3;
    let rm_index = u32::from(modrm & 0x7) + rex_b(prefix);

    let src = x86_xmm_reg(decoded.reg_index, 16);
    let ext = insn[op_idx + 1];
    let policy_id = simd_intrinsic_policy_id(classify_simd_prefix(prefix), ext);

    if mode == 0x3 {
        let out = temp.alloc(16);
        let dst = x86_xmm_reg(rm_index, 16);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: Some(out.clone()),
            inputs: vec![const_u64(policy_id, 8), src],
            asm_mnemonic: Some(format!("{tag}_INTRINSIC")),
        });
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::Copy,
            address,
            output: Some(dst),
            inputs: vec![out],
            asm_mnemonic: Some(format!("{tag}_WRITE")),
        });
        ops
    } else {
        let out = temp.alloc(scalar_size);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: Some(out.clone()),
            inputs: vec![const_u64(policy_id, 8), src],
            asm_mnemonic: Some(format!("{tag}_INTRINSIC")),
        });
        write_rm_value(&decoded.rm, out, address, &mut ops, seq, tag)
    }
}

pub(super) fn decode_two_byte_scalar_binop(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    scalar_size: u32,
    tag: &str,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, scalar_size, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let modrm = match insn.get(op_idx + 2) {
        Some(v) => *v,
        None => return Vec::new(),
    };
    let mode = (modrm >> 6) & 0x3;
    let rm_index = u32::from(modrm & 0x7) + rex_b(prefix);

    let dst = x86_xmm_reg(decoded.reg_index, 16);
    let rhs = if mode == 0x3 {
        x86_xmm_reg(rm_index, 16)
    } else {
        materialize_rm_value(&decoded.rm, scalar_size, address, &mut ops, temp, seq)
    };

    let ext = insn[op_idx + 1];
    let policy_id = simd_intrinsic_policy_id(classify_simd_prefix(prefix), ext);
    let out = temp.alloc(16);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: Some(out.clone()),
        inputs: vec![const_u64(policy_id, 8), dst.clone(), rhs],
        asm_mnemonic: Some(format!("{tag}_INTRINSIC")),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(dst),
        inputs: vec![out],
        asm_mnemonic: Some(format!("{tag}_WRITE")),
    });
    ops
}

pub(super) fn decode_two_byte_scalar_cmp(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    scalar_size: u32,
    tag: &str,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, scalar_size, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let modrm = match insn.get(op_idx + 2) {
        Some(v) => *v,
        None => return Vec::new(),
    };
    let mode = (modrm >> 6) & 0x3;
    let rm_index = u32::from(modrm & 0x7) + rex_b(prefix);

    let lhs = x86_xmm_reg(decoded.reg_index, 16);
    let rhs = if mode == 0x3 {
        x86_xmm_reg(rm_index, 16)
    } else {
        materialize_rm_value(&decoded.rm, scalar_size, address, &mut ops, temp, seq)
    };

    let ext = insn[op_idx + 1];
    let policy_id = simd_intrinsic_policy_id(classify_simd_prefix(prefix), ext);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: None,
        inputs: vec![const_u64(policy_id, 8), lhs, rhs],
        asm_mnemonic: Some(format!("{tag}_INTRINSIC")),
    });
    ops
}

pub(super) fn decode_two_byte_cvtsi_to_scalar(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    _scalar_size: u32,
    tag: &str,
) -> Vec<PcodeOp> {
    let int_size = if size == 8 { 8 } else { 4 };
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, int_size, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let src = materialize_rm_value(&decoded.rm, int_size, address, &mut ops, temp, seq);
    let dst = x86_xmm_reg(decoded.reg_index, 16);

    let ext = insn[op_idx + 1];
    let policy_id = simd_intrinsic_policy_id(classify_simd_prefix(prefix), ext);
    let out = temp.alloc(16);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: Some(out.clone()),
        inputs: vec![const_u64(policy_id, 8), dst.clone(), src],
        asm_mnemonic: Some(format!("{tag}_INTRINSIC")),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(dst),
        inputs: vec![out],
        asm_mnemonic: Some(format!("{tag}_WRITE")),
    });
    ops
}

pub(super) fn decode_two_byte_cvtt_scalar_to_si(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    scalar_size: u32,
    tag: &str,
) -> Vec<PcodeOp> {
    let out_size = if size == 8 { 8 } else { 4 };
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, scalar_size, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let modrm = match insn.get(op_idx + 2) {
        Some(v) => *v,
        None => return Vec::new(),
    };
    let mode = (modrm >> 6) & 0x3;
    let rm_index = u32::from(modrm & 0x7) + rex_b(prefix);

    let src = if mode == 0x3 {
        x86_xmm_reg(rm_index, 16)
    } else {
        materialize_rm_value(&decoded.rm, scalar_size, address, &mut ops, temp, seq)
    };
    let dst = x86_reg(decoded.reg_index, out_size);

    let ext = insn[op_idx + 1];
    let policy_id = simd_intrinsic_policy_id(classify_simd_prefix(prefix), ext);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: Some(dst),
        inputs: vec![const_u64(policy_id, 8), src],
        asm_mnemonic: Some(format!("{tag}_INTRINSIC")),
    });
    ops
}
