use super::*;
use super::super::predicate::emit_jcc_predicate_with_allocator;

const X86_RDTSC_POLICY_ID: u64 = 0x0F31;
const X86_CLFLUSH_POLICY_ID: u64 = 0x0FAE07;
const X86_SYSCALL_POLICY_ID: u64 = 0x0F05;
const X86_SYSRET_POLICY_ID: u64 = 0x0F07;
const X86_CLTS_POLICY_ID: u64 = 0x0F06;
const X86_INVD_POLICY_ID: u64 = 0x0F08;
const X86_WBINVD_POLICY_ID: u64 = 0x0F09;
const X86_UD2_POLICY_ID: u64 = 0x0F0B;
const X86_WRMSR_POLICY_ID: u64 = 0x0F30;
const X86_RDMSR_POLICY_ID: u64 = 0x0F32;
const X86_SYSENTER_POLICY_ID: u64 = 0x0F34;
const X86_SYSEXIT_POLICY_ID: u64 = 0x0F35;
const X86_EMMS_POLICY_ID: u64 = 0x0F77;
const X86_PUSH_FS_POLICY_ID: u64 = 0x0FA0;
const X86_POP_FS_POLICY_ID: u64 = 0x0FA1;
const X86_CPUID_POLICY_ID: u64 = 0x0FA2;
const X86_SIMD_POLICY_BASE_ID: u64 = 0x0F00_00;
const X86_X87_POLICY_BASE_ID: u64 = 0x0FD8_00;
const X86_3BYTE_0F38_POLICY_BASE_ID: u64 = 0x0F38_00;
const X86_3BYTE_0F3A_POLICY_BASE_ID: u64 = 0x0F3A_00;

pub(super) fn decode_extended_semantic(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let ext = match insn.get(op_idx + 1) {
        Some(v) => *v,
        None => return Vec::new(),
    };

    match ext {
        0x05 => decode_system_policy(address, seq, X86_SYSCALL_POLICY_ID, "SYSCALL_POLICY"),
        0x06 => decode_system_policy(address, seq, X86_CLTS_POLICY_ID, "CLTS_POLICY"),
        0x07 => decode_system_policy(address, seq, X86_SYSRET_POLICY_ID, "SYSRET_POLICY"),
        0x08 => decode_system_policy(address, seq, X86_INVD_POLICY_ID, "INVD_POLICY"),
        0x09 => decode_system_policy(address, seq, X86_WBINVD_POLICY_ID, "WBINVD_POLICY"),
        0x0B => decode_system_policy(address, seq, X86_UD2_POLICY_ID, "UD2_POLICY"),
        0x1F => decode_nop_extended(insn, op_idx, prefix, size, address, temp, seq),
        0x30 => decode_system_policy(address, seq, X86_WRMSR_POLICY_ID, "WRMSR_POLICY"),
        0x31 => decode_rdtsc_policy(address, seq),
        0x32 => decode_system_policy(address, seq, X86_RDMSR_POLICY_ID, "RDMSR_POLICY"),
        0x34 => decode_system_policy(address, seq, X86_SYSENTER_POLICY_ID, "SYSENTER_POLICY"),
        0x35 => decode_system_policy(address, seq, X86_SYSEXIT_POLICY_ID, "SYSEXIT_POLICY"),
        0x38 => decode_three_byte_escape_semantic(insn, op_idx, prefix, size, address, temp, seq, false),
        0x3A => decode_three_byte_escape_semantic(insn, op_idx, prefix, size, address, temp, seq, true),
        0x77 => decode_system_policy(address, seq, X86_EMMS_POLICY_ID, "EMMS_POLICY"),
        0xA2 => decode_system_policy(address, seq, X86_CPUID_POLICY_ID, "CPUID_POLICY"),
        0xA0 => decode_system_policy(address, seq, X86_PUSH_FS_POLICY_ID, "PUSH_FS_POLICY"),
        0xA1 => decode_system_policy(address, seq, X86_POP_FS_POLICY_ID, "POP_FS_POLICY"),
        0xAE => decode_clflush_policy(insn, op_idx, prefix, address, temp, seq),
        0xA3 => decode_bt_family(insn, op_idx, prefix, size, address, temp, seq, BitTestKind::Bt),
        0xAB => decode_bt_family(insn, op_idx, prefix, size, address, temp, seq, BitTestKind::Bts),
        0xB3 => decode_bt_family(insn, op_idx, prefix, size, address, temp, seq, BitTestKind::Btr),
        0xBB => decode_bt_family(insn, op_idx, prefix, size, address, temp, seq, BitTestKind::Btc),
        0xC8..=0xCF => decode_bswap(prefix, size, ext, address, temp, seq),
        0xB6 | 0xB7 | 0xBE | 0xBF => {
            let src_size = if matches!(ext, 0xB6 | 0xBE) { 1 } else { 2 };
            let is_sign_extend = matches!(ext, 0xBE | 0xBF);
            decode_movx(insn, op_idx, prefix, size, src_size, is_sign_extend, address, temp, seq)
        }
        0xAF => decode_imul_r_rm(insn, op_idx, prefix, size, address, temp, seq),
        0xBC => decode_bsf_bsr(insn, op_idx, prefix, size, address, temp, seq, false),
        0xBD => decode_bsf_bsr(insn, op_idx, prefix, size, address, temp, seq, true),
        0x40..=0x4F => decode_cmovcc(insn, op_idx, prefix, size, address, temp, seq, ext - 0x40),
        0x10..=0x17 | 0x28..=0x2F | 0x50..=0x76 | 0x78..=0x7F => {
            decode_simd_policy(address, seq, ext)
        }
        0x90..=0x9F => decode_setcc(insn, op_idx, prefix, address, temp, seq, ext - 0x90),
        0xD8..=0xDF => decode_x87_policy(address, seq, ext),
        _ => Vec::new(),
    }
}

