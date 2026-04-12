use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EscapeMandatoryPrefix {
    None,
    P66,
    F2,
    F3,
}

fn classify_escape_prefix(prefix: &PrefixState) -> EscapeMandatoryPrefix {
    match prefix.rep_prefix {
        Some(RepPrefix::Repne) => EscapeMandatoryPrefix::F2,
        Some(RepPrefix::Rep) => EscapeMandatoryPrefix::F3,
        None => {
            if prefix.operand_size_override {
                EscapeMandatoryPrefix::P66
            } else {
                EscapeMandatoryPrefix::None
            }
        }
    }
}

pub(super) fn decode_three_byte_escape_semantic(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    map_0f3a: bool,
    vvvv_reg: u32,
) -> Vec<PcodeOp> {
    let ext3 = match insn.get(op_idx + 2) {
        Some(v) => *v,
        None => return Vec::new(),
    };
    let mandatory = classify_escape_prefix(prefix);

    // Phase D: BMI1/2 instructions (0F38 map only, require VEX prefix with VVVV)
    if !map_0f3a {
        match (mandatory, ext3) {
            // ANDN: dst = ~src1(vvvv) & src2(r/m)
            (EscapeMandatoryPrefix::None, 0xF2) => {
                return decode_bmi_andn(insn, op_idx, prefix, size, address, temp, seq, vvvv_reg);
            }
            // BLSR/BLSI/BLSMSK: dst(vvvv), src(r/m), reg_field selects variant
            (EscapeMandatoryPrefix::None, 0xF3) => {
                return decode_bmi_blsr_family(
                    insn, op_idx, prefix, size, address, temp, seq, vvvv_reg,
                );
            }
            // BEXTR: dst(ModRM.reg), src(r/m), ctrl(vvvv)
            (EscapeMandatoryPrefix::None, 0xF7) => {
                return decode_bmi_bextr(insn, op_idx, prefix, size, address, temp, seq, vvvv_reg);
            }
            // BZHI: dst(ModRM.reg), src(r/m), idx(vvvv)
            (EscapeMandatoryPrefix::None, 0xF5) => {
                return decode_bmi_bzhi(insn, op_idx, prefix, size, address, temp, seq, vvvv_reg);
            }
            // SARX: dst(ModRM.reg), src(r/m), cnt(vvvv) — no flags
            (EscapeMandatoryPrefix::F3, 0xF7) => {
                return decode_bmi2_shift(
                    insn,
                    op_idx,
                    prefix,
                    size,
                    address,
                    temp,
                    seq,
                    vvvv_reg,
                    PcodeOpcode::IntSRight,
                    "SARX",
                );
            }
            // SHLX: dst, src, cnt(vvvv) — no flags
            (EscapeMandatoryPrefix::P66, 0xF7) => {
                return decode_bmi2_shift(
                    insn,
                    op_idx,
                    prefix,
                    size,
                    address,
                    temp,
                    seq,
                    vvvv_reg,
                    PcodeOpcode::IntLeft,
                    "SHLX",
                );
            }
            // SHRX: dst, src, cnt(vvvv) — no flags
            (EscapeMandatoryPrefix::F2, 0xF7) => {
                return decode_bmi2_shift(
                    insn,
                    op_idx,
                    prefix,
                    size,
                    address,
                    temp,
                    seq,
                    vvvv_reg,
                    PcodeOpcode::IntRight,
                    "SHRX",
                );
            }
            // MULX: hi(ModRM.reg), lo(vvvv) = (EDX/RDX) * src(r/m) — no flags
            (EscapeMandatoryPrefix::F2, 0xF6) => {
                return decode_bmi2_mulx(insn, op_idx, prefix, size, address, temp, seq, vvvv_reg);
            }
            // PEXT: dst(ModRM.reg), src(vvvv), mask(r/m) → CallOther
            (EscapeMandatoryPrefix::F3, 0xF5) => {
                return decode_bmi2_pext_pdep(
                    insn,
                    op_idx,
                    prefix,
                    size,
                    address,
                    temp,
                    seq,
                    "PEXT",
                    0xF5_F3_u64,
                );
            }
            // PDEP: dst(ModRM.reg), src(vvvv), mask(r/m) → CallOther
            (EscapeMandatoryPrefix::F2, 0xF5) => {
                return decode_bmi2_pext_pdep(
                    insn,
                    op_idx,
                    prefix,
                    size,
                    address,
                    temp,
                    seq,
                    "PDEP",
                    0xF5_F2_u64,
                );
            }
            _ => {}
        }
    }

    if !map_0f3a {
        match (mandatory, ext3) {
            (EscapeMandatoryPrefix::F2, 0xF0) | (EscapeMandatoryPrefix::F2, 0xF1) => {
                return decode_crc32_semantic(insn, op_idx, prefix, size, address, temp, seq, ext3)
            }
            (EscapeMandatoryPrefix::P66, 0xDB) => {
                return decode_three_byte_xmm_intrinsic(
                    insn, op_idx, prefix, address, temp, seq, false, ext3, "AESIMC", false,
                )
            }
            _ => {}
        }
    }

    if map_0f3a {
        match ext3 {
            0x16 => {
                return decode_pextrd_pinsrd_family(
                    insn, op_idx, prefix, size, address, temp, seq, true,
                )
            }
            0x17 => {
                return decode_extractps_semantic(insn, op_idx, prefix, size, address, temp, seq)
            }
            0x22 => {
                return decode_pextrd_pinsrd_family(
                    insn, op_idx, prefix, size, address, temp, seq, false,
                )
            }
            0x61 => {
                return decode_pcmpstrx_semantic(insn, op_idx, prefix, address, temp, seq, 0x61)
            }
            0x62 => {
                return decode_pcmpstrx_semantic(insn, op_idx, prefix, address, temp, seq, 0x62)
            }
            0x63 => return decode_pcmpistri_semantic(insn, op_idx, prefix, address, temp, seq),
            // RORX: VEX.F2.0F3A.W0/1 F0 /r imm8 — rotate right without flags
            0xF0 if mandatory == EscapeMandatoryPrefix::F2 => {
                return decode_rorx(insn, op_idx, prefix, size, address, temp, seq);
            }
            _ => {}
        }
    }

    let selected = if map_0f3a {
        match ext3 {
            0x0C => Some(("BLENDPS", true)),
            0x0D => Some(("BLENDPD", true)),
            0x08 => Some(("ROUNDPS", true)),
            0x09 => Some(("ROUNDPD", true)),
            0x0A => Some(("ROUNDSS", true)),
            0x0B => Some(("ROUNDSD", true)),
            0x44 => Some(("PCLMULQDQ", true)),
            0xCC => Some(("SHA1RNDS4", true)),
            0x0F => Some(("PALIGNR", true)),
            0x0E => Some(("PBLENDW", true)),
            0xDF => Some(("AESKEYGENASSIST", true)),
            _ => None,
        }
    } else {
        match ext3 {
            0x00 => Some(("PSHUFB", false)),
            0xC8 => Some(("SHA1NEXTE", false)),
            0xC9 => Some(("SHA1MSG1", false)),
            0xCA => Some(("SHA1MSG2", false)),
            0xCB => Some(("SHA256RNDS2", false)),
            0xDC => Some(("AESENC", false)),
            0xCC => Some(("SHA256MSG1", false)),
            0xCD => Some(("SHA256MSG2", false)),
            0xDD => Some(("AESENCLAST", false)),
            0xDE => Some(("AESDEC", false)),
            0xDF => Some(("AESDECLAST", false)),
            _ => None,
        }
    };

    if let Some((tag, has_imm8)) = selected {
        return decode_three_byte_xmm_intrinsic(
            insn, op_idx, prefix, address, temp, seq, map_0f3a, ext3, tag, has_imm8,
        );
    }

    let base = if map_0f3a {
        X86_3BYTE_0F3A_POLICY_BASE_ID
    } else {
        X86_3BYTE_0F38_POLICY_BASE_ID
    };
    vec![PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: None,
        inputs: vec![const_u64(base + u64::from(ext3), 8)],
        asm_mnemonic: Some(if map_0f3a {
            "0F3A_POLICY".to_string()
        } else {
            "0F38_POLICY".to_string()
        }),
    }]
}

