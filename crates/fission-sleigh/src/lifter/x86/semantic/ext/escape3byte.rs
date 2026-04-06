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
) -> Vec<PcodeOp> {
    let ext3 = match insn.get(op_idx + 2) {
        Some(v) => *v,
        None => return Vec::new(),
    };
    let mandatory = classify_escape_prefix(prefix);

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
            0x16 => return decode_pextrd_pinsrd_family(insn, op_idx, prefix, size, address, temp, seq, true),
            0x17 => return decode_extractps_semantic(insn, op_idx, prefix, size, address, temp, seq),
            0x22 => {
                return decode_pextrd_pinsrd_family(insn, op_idx, prefix, size, address, temp, seq, false)
            }
            0x61 => return decode_pcmpstrx_semantic(insn, op_idx, prefix, address, temp, seq, 0x61),
            0x62 => return decode_pcmpstrx_semantic(insn, op_idx, prefix, address, temp, seq, 0x62),
            0x63 => return decode_pcmpistri_semantic(insn, op_idx, prefix, address, temp, seq),
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
    let decoded = match decode_modrm_operand(insn, op_idx + 2, prefix, src_size, address, temp, &mut ops, seq) {
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
    let decoded = match decode_modrm_operand(insn, op_idx + 2, prefix, elem_size, address, temp, &mut ops, seq) {
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
    let decoded = match decode_modrm_operand(insn, op_idx + 2, prefix, 16, address, temp, &mut ops, seq) {
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
            RmOperand::Mem(_) => materialize_rm_value(&decoded.rm, 16, address, &mut ops, temp, seq),
            RmOperand::Reg(_) => return Vec::new(),
        }
    };

    let out = temp.alloc(4);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: Some(out.clone()),
        inputs: vec![const_u64(X86_3BYTE_0F3A_POLICY_BASE_ID + 0x63, 8), lhs, rhs, imm8],
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
    let decoded = match decode_modrm_operand(insn, op_idx + 2, prefix, 16, address, temp, &mut ops, seq) {
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
            RmOperand::Mem(_) => materialize_rm_value(&decoded.rm, 16, address, &mut ops, temp, seq),
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
        inputs: vec![const_u64(X86_3BYTE_0F3A_POLICY_BASE_ID + u64::from(ext3), 8), lhs, rhs, imm8],
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
    let decoded = match decode_modrm_operand(insn, op_idx + 2, prefix, rm_size, address, temp, &mut ops, seq) {
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
        inputs: vec![const_u64(X86_3BYTE_0F3A_POLICY_BASE_ID + 0x17, 8), src_xmm, imm8],
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
    let decoded = match decode_modrm_operand(insn, op_idx + 2, prefix, 16, address, temp, &mut ops, seq) {
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
            RmOperand::Mem(_) => materialize_rm_value(&decoded.rm, 16, address, &mut ops, temp, seq),
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