fn decode_simd_policy(address: u64, seq: &mut u32, ext: u8) -> Vec<PcodeOp> {
    vec![PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: None,
        inputs: vec![const_u64(X86_SIMD_POLICY_BASE_ID + u64::from(ext), 8)],
        asm_mnemonic: Some("SIMD_POLICY".to_string()),
    }]
}

fn decode_x87_policy(address: u64, seq: &mut u32, ext: u8) -> Vec<PcodeOp> {
    vec![PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: None,
        inputs: vec![const_u64(X86_X87_POLICY_BASE_ID + u64::from(ext), 8)],
        asm_mnemonic: Some("X87_POLICY".to_string()),
    }]
}

fn decode_three_byte_escape_semantic(
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
            0x08 => Some(("ROUNDPS", true)),
            0x09 => Some(("ROUNDPD", true)),
            0x0A => Some(("ROUNDSS", true)),
            0x0B => Some(("ROUNDSD", true)),
            0x44 => Some(("PCLMULQDQ", true)),
            0xCC => Some(("SHA1RNDS4", true)),
            0x0F => Some(("PALIGNR", true)),
            0x0E => Some(("PBLENDW", true)),
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

fn decode_pextrd_pinsrd_family(
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

fn decode_pcmpistri_semantic(
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

fn decode_pcmpstrx_semantic(
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

fn decode_extractps_semantic(
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

fn decode_three_byte_xmm_intrinsic(
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

fn decode_system_policy(address: u64, seq: &mut u32, policy_id: u64, mnemonic: &str) -> Vec<PcodeOp> {
    vec![PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: None,
        inputs: vec![const_u64(policy_id, 8)],
        asm_mnemonic: Some(mnemonic.to_string()),
    }]
}

fn decode_rdtsc_policy(address: u64, seq: &mut u32) -> Vec<PcodeOp> {
    vec![PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: None,
        inputs: vec![const_u64(X86_RDTSC_POLICY_ID, 8)],
        asm_mnemonic: Some("RDTSC_POLICY".to_string()),
    }]
}

fn decode_clflush_policy(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, 1, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };

    if decoded.reg_field != 7 {
        return Vec::new();
    }

    let addr_vn = match decoded.rm {
        RmOperand::Mem(addr) => addr,
        RmOperand::Reg(_) => return Vec::new(),
    };

    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: None,
        inputs: vec![const_u64(X86_CLFLUSH_POLICY_ID, 8), addr_vn],
        asm_mnemonic: Some("CLFLUSH_POLICY".to_string()),
    });

    ops
}

fn decode_nop_extended(
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

    if decoded.reg_field != 0 {
        return Vec::new();
    }

    // Treat 0F 1F /0 as a semantic no-op hint; keep address-side decoding deterministic.
    if matches!(decoded.rm, RmOperand::Reg(_)) {
        return Vec::new();
    }

    let hint = temp.alloc(8);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(hint),
        inputs: vec![const_u64(0x0F1F, 8)],
        asm_mnemonic: Some("NOP_EXT_HINT".to_string()),
    });

    ops
}