fn decode_crc32_semantic(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    ext3: u8,
) -> Vec<PcodeOp> {
    let dst_size = if (prefix.rex & 0x08) != 0 { 8 } else { 4 };
    let src_size = if ext3 == 0xF0 {
        1
    } else if prefix.operand_size_override {
        2
    } else if size == 8 {
        8
    } else {
        4
    };

    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(
        insn,
        op_idx + 2,
        prefix,
        src_size,
        address,
        temp,
        &mut ops,
        seq,
    ) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let dst = x86_reg(decoded.reg_index, dst_size);
    let src = materialize_rm_value(&decoded.rm, src_size, address, &mut ops, temp, seq);
    let out = temp.alloc(dst_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: Some(out.clone()),
        inputs: vec![
            const_u64(X86_3BYTE_0F38_POLICY_BASE_ID + u64::from(ext3), 8),
            dst.clone(),
            src,
        ],
        asm_mnemonic: Some("CRC32_INTRINSIC".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(dst),
        inputs: vec![out],
        asm_mnemonic: Some("CRC32_WRITE".to_string()),
    });
    ops
}

pub(super) fn decode_pextrd_pinsrd_family(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    is_extract: bool,
) -> Vec<PcodeOp> {
    let elem_size = if size == 8 { 8 } else { 4 };
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(
        insn,
        op_idx + 2,
        prefix,
        elem_size,
        address,
        temp,
        &mut ops,
        seq,
    ) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let imm8 = match decode_immediate(insn, decoded.next_idx, 1, 1, false) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let opcode_id = if is_extract { 0x16u64 } else { 0x22u64 };
    let policy_id = const_u64(X86_3BYTE_0F3A_POLICY_BASE_ID + opcode_id, 8);

    if is_extract {
        let src_xmm = x86_xmm_reg(decoded.reg_index, 16);
        let out = temp.alloc(elem_size);
        let tag = if elem_size == 8 { "PEXTRQ" } else { "PEXTRD" };
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: Some(out.clone()),
            inputs: vec![policy_id, src_xmm, imm8],
            asm_mnemonic: Some(format!("{tag}_INTRINSIC")),
        });
        write_rm_value(&decoded.rm, out, address, &mut ops, seq, tag)
    } else {
        let dst_xmm = x86_xmm_reg(decoded.reg_index, 16);
        let src = materialize_rm_value(&decoded.rm, elem_size, address, &mut ops, temp, seq);
        let out = temp.alloc(16);
        let tag = if elem_size == 8 { "PINSRQ" } else { "PINSRD" };
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: Some(out.clone()),
            inputs: vec![policy_id, dst_xmm.clone(), src, imm8],
            asm_mnemonic: Some(format!("{tag}_INTRINSIC")),
        });
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::Copy,
            address,
            output: Some(dst_xmm),
            inputs: vec![out],
            asm_mnemonic: Some(format!("{tag}_WRITE")),
        });
        ops
    }
}

