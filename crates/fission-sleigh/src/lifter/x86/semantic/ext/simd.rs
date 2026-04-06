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
        (SimdMandatoryPrefix::P66, 0x28) => {
            decode_two_byte_xmm_mov_load(insn, op_idx, prefix, address, temp, seq, "MOVAPD")
        }
        (SimdMandatoryPrefix::P66, 0x29) => {
            decode_two_byte_xmm_mov_store(insn, op_idx, prefix, address, temp, seq, "MOVAPD")
        }
        (SimdMandatoryPrefix::P66, 0x6F) => {
            decode_two_byte_xmm_mov_load(insn, op_idx, prefix, address, temp, seq, "MOVDQA")
        }
        (SimdMandatoryPrefix::P66, 0x7F) => {
            decode_two_byte_xmm_mov_store(insn, op_idx, prefix, address, temp, seq, "MOVDQA")
        }
        (SimdMandatoryPrefix::P66, 0x6E) => {
            decode_two_byte_movd_transfer_load(insn, op_idx, prefix, address, temp, seq)
        }
        (SimdMandatoryPrefix::P66, 0x7E) => {
            decode_two_byte_movd_transfer_store(insn, op_idx, prefix, address, temp, seq)
        }
        (SimdMandatoryPrefix::P66, 0x6C) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "PUNPCKLQDQ")
        }
        (SimdMandatoryPrefix::P66, 0x6D) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "PUNPCKHQDQ")
        }
        (SimdMandatoryPrefix::P66, 0x70) => {
            decode_two_byte_xmm_binop_imm8(insn, op_idx, prefix, address, temp, seq, "PSHUFD")
        }
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
        (SimdMandatoryPrefix::P66, 0x54) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "ANDPD")
        }
        (SimdMandatoryPrefix::P66, 0x55) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "ANDNPD")
        }
        (SimdMandatoryPrefix::P66, 0x56) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "ORPD")
        }
        (SimdMandatoryPrefix::P66, 0x57) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "XORPD")
        }
        (SimdMandatoryPrefix::P66, 0x74) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "PCMPEQB")
        }
        (SimdMandatoryPrefix::P66, 0x75) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "PCMPEQW")
        }
        (SimdMandatoryPrefix::P66, 0x76) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "PCMPEQD")
        }
        (SimdMandatoryPrefix::P66, 0xDB) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "PAND")
        }
        (SimdMandatoryPrefix::P66, 0xDF) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "PANDN")
        }
        (SimdMandatoryPrefix::P66, 0xEB) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "POR")
        }
        (SimdMandatoryPrefix::P66, 0xD4) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "PADDQ")
        }
        (SimdMandatoryPrefix::P66, 0xD5) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "PMULLW")
        }
        (SimdMandatoryPrefix::P66, 0xF8) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "PSUBB")
        }
        (SimdMandatoryPrefix::P66, 0xF9) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "PSUBW")
        }
        (SimdMandatoryPrefix::P66, 0xFA) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "PSUBD")
        }
        (SimdMandatoryPrefix::P66, 0xFB) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "PSUBQ")
        }
        (SimdMandatoryPrefix::P66, 0xFC) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "PADDB")
        }
        (SimdMandatoryPrefix::P66, 0xFD) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "PADDW")
        }
        (SimdMandatoryPrefix::P66, 0xFE) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "PADDD")
        }
        (SimdMandatoryPrefix::F2, 0x5E) => {
            decode_two_byte_scalar_binop(insn, op_idx, prefix, address, temp, seq, 8, "DIVSD")
        }
        (SimdMandatoryPrefix::F3, 0x5E) => {
            decode_two_byte_scalar_binop(insn, op_idx, prefix, address, temp, seq, 4, "DIVSS")
        }
        (SimdMandatoryPrefix::F2, 0x51) => {
            decode_two_byte_scalar_binop(insn, op_idx, prefix, address, temp, seq, 8, "SQRTSD")
        }
        (SimdMandatoryPrefix::F3, 0x51) => {
            decode_two_byte_scalar_binop(insn, op_idx, prefix, address, temp, seq, 4, "SQRTSS")
        }
        (SimdMandatoryPrefix::F2, 0x5A) => {
            decode_two_byte_scalar_binop(insn, op_idx, prefix, address, temp, seq, 8, "CVTSD2SS")
        }
        (SimdMandatoryPrefix::F3, 0x5A) => {
            decode_two_byte_scalar_binop(insn, op_idx, prefix, address, temp, seq, 4, "CVTSS2SD")
        }
        (SimdMandatoryPrefix::F2, 0x5D) => {
            decode_two_byte_scalar_binop(insn, op_idx, prefix, address, temp, seq, 8, "MINSD")
        }
        (SimdMandatoryPrefix::F3, 0x5D) => {
            decode_two_byte_scalar_binop(insn, op_idx, prefix, address, temp, seq, 4, "MINSS")
        }
        (SimdMandatoryPrefix::F2, 0x5F) => {
            decode_two_byte_scalar_binop(insn, op_idx, prefix, address, temp, seq, 8, "MAXSD")
        }
        (SimdMandatoryPrefix::F3, 0x5F) => {
            decode_two_byte_scalar_binop(insn, op_idx, prefix, address, temp, seq, 4, "MAXSS")
        }
        (SimdMandatoryPrefix::P66, 0x2E) => {
            decode_two_byte_scalar_cmp(insn, op_idx, prefix, address, temp, seq, 8, "UCOMISD")
        }
        (SimdMandatoryPrefix::None, 0x2E) => {
            decode_two_byte_scalar_cmp(insn, op_idx, prefix, address, temp, seq, 4, "UCOMISS")
        }
        (SimdMandatoryPrefix::P66, 0x2F) => {
            decode_two_byte_scalar_cmp(insn, op_idx, prefix, address, temp, seq, 8, "COMISD")
        }
        (SimdMandatoryPrefix::None, 0x2F) => {
            decode_two_byte_scalar_cmp(insn, op_idx, prefix, address, temp, seq, 4, "COMISS")
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
        (SimdMandatoryPrefix::P66, 0xEF) => {
            decode_two_byte_xmm_binop(insn, op_idx, prefix, address, temp, seq, "PXOR")
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

pub(super) fn decode_two_byte_xmm_mov_load(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    tag: &str,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, 16, address, temp, &mut ops, seq) {
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
        materialize_rm_value(&decoded.rm, 16, address, &mut ops, temp, seq)
    };
    let dst = x86_xmm_reg(decoded.reg_index, 16);
    let ext = insn[op_idx + 1];
    let policy_id = simd_intrinsic_policy_id(classify_simd_prefix(prefix), ext);
    let out = temp.alloc(16);

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

pub(super) fn decode_two_byte_xmm_mov_store(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    tag: &str,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, 16, address, temp, &mut ops, seq) {
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
        let out = temp.alloc(16);
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

pub(super) fn decode_two_byte_xmm_binop(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    tag: &str,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, 16, address, temp, &mut ops, seq) {
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
        materialize_rm_value(&decoded.rm, 16, address, &mut ops, temp, seq)
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

pub(super) fn decode_two_byte_xmm_binop_imm8(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    tag: &str,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, 16, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let imm8 = match decode_immediate(insn, decoded.next_idx, 1, 1, false) {
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
        materialize_rm_value(&decoded.rm, 16, address, &mut ops, temp, seq)
    };

    let ext = insn[op_idx + 1];
    let policy_id = simd_intrinsic_policy_id(classify_simd_prefix(prefix), ext);
    let out = temp.alloc(16);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: Some(out.clone()),
        inputs: vec![const_u64(policy_id, 8), dst.clone(), rhs, imm8],
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

fn decode_two_byte_movd_transfer_load(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let scalar_size = if (prefix.rex & 0x08) != 0 { 8 } else { 4 };
    let tag = if scalar_size == 8 { "MOVQ" } else { "MOVD" };

    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, scalar_size, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let src = materialize_rm_value(&decoded.rm, scalar_size, address, &mut ops, temp, seq);
    let dst = x86_xmm_reg(decoded.reg_index, 16);
    let ext = insn[op_idx + 1];
    let policy_id = simd_intrinsic_policy_id(classify_simd_prefix(prefix), ext);
    let out = temp.alloc(16);

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

fn decode_two_byte_movd_transfer_store(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let scalar_size = if (prefix.rex & 0x08) != 0 { 8 } else { 4 };
    let tag = if scalar_size == 8 { "MOVQ" } else { "MOVD" };

    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, scalar_size, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let src = x86_xmm_reg(decoded.reg_index, 16);
    let ext = insn[op_idx + 1];
    let policy_id = simd_intrinsic_policy_id(classify_simd_prefix(prefix), ext);
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