fn decode_bswap(
    prefix: &PrefixState,
    size: u32,
    ext: u8,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let reg_size = if size == 8 { 8 } else { 4 };
    let reg_index = u32::from(ext.saturating_sub(0xC8)) + rex_b(prefix);
    let reg = x86_reg(reg_index, reg_size);

    let mut ops = Vec::new();
    let mut reversed: Option<Varnode> = None;
    for byte_idx in (0..reg_size).rev() {
        let byte = temp.alloc(1);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::SubPiece,
            address,
            output: Some(byte.clone()),
            inputs: vec![reg.clone(), const_u64(u64::from(byte_idx), 4)],
            asm_mnemonic: Some("BSWAP_EXTRACT".to_string()),
        });
        reversed = Some(match reversed {
            Some(low) => {
                let combined = temp.alloc(low.size.saturating_add(1));
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::Piece,
                    address,
                    output: Some(combined.clone()),
                    inputs: vec![byte, low],
                    asm_mnemonic: Some("BSWAP_PIECE".to_string()),
                });
                combined
            }
            None => byte,
        });
    }

    let result = match reversed {
        Some(v) => v,
        None => return Vec::new(),
    };
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(reg),
        inputs: vec![result],
        asm_mnemonic: Some("BSWAP_WRITE".to_string()),
    });
    ops
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BitTestKind {
    Bt,
    Bts,
    Btr,
    Btc,
}

#[derive(Debug, Clone)]
enum BitTestTarget {
    Reg(Varnode),
    Mem(Varnode),
}

fn bt_tag(kind: BitTestKind) -> &'static str {
    match kind {
        BitTestKind::Bt => "BT",
        BitTestKind::Bts => "BTS",
        BitTestKind::Btr => "BTR",
        BitTestKind::Btc => "BTC",
    }
}

fn bt_word_shift(size: u32) -> u64 {
    let bits = u64::from(size.saturating_mul(8).max(1));
    u64::from(bits.trailing_zeros())
}