pub(super) fn decode_pcmpistri_semantic(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, op_idx + 2, prefix, 16, address, temp, &mut ops, seq) {
            Some(v) => v,
            None => return Vec::new(),
        };
    let imm8 = match decode_immediate(insn, decoded.next_idx, 1, 1, false) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let modrm = match insn.get(op_idx + 3) {
        Some(v) => *v,
        None => return Vec::new(),
    };
    let mode = (modrm >> 6) & 0x3;
    let rm_index = u32::from(modrm & 0x7) + rex_b(prefix);

    let lhs = x86_xmm_reg(decoded.reg_index, 16);
    let rhs = if mode == 0x3 {
        x86_xmm_reg(rm_index, 16)
    } else {
        match &decoded.rm {
            RmOperand::Mem(_) => {
                materialize_rm_value(&decoded.rm, 16, address, &mut ops, temp, seq)
            }
            RmOperand::Reg(_) => return Vec::new(),
        }
    };

    let out = temp.alloc(4);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: Some(out.clone()),
        inputs: vec![
            const_u64(X86_3BYTE_0F3A_POLICY_BASE_ID + 0x63, 8),
            lhs,
            rhs,
            imm8,
        ],
        asm_mnemonic: Some("PCMPISTRI_INTRINSIC".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_reg(1, 4)),
        inputs: vec![out],
        asm_mnemonic: Some("PCMPISTRI_ECX_WRITE".to_string()),
    });

    ops
}