fn decode_bt_family(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    kind: BitTestKind,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, size, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let tag = bt_tag(kind);
    let bit_index = x86_reg(decoded.reg_index, size);
    let bits_per_word = u64::from(size.saturating_mul(8));
    let local_index = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(local_index.clone()),
        inputs: vec![bit_index.clone(), const_u64(bits_per_word.saturating_sub(1), size)],
        asm_mnemonic: Some(format!("{tag}_BIT_INDEX")),
    });

    let (base_value, target) = match decoded.rm {
        RmOperand::Reg(dst) => (dst.clone(), BitTestTarget::Reg(dst)),
        RmOperand::Mem(base_addr) => {
            let word_index = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntSRight,
                address,
                output: Some(word_index.clone()),
                inputs: vec![bit_index, const_u64(bt_word_shift(size), size)],
                asm_mnemonic: Some(format!("{tag}_MEM_WORD_INDEX")),
            });

            let byte_delta = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntMult,
                address,
                output: Some(byte_delta.clone()),
                inputs: vec![word_index, const_u64(u64::from(size), size)],
                asm_mnemonic: Some(format!("{tag}_MEM_BYTE_DELTA")),
            });

            let addr_delta = if byte_delta.size < base_addr.size {
                let extended = temp.alloc(base_addr.size);
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::IntSExt,
                    address,
                    output: Some(extended.clone()),
                    inputs: vec![byte_delta],
                    asm_mnemonic: Some(format!("{tag}_MEM_ADDR_DELTA_EXT")),
                });
                extended
            } else if byte_delta.size > base_addr.size {
                let truncated = temp.alloc(base_addr.size);
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::SubPiece,
                    address,
                    output: Some(truncated.clone()),
                    inputs: vec![byte_delta, const_u64(0, 4)],
                    asm_mnemonic: Some(format!("{tag}_MEM_ADDR_DELTA_TRUNC")),
                });
                truncated
            } else {
                byte_delta
            };

            let effective_addr = temp.alloc(base_addr.size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntAdd,
                address,
                output: Some(effective_addr.clone()),
                inputs: vec![base_addr, addr_delta],
                asm_mnemonic: Some(format!("{tag}_MEM_ADDR")),
            });

            let loaded = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Load,
                address,
                output: Some(loaded.clone()),
                inputs: vec![const_u64(RAM_SPACE_ID, 8), effective_addr.clone()],
                asm_mnemonic: Some(format!("{tag}_MEM_LOAD")),
            });
            (loaded, BitTestTarget::Mem(effective_addr))
        }
    };

    let bit_mask = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntLeft,
        address,
        output: Some(bit_mask.clone()),
        inputs: vec![const_u64(1, size), local_index],
        asm_mnemonic: Some(format!("{tag}_MASK")),
    });

    let bit_value = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(bit_value.clone()),
        inputs: vec![base_value.clone(), bit_mask.clone()],
        asm_mnemonic: Some(format!("{tag}_BIT")),
    });

    let cf = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntNotEqual,
        address,
        output: Some(cf.clone()),
        inputs: vec![bit_value, const_u64(0, size)],
        asm_mnemonic: Some(format!("{tag}_CF")),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_flag_cf()),
        inputs: vec![cf],
        asm_mnemonic: Some(format!("{tag}_CF_WRITE")),
    });

    let updated = match kind {
        BitTestKind::Bt => None,
        BitTestKind::Bts => {
            let out = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(out.clone()),
                inputs: vec![base_value, bit_mask],
                asm_mnemonic: Some(format!("{tag}_SET")),
            });
            Some(out)
        }
        BitTestKind::Btr => {
            let inv_mask = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntNegate,
                address,
                output: Some(inv_mask.clone()),
                inputs: vec![bit_mask],
                asm_mnemonic: Some(format!("{tag}_MASK_INV")),
            });
            let out = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntAnd,
                address,
                output: Some(out.clone()),
                inputs: vec![base_value, inv_mask],
                asm_mnemonic: Some(format!("{tag}_RESET")),
            });
            Some(out)
        }
        BitTestKind::Btc => {
            let out = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntXor,
                address,
                output: Some(out.clone()),
                inputs: vec![base_value, bit_mask],
                asm_mnemonic: Some(format!("{tag}_TOGGLE")),
            });
            Some(out)
        }
    };

    if let Some(value) = updated {
        match target {
            BitTestTarget::Reg(dst) => {
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::Copy,
                    address,
                    output: Some(dst),
                    inputs: vec![value],
                    asm_mnemonic: Some(format!("{tag}_WRITE")),
                });
            }
            BitTestTarget::Mem(addr_vn) => {
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::Store,
                    address,
                    output: None,
                    inputs: vec![const_u64(RAM_SPACE_ID, 8), addr_vn, value],
                    asm_mnemonic: Some(format!("{tag}_STORE")),
                });
            }
        }
    }

    ops
}

fn decode_movx(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    dst_size: u32,
    src_size: u32,
    is_sign_extend: bool,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, src_size, address, temp, &mut ops, seq)
    {
        Some(v) => v,
        None => return Vec::new(),
    };

    let src = materialize_rm_value(&decoded.rm, src_size, address, &mut ops, temp, seq);
    let dst = x86_reg(decoded.reg_index, dst_size);
    let (opcode, mnemonic) = if dst_size == src_size {
        (PcodeOpcode::Copy, if is_sign_extend { "MOVSX_WRITE" } else { "MOVZX_WRITE" })
    } else if is_sign_extend {
        (PcodeOpcode::IntSExt, "MOVSX_WRITE")
    } else {
        (PcodeOpcode::IntZExt, "MOVZX_WRITE")
    };

    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode,
        address,
        output: Some(dst),
        inputs: vec![src],
        asm_mnemonic: Some(mnemonic.to_string()),
    });

    ops
}

fn decode_imul_r_rm(
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

fn decode_bsf_bsr(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    is_reverse: bool,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, size, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let src = materialize_rm_value(&decoded.rm, size, address, &mut ops, temp, seq);
    let dst = x86_reg(decoded.reg_index, size);
    let zf = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntEqual,
        address,
        output: Some(zf.clone()),
        inputs: vec![src.clone(), const_u64(0, size)],
        asm_mnemonic: Some(if is_reverse {
            "BSR_ZF".to_string()
        } else {
            "BSF_ZF".to_string()
        }),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_flag_zf()),
        inputs: vec![zf.clone()],
        asm_mnemonic: Some(if is_reverse {
            "BSR_ZF_WRITE".to_string()
        } else {
            "BSF_ZF_WRITE".to_string()
        }),
    });

    let nonzero = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntNotEqual,
        address,
        output: Some(nonzero.clone()),
        inputs: vec![src.clone(), const_u64(0, size)],
        asm_mnemonic: Some(if is_reverse {
            "BSR_NONZERO".to_string()
        } else {
            "BSF_NONZERO".to_string()
        }),
    });

    let idx = if is_reverse {
        emit_bsr_index(&mut ops, address, size, src, temp, seq)
    } else {
        emit_bsf_index(&mut ops, address, size, src, temp, seq)
    };

    let merged = emit_conditional_value_merge(
        &mut ops,
        address,
        size,
        idx,
        dst.clone(),
        &nonzero,
        temp,
        seq,
        if is_reverse { "BSR" } else { "BSF" },
    );
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(dst),
        inputs: vec![merged],
        asm_mnemonic: Some(if is_reverse {
            "BSR_WRITE".to_string()
        } else {
            "BSF_WRITE".to_string()
        }),
    });

    ops
}

fn emit_bsf_index(
    ops: &mut Vec<PcodeOp>,
    address: u64,
    size: u32,
    src: Varnode,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Varnode {
    let neg = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Int2Comp,
        address,
        output: Some(neg.clone()),
        inputs: vec![src.clone()],
        asm_mnemonic: Some("BSF_NEG".to_string()),
    });
    let lowest = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(lowest.clone()),
        inputs: vec![src, neg],
        asm_mnemonic: Some("BSF_LOWEST".to_string()),
    });
    let minus_one = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSub,
        address,
        output: Some(minus_one.clone()),
        inputs: vec![lowest, const_u64(1, size)],
        asm_mnemonic: Some("BSF_MINUS1".to_string()),
    });
    let idx = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::PopCount,
        address,
        output: Some(idx.clone()),
        inputs: vec![minus_one],
        asm_mnemonic: Some("BSF_INDEX".to_string()),
    });
    idx
}