pub(super) fn decode_pcmpstrx_semantic(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    ext3: u8,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, op_idx + 2, prefix, 16, address, temp, &mut ops, seq) {
            Some(v) => v,
            None => return Vec::new(),
        };
    let imm8 = match decode_immediate(insn, decoded.next_idx, 1, 1, false) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let modrm = match insn.get(op_idx + 3) {
        Some(v) => *v,
        None => return Vec::new(),
    };
    let mode = (modrm >> 6) & 0x3;
    let rm_index = u32::from(modrm & 0x7) + rex_b(prefix);

    let lhs = x86_xmm_reg(decoded.reg_index, 16);
    let rhs = if mode == 0x3 {
        x86_xmm_reg(rm_index, 16)
    } else {
        match &decoded.rm {
            RmOperand::Mem(_) => {
                materialize_rm_value(&decoded.rm, 16, address, &mut ops, temp, seq)
            }
            RmOperand::Reg(_) => return Vec::new(),
        }
    };

    let (tag, out_size, out_target, write_mnemonic) = match ext3 {
        0x61 => (
            "PCMPESTRI",
            4,
            x86_reg(1, 4),
            "PCMPESTRI_ECX_WRITE".to_string(),
        ),
        0x62 => (
            "PCMPISTRM",
            16,
            x86_xmm_reg(0, 16),
            "PCMPISTRM_XMM0_WRITE".to_string(),
        ),
        _ => return Vec::new(),
    };

    let out = temp.alloc(out_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: Some(out.clone()),
        inputs: vec![
            const_u64(X86_3BYTE_0F3A_POLICY_BASE_ID + u64::from(ext3), 8),
            lhs,
            rhs,
            imm8,
        ],
        asm_mnemonic: Some(format!("{tag}_INTRINSIC")),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(out_target),
        inputs: vec![out],
        asm_mnemonic: Some(write_mnemonic),
    });

    ops
}

pub(super) fn decode_extractps_semantic(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let rm_size = if size == 8 { 8 } else { 4 };
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(
        insn,
        op_idx + 2,
        prefix,
        rm_size,
        address,
        temp,
        &mut ops,
        seq,
    ) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let imm8 = match decode_immediate(insn, decoded.next_idx, 1, 1, false) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let src_xmm = x86_xmm_reg(decoded.reg_index, 16);
    let out = temp.alloc(rm_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: Some(out.clone()),
        inputs: vec![
            const_u64(X86_3BYTE_0F3A_POLICY_BASE_ID + 0x17, 8),
            src_xmm,
            imm8,
        ],
        asm_mnemonic: Some("EXTRACTPS_INTRINSIC".to_string()),
    });
    write_rm_value(&decoded.rm, out, address, &mut ops, seq, "EXTRACTPS")
}

pub(super) fn decode_three_byte_xmm_intrinsic(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    map_0f3a: bool,
    ext3: u8,
    tag: &str,
    has_imm8: bool,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, op_idx + 2, prefix, 16, address, temp, &mut ops, seq) {
            Some(v) => v,
            None => return Vec::new(),
        };

    let modrm = match insn.get(op_idx + 3) {
        Some(v) => *v,
        None => return Vec::new(),
    };
    let mode = (modrm >> 6) & 0x3;
    let rm_index = u32::from(modrm & 0x7) + rex_b(prefix);

    let dst = x86_xmm_reg(decoded.reg_index, 16);
    let src = if mode == 0x3 {
        x86_xmm_reg(rm_index, 16)
    } else {
        match &decoded.rm {
            RmOperand::Mem(_) => {
                materialize_rm_value(&decoded.rm, 16, address, &mut ops, temp, seq)
            }
            RmOperand::Reg(_) => return Vec::new(),
        }
    };

    let base = if map_0f3a {
        X86_3BYTE_0F3A_POLICY_BASE_ID
    } else {
        X86_3BYTE_0F38_POLICY_BASE_ID
    };
    let mut inputs = vec![const_u64(base + u64::from(ext3), 8), dst.clone(), src];
    if has_imm8 {
        let imm8 = match decode_immediate(insn, decoded.next_idx, 1, 1, false) {
            Some(v) => v,
            None => return Vec::new(),
        };
        inputs.push(imm8);
    }

    let out = temp.alloc(16);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: Some(out.clone()),
        inputs,
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

// ── BMI flag helper ───────────────────────────────────────────────────────────
// All BMI1 logical instructions: CF=0, OF=0, ZF/SF from result, PF undefined.
fn emit_bmi_flags(ops: &mut Vec<PcodeOp>, address: u64, size: u32, result: Varnode, seq: &mut u32) {
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_flag_cf()),
        inputs: vec![const_u64(0, 1)],
        asm_mnemonic: Some("BMI_CF_ZERO".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_flag_of()),
        inputs: vec![const_u64(0, 1)],
        asm_mnemonic: Some("BMI_OF_ZERO".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntEqual,
        address,
        output: Some(x86_flag_zf()),
        inputs: vec![result.clone(), const_u64(0, size)],
        asm_mnemonic: Some("BMI_ZF".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSLess,
        address,
        output: Some(x86_flag_sf()),
        inputs: vec![result.clone(), const_u64(0, size)],
        asm_mnemonic: Some("BMI_SF".to_string()),
    });
}

/// ANDN dst(ModRM.reg), src1(vvvv), src2(r/m): dst = ~src1 & src2
fn decode_bmi_andn(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    vvvv_reg: u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, op_idx + 2, prefix, size, address, temp, &mut ops, seq) {
            Some(v) => v,
            None => return Vec::new(),
        };
    let dst = x86_reg(decoded.reg_index, size);
    let src1 = x86_reg(vvvv_reg, size);
    let src2 = materialize_rm_value(&decoded.rm, size, address, &mut ops, temp, seq);
    let not_src1 = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntNegate,
        address,
        output: Some(not_src1.clone()),
        inputs: vec![src1],
        asm_mnemonic: Some("ANDN_NOT".to_string()),
    });
    let result = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(result.clone()),
        inputs: vec![not_src1, src2],
        asm_mnemonic: Some("ANDN".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(dst),
        inputs: vec![result.clone()],
        asm_mnemonic: Some("ANDN_WRITE".to_string()),
    });
    emit_bmi_flags(&mut ops, address, size, result, seq);
    ops
}

/// BLSR/BLSI/BLSMSK: vvvv = dst, r/m = src; reg_field selects variant.
fn decode_bmi_blsr_family(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    vvvv_reg: u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, op_idx + 2, prefix, size, address, temp, &mut ops, seq) {
            Some(v) => v,
            None => return Vec::new(),
        };
    let reg_field = decoded.reg_field;
    let dst = x86_reg(vvvv_reg, size);
    let src = materialize_rm_value(&decoded.rm, size, address, &mut ops, temp, seq);

    let result = temp.alloc(size);
    match reg_field {
        1 => {
            // BLSR: dst = src & (src - 1)
            let src_m1 = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntSub,
                address,
                output: Some(src_m1.clone()),
                inputs: vec![src.clone(), const_u64(1, size)],
                asm_mnemonic: Some("BLSR_DEC".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntAnd,
                address,
                output: Some(result.clone()),
                inputs: vec![src, src_m1],
                asm_mnemonic: Some("BLSR".to_string()),
            });
        }
        2 => {
            // BLSI: dst = src & (-src)
            let neg_src = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Int2Comp,
                address,
                output: Some(neg_src.clone()),
                inputs: vec![src.clone()],
                asm_mnemonic: Some("BLSI_NEG".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntAnd,
                address,
                output: Some(result.clone()),
                inputs: vec![src, neg_src],
                asm_mnemonic: Some("BLSI".to_string()),
            });
        }
        3 => {
            // BLSMSK: dst = src ^ (src - 1)
            let src_m1 = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntSub,
                address,
                output: Some(src_m1.clone()),
                inputs: vec![src.clone(), const_u64(1, size)],
                asm_mnemonic: Some("BLSMSK_DEC".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntXor,
                address,
                output: Some(result.clone()),
                inputs: vec![src, src_m1],
                asm_mnemonic: Some("BLSMSK".to_string()),
            });
        }
        _ => return Vec::new(),
    }
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(dst),
        inputs: vec![result.clone()],
        asm_mnemonic: Some("BLSR_FAMILY_WRITE".to_string()),
    });
    emit_bmi_flags(&mut ops, address, size, result, seq);
    ops
}