fn emit_bsr_index(
    ops: &mut Vec<PcodeOp>,
    address: u64,
    size: u32,
    src: Varnode,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Varnode {
    let bits = size.saturating_mul(8);
    let mut filled = src;
    for shift in [1u64, 2, 4, 8, 16, 32] {
        if shift >= u64::from(bits) {
            break;
        }
        let shr = temp.alloc(size);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntRight,
            address,
            output: Some(shr.clone()),
            inputs: vec![filled.clone(), const_u64(shift, size)],
            asm_mnemonic: Some("BSR_FILL_SHR".to_string()),
        });
        let or = temp.alloc(size);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntOr,
            address,
            output: Some(or.clone()),
            inputs: vec![filled, shr],
            asm_mnemonic: Some("BSR_FILL_OR".to_string()),
        });
        filled = or;
    }

    let count = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::PopCount,
        address,
        output: Some(count.clone()),
        inputs: vec![filled],
        asm_mnemonic: Some("BSR_POPCNT".to_string()),
    });
    let idx = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSub,
        address,
        output: Some(idx.clone()),
        inputs: vec![count, const_u64(1, size)],
        asm_mnemonic: Some("BSR_INDEX".to_string()),
    });
    idx
}

fn emit_conditional_value_merge(
    ops: &mut Vec<PcodeOp>,
    address: u64,
    size: u32,
    new_val: Varnode,
    old_val: Varnode,
    cond: &Varnode,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    tag: &str,
) -> Varnode {
    let cond_ext = if size == 1 {
        cond.clone()
    } else {
        let out = temp.alloc(size);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntZExt,
            address,
            output: Some(out.clone()),
            inputs: vec![cond.clone()],
            asm_mnemonic: Some(format!("{tag}_COND_ZEXT")),
        });
        out
    };

    let mask = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSub,
        address,
        output: Some(mask.clone()),
        inputs: vec![const_u64(0, size), cond_ext],
        asm_mnemonic: Some(format!("{tag}_COND_MASK")),
    });

    let inv_mask = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntNegate,
        address,
        output: Some(inv_mask.clone()),
        inputs: vec![mask.clone()],
        asm_mnemonic: Some(format!("{tag}_COND_NMASK")),
    });

    let kept = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(kept.clone()),
        inputs: vec![old_val, inv_mask],
        asm_mnemonic: Some(format!("{tag}_KEEP")),
    });

    let applied = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(applied.clone()),
        inputs: vec![new_val, mask],
        asm_mnemonic: Some(format!("{tag}_APPLY")),
    });

    let merged = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntOr,
        address,
        output: Some(merged.clone()),
        inputs: vec![kept, applied],
        asm_mnemonic: Some(format!("{tag}_MERGE")),
    });

    merged
}

pub(super) fn emit_mul_one_operand(
    rm: &RmOperand,
    size: u32,
    is_signed: bool,
    address: u64,
    ops: &mut Vec<PcodeOp>,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) {
    let implicit_hi = if size == 1 {
        x86_reg(4, 1)
    } else {
        x86_reg(2, size)
    };
    let lhs = x86_reg(0, size);
    let rhs = materialize_rm_value(rm, size, address, ops, temp, seq);
    let full_size = size.saturating_mul(2);
    let (ext_opcode, mul_mnemonic, cf_of_tag) = if is_signed {
        (PcodeOpcode::IntSExt, "IMUL", "IMUL")
    } else {
        (PcodeOpcode::IntZExt, "MUL", "MUL")
    };

    let lhs_ext = temp.alloc(full_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: ext_opcode,
        address,
        output: Some(lhs_ext.clone()),
        inputs: vec![lhs],
        asm_mnemonic: Some(format!("{mul_mnemonic}_LHS_EXT")),
    });
    let rhs_ext = temp.alloc(full_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: ext_opcode,
        address,
        output: Some(rhs_ext.clone()),
        inputs: vec![rhs],
        asm_mnemonic: Some(format!("{mul_mnemonic}_RHS_EXT")),
    });
    let full = temp.alloc(full_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntMult,
        address,
        output: Some(full.clone()),
        inputs: vec![lhs_ext, rhs_ext],
        asm_mnemonic: Some(mul_mnemonic.to_string()),
    });

    let low = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::SubPiece,
        address,
        output: Some(low.clone()),
        inputs: vec![full.clone(), const_u64(0, 4)],
        asm_mnemonic: Some(format!("{mul_mnemonic}_LOW")),
    });
    let high = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::SubPiece,
        address,
        output: Some(high.clone()),
        inputs: vec![full.clone(), const_u64(u64::from(size), 4)],
        asm_mnemonic: Some(format!("{mul_mnemonic}_HIGH")),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_reg(0, size)),
        inputs: vec![low.clone()],
        asm_mnemonic: Some(format!("{mul_mnemonic}_LO_WRITE")),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(implicit_hi),
        inputs: vec![high.clone()],
        asm_mnemonic: Some(format!("{mul_mnemonic}_HI_WRITE")),
    });

    let cf_of = temp.alloc(1);
    if is_signed {
        let low_ext = temp.alloc(full_size);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntSExt,
            address,
            output: Some(low_ext.clone()),
            inputs: vec![low],
            asm_mnemonic: Some(format!("{mul_mnemonic}_LOW_SEXT")),
        });
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntNotEqual,
            address,
            output: Some(cf_of.clone()),
            inputs: vec![full, low_ext],
            asm_mnemonic: Some(format!("{cf_of_tag}_CF_OF")),
        });
    } else {
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntNotEqual,
            address,
            output: Some(cf_of.clone()),
            inputs: vec![high, const_u64(0, size)],
            asm_mnemonic: Some(format!("{cf_of_tag}_CF_OF")),
        });
    }
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_flag_cf()),
        inputs: vec![cf_of.clone()],
        asm_mnemonic: Some(format!("{cf_of_tag}_CF_WRITE")),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_flag_of()),
        inputs: vec![cf_of],
        asm_mnemonic: Some(format!("{cf_of_tag}_OF_WRITE")),
    });
}