/// BEXTR dst(ModRM.reg), src(r/m), ctrl(vvvv): extract bitfield → CallOther.
fn decode_bmi_bextr(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    vvvv_reg: u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, op_idx + 2, prefix, size, address, temp, &mut ops, seq) {
            Some(v) => v,
            None => return Vec::new(),
        };
    let dst = x86_reg(decoded.reg_index, size);
    let src = materialize_rm_value(&decoded.rm, size, address, &mut ops, temp, seq);
    let ctrl = x86_reg(vvvv_reg, size);
    let out = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: Some(out.clone()),
        inputs: vec![
            const_u64(X86_3BYTE_0F38_POLICY_BASE_ID + 0xF7, 8),
            src,
            ctrl,
        ],
        asm_mnemonic: Some("BEXTR_INTRINSIC".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(dst),
        inputs: vec![out],
        asm_mnemonic: Some("BEXTR_WRITE".to_string()),
    });
    ops
}

/// BZHI dst(ModRM.reg), src(r/m), idx(vvvv): zero bits above index.
fn decode_bmi_bzhi(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    vvvv_reg: u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, op_idx + 2, prefix, size, address, temp, &mut ops, seq) {
            Some(v) => v,
            None => return Vec::new(),
        };
    let dst = x86_reg(decoded.reg_index, size);
    let src = materialize_rm_value(&decoded.rm, size, address, &mut ops, temp, seq);
    let idx = x86_reg(vvvv_reg, size);

    // mask = (1 << idx) - 1; dst = src & mask
    let shifted = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntLeft,
        address,
        output: Some(shifted.clone()),
        inputs: vec![const_u64(1, size), idx],
        asm_mnemonic: Some("BZHI_SHIFT".to_string()),
    });
    let mask = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSub,
        address,
        output: Some(mask.clone()),
        inputs: vec![shifted, const_u64(1, size)],
        asm_mnemonic: Some("BZHI_MASK".to_string()),
    });
    let result = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(result.clone()),
        inputs: vec![src, mask],
        asm_mnemonic: Some("BZHI".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(dst),
        inputs: vec![result.clone()],
        asm_mnemonic: Some("BZHI_WRITE".to_string()),
    });
    emit_bmi_flags(&mut ops, address, size, result, seq);
    ops
}

/// SARX/SHLX/SHRX: shift without flags.
fn decode_bmi2_shift(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    vvvv_reg: u32,
    shift_op: PcodeOpcode,
    tag: &str,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, op_idx + 2, prefix, size, address, temp, &mut ops, seq) {
            Some(v) => v,
            None => return Vec::new(),
        };
    let dst = x86_reg(decoded.reg_index, size);
    let src = materialize_rm_value(&decoded.rm, size, address, &mut ops, temp, seq);
    let cnt_raw = x86_reg(vvvv_reg, size);
    // Mask count to width-1 bits
    let cnt_mask = (size * 8 - 1) as u64;
    let cnt = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(cnt.clone()),
        inputs: vec![cnt_raw, const_u64(cnt_mask, size)],
        asm_mnemonic: Some(format!("{tag}_MASK_CNT")),
    });
    let result = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: shift_op,
        address,
        output: Some(result.clone()),
        inputs: vec![src, cnt],
        asm_mnemonic: Some(tag.to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(dst),
        inputs: vec![result],
        asm_mnemonic: Some(format!("{tag}_WRITE")),
    });
    ops
}

/// MULX hi(ModRM.reg), lo(vvvv) = RDX * src(r/m), no flags.
fn decode_bmi2_mulx(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    vvvv_reg: u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, op_idx + 2, prefix, size, address, temp, &mut ops, seq) {
            Some(v) => v,
            None => return Vec::new(),
        };
    let hi_dst = x86_reg(decoded.reg_index, size);
    let lo_dst = x86_reg(vvvv_reg, size);
    // Implicit source: EDX (reg index 2) or RDX
    let rdx = x86_reg(2, size);
    let src = materialize_rm_value(&decoded.rm, size, address, &mut ops, temp, seq);
    // Low half
    let lo = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntMult,
        address,
        output: Some(lo.clone()),
        inputs: vec![rdx.clone(), src.clone()],
        asm_mnemonic: Some("MULX_LO".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(lo_dst),
        inputs: vec![lo],
        asm_mnemonic: Some("MULX_LO_WRITE".to_string()),
    });
    // High half via CallOther (no IntMultHigh in P-code)
    let hi = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: Some(hi.clone()),
        inputs: vec![const_u64(X86_3BYTE_0F38_POLICY_BASE_ID + 0xF6, 8), rdx, src],
        asm_mnemonic: Some("MULX_HI_INTRINSIC".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(hi_dst),
        inputs: vec![hi],
        asm_mnemonic: Some("MULX_HI_WRITE".to_string()),
    });
    ops
}

/// RORX dst, src(r/m), imm8: rotate right without flags.
/// dst = (src >> imm8) | (src << (width - imm8))
fn decode_rorx(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, op_idx + 2, prefix, size, address, temp, &mut ops, seq) {
            Some(v) => v,
            None => return Vec::new(),
        };
    let imm8_vn = match decode_immediate(insn, decoded.next_idx, 1, 1, false) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let dst = x86_reg(decoded.reg_index, size);
    let src = materialize_rm_value(&decoded.rm, size, address, &mut ops, temp, seq);
    let width_bits = size * 8;

    // imm8 is a constant shift amount
    let imm_val = if imm8_vn.is_constant {
        (imm8_vn.constant_val as u64) & u64::from(width_bits - 1)
    } else {
        // Shouldn't happen for RORX; fall back to CallOther
        let out = temp.alloc(size);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: Some(out.clone()),
            inputs: vec![
                const_u64(X86_3BYTE_0F3A_POLICY_BASE_ID + 0xF0, 8),
                src,
                imm8_vn,
            ],
            asm_mnemonic: Some("RORX_INTRINSIC".to_string()),
        });
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::Copy,
            address,
            output: Some(dst),
            inputs: vec![out],
            asm_mnemonic: Some("RORX_WRITE".to_string()),
        });
        return ops;
    };

    if imm_val == 0 {
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::Copy,
            address,
            output: Some(dst),
            inputs: vec![src],
            asm_mnemonic: Some("RORX_IDENTITY".to_string()),
        });
        return ops;
    }

    // right_part = src >> imm_val
    let right_part = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntRight,
        address,
        output: Some(right_part.clone()),
        inputs: vec![src.clone(), const_u64(imm_val, size)],
        asm_mnemonic: Some("RORX_SHR".to_string()),
    });
    // left_part = src << (width_bits - imm_val)
    let left_shift = u64::from(width_bits) - imm_val;
    let left_part = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntLeft,
        address,
        output: Some(left_part.clone()),
        inputs: vec![src, const_u64(left_shift, size)],
        asm_mnemonic: Some("RORX_SHL".to_string()),
    });
    // result = right_part | left_part
    let result = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntOr,
        address,
        output: Some(result.clone()),
        inputs: vec![right_part, left_part],
        asm_mnemonic: Some("RORX_OR".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(dst),
        inputs: vec![result],
        asm_mnemonic: Some("RORX_WRITE".to_string()),
    });
    ops
}

/// PEXT/PDEP → CallOther (bit-scatter/gather not expressible in P-code).
fn decode_bmi2_pext_pdep(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    tag: &str,
    policy_id: u64,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, op_idx + 2, prefix, size, address, temp, &mut ops, seq) {
            Some(v) => v,
            None => return Vec::new(),
        };
    let dst = x86_reg(decoded.reg_index, size);
    let mask = materialize_rm_value(&decoded.rm, size, address, &mut ops, temp, seq);
    let out = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: Some(out.clone()),
        inputs: vec![const_u64(policy_id, 8), mask],
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