pub(super) fn emit_div_one_operand(
    rm: &RmOperand,
    size: u32,
    is_signed: bool,
    address: u64,
    ops: &mut Vec<PcodeOp>,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) {
    let dividend_hi = if size == 1 {
        x86_reg(4, 1)
    } else {
        x86_reg(2, size)
    };
    let dividend_lo = x86_reg(0, size);
    let divisor = materialize_rm_value(rm, size, address, ops, temp, seq);
    let policy_id = if is_signed {
        X86_IDIV_EXCEPTION_POLICY_ID
    } else {
        X86_DIV_EXCEPTION_POLICY_ID
    };
    let policy_tag = if is_signed {
        "IDIV_EXCEPTION_POLICY"
    } else {
        "DIV_EXCEPTION_POLICY"
    };
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: None,
        inputs: vec![
            const_u64(policy_id, 8),
            divisor.clone(),
            dividend_hi.clone(),
            dividend_lo.clone(),
            const_u64(u64::from(size), 4),
        ],
        asm_mnemonic: Some(policy_tag.to_string()),
    });

    let full_size = size.saturating_mul(2);
    let dividend = temp.alloc(full_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Piece,
        address,
        output: Some(dividend.clone()),
        inputs: vec![dividend_hi, dividend_lo],
        asm_mnemonic: Some(if is_signed {
            "IDIV_DIVIDEND"
        } else {
            "DIV_DIVIDEND"
        }
        .to_string()),
    });

    let divisor_ext = temp.alloc(full_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: if is_signed {
            PcodeOpcode::IntSExt
        } else {
            PcodeOpcode::IntZExt
        },
        address,
        output: Some(divisor_ext.clone()),
        inputs: vec![divisor],
        asm_mnemonic: Some(if is_signed {
            "IDIV_DIVISOR_EXT"
        } else {
            "DIV_DIVISOR_EXT"
        }
        .to_string()),
    });

    let quotient_full = temp.alloc(full_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: if is_signed {
            PcodeOpcode::IntSDiv
        } else {
            PcodeOpcode::IntDiv
        },
        address,
        output: Some(quotient_full.clone()),
        inputs: vec![dividend.clone(), divisor_ext.clone()],
        asm_mnemonic: Some(if is_signed { "IDIV_QUOT" } else { "DIV_QUOT" }.to_string()),
    });

    let remainder_full = temp.alloc(full_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: if is_signed {
            PcodeOpcode::IntSRem
        } else {
            PcodeOpcode::IntRem
        },
        address,
        output: Some(remainder_full.clone()),
        inputs: vec![dividend, divisor_ext],
        asm_mnemonic: Some(if is_signed { "IDIV_REM" } else { "DIV_REM" }.to_string()),
    });

    let quotient = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::SubPiece,
        address,
        output: Some(quotient.clone()),
        inputs: vec![quotient_full, const_u64(0, 4)],
        asm_mnemonic: Some(if is_signed { "IDIV_QUOT_LO" } else { "DIV_QUOT_LO" }.to_string()),
    });
    let remainder = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::SubPiece,
        address,
        output: Some(remainder.clone()),
        inputs: vec![remainder_full, const_u64(0, 4)],
        asm_mnemonic: Some(if is_signed { "IDIV_REM_LO" } else { "DIV_REM_LO" }.to_string()),
    });

    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_reg(0, size)),
        inputs: vec![quotient],
        asm_mnemonic: Some(if is_signed { "IDIV_QUOT_WRITE" } else { "DIV_QUOT_WRITE" }.to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(if size == 1 { x86_reg(4, 1) } else { x86_reg(2, size) }),
        inputs: vec![remainder],
        asm_mnemonic: Some(if is_signed { "IDIV_REM_WRITE" } else { "DIV_REM_WRITE" }.to_string()),
    });
}

fn decode_setcc(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    cond: u8,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, 1, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let mut alloc_tmp = |size: u32| temp.alloc(size);
    let pred = match emit_jcc_predicate_with_allocator(&mut ops, address, cond, seq, &mut alloc_tmp) {
        Some(v) => v,
        None => return Vec::new(),
    };
    match decoded.rm {
        RmOperand::Reg(dst) => {
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(dst),
                inputs: vec![pred],
                asm_mnemonic: Some("SETcc_WRITE".to_string()),
            });
        }
        RmOperand::Mem(addr_vn) => {
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Store,
                address,
                output: None,
                inputs: vec![const_u64(RAM_SPACE_ID, 8), addr_vn, pred],
                asm_mnemonic: Some("SETcc_STORE".to_string()),
            });
        }
    }

    ops
}

fn decode_cmovcc(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    cond: u8,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, size, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let dst = x86_reg(decoded.reg_index, size);
    let src = materialize_rm_value(&decoded.rm, size, address, &mut ops, temp, seq);
    let mut alloc_tmp = |alloc_size: u32| temp.alloc(alloc_size);
    let pred = match emit_jcc_predicate_with_allocator(&mut ops, address, cond, seq, &mut alloc_tmp) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let pred_ext = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntZExt,
        address,
        output: Some(pred_ext.clone()),
        inputs: vec![pred],
        asm_mnemonic: Some("CMOVcc_PRED_ZEXT".to_string()),
    });

    let mask = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSub,
        address,
        output: Some(mask.clone()),
        inputs: vec![const_u64(0, size), pred_ext],
        asm_mnemonic: Some("CMOVcc_MASK".to_string()),
    });

    let inv_mask = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntNegate,
        address,
        output: Some(inv_mask.clone()),
        inputs: vec![mask.clone()],
        asm_mnemonic: Some("CMOVcc_INV_MASK".to_string()),
    });

    let kept = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(kept.clone()),
        inputs: vec![dst.clone(), inv_mask],
        asm_mnemonic: Some("CMOVcc_KEEP".to_string()),
    });

    let applied = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(applied.clone()),
        inputs: vec![src, mask],
        asm_mnemonic: Some("CMOVcc_APPLY".to_string()),
    });

    let merged = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntOr,
        address,
        output: Some(merged.clone()),
        inputs: vec![kept, applied],
        asm_mnemonic: Some("CMOVcc_MERGE".to_string()),
    });

    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(dst),
        inputs: vec![merged],
        asm_mnemonic: Some("CMOVcc_WRITE".to_string()),
    });

    ops
}
