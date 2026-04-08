use super::*;
use self::bitops::BitTestKind;

mod bitshift;
mod bitops;
mod cond;
mod escape3byte;
mod imul;
mod movmuldiv;
mod simd;
mod system;

pub(super) use self::system::decode_x87_policy;
mod vex;
pub(super) use self::vex::decode_vex_semantic;

const X86_RDTSC_POLICY_ID: u64 = 0x0F31;
const X86_CLFLUSH_POLICY_ID: u64 = 0x0FAE07;
const X86_CLFLUSHOPT_POLICY_ID: u64 = 0x660FAE07;
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
const X86_SIMD_SCALAR_INTRINSIC_BASE_ID: u64 = 0x0F80_00;
const X86_3BYTE_0F38_POLICY_BASE_ID: u64 = 0x0F38_00;
const X86_3BYTE_0F3A_POLICY_BASE_ID: u64 = 0x0F3A_00;
const X86_CMPXCHG8B_POLICY_ID: u64 = 0x0FC7_01;
const X86_PUSH_GS_POLICY_ID: u64 = 0x0FA8;
const X86_POP_GS_POLICY_ID: u64 = 0x0FA9;
const X86_LGDT_POLICY_ID: u64 = 0x0F01_02;
const X86_SGDT_POLICY_ID: u64 = 0x0F01_00;
const X86_LIDT_POLICY_ID: u64 = 0x0F01_03;
const X86_SIDT_POLICY_ID: u64 = 0x0F01_01;
const X86_LMSW_POLICY_ID: u64 = 0x0F01_06;
const X86_SMSW_POLICY_ID: u64 = 0x0F01_04;
const X86_INVLPG_POLICY_ID: u64 = 0x0F01_07;
const X86_INVD_POLICY_ID2: u64 = 0x0F01_05;
const X86_MOV_CR_POLICY_ID: u64 = 0x0F20_00;
const X86_MOV_DR_POLICY_ID: u64 = 0x0F21_00;
const X86_RDPMC_POLICY_ID: u64 = 0x0F33;
// 0F 00 group: segment descriptor table instructions
const X86_SLDT_POLICY_ID: u64 = 0x0F00_00;
const X86_STR_POLICY_ID: u64 = 0x0F00_01;
const X86_LLDT_POLICY_ID: u64 = 0x0F00_02;
const X86_LTR_POLICY_ID: u64 = 0x0F00_03;
const X86_VERR_POLICY_ID: u64 = 0x0F00_04;
const X86_VERW_POLICY_ID: u64 = 0x0F00_05;
// Far load: LSS/LFS/LGS
const X86_LSS_POLICY_ID: u64 = 0x0FB2_00;
const X86_LFS_POLICY_ID: u64 = 0x0FB4_00;
const X86_LGS_POLICY_ID: u64 = 0x0FB5_00;

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
        0x00 => decode_0f00_group(insn, op_idx, prefix, address, temp, seq),
        0x01 => decode_0f01_group(insn, op_idx, prefix, address, temp, seq),
        0x05 => system::decode_system_policy(address, seq, X86_SYSCALL_POLICY_ID, "SYSCALL_POLICY"),
        0x06 => system::decode_system_policy(address, seq, X86_CLTS_POLICY_ID, "CLTS_POLICY"),
        0x07 => system::decode_system_policy(address, seq, X86_SYSRET_POLICY_ID, "SYSRET_POLICY"),
        0x08 => system::decode_system_policy(address, seq, X86_INVD_POLICY_ID, "INVD_POLICY"),
        0x09 => system::decode_system_policy(address, seq, X86_WBINVD_POLICY_ID, "WBINVD_POLICY"),
        0x0B => system::decode_system_policy(address, seq, X86_UD2_POLICY_ID, "UD2_POLICY"),
        // MOV CR0–7, r64 / r64, CR0–7
        0x20 | 0x22 => {
            let mut ops = Vec::new();
            let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, size, address, temp, &mut ops, seq) {
                Some(v) => v,
                None => return Vec::new(),
            };
            let _ = decoded;
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::CallOther,
                address,
                output: None,
                inputs: vec![const_u64(X86_MOV_CR_POLICY_ID + u64::from(ext), 8)],
                asm_mnemonic: Some("MOV_CR_POLICY".to_string()),
            });
            ops
        }
        // MOV DR0–7, r64 / r64, DR0–7
        0x21 | 0x23 => {
            let mut ops = Vec::new();
            let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, size, address, temp, &mut ops, seq) {
                Some(v) => v,
                None => return Vec::new(),
            };
            let _ = decoded;
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::CallOther,
                address,
                output: None,
                inputs: vec![const_u64(X86_MOV_DR_POLICY_ID + u64::from(ext), 8)],
                asm_mnemonic: Some("MOV_DR_POLICY".to_string()),
            });
            ops
        }
        0x33 => system::decode_system_policy(address, seq, X86_RDPMC_POLICY_ID, "RDPMC_POLICY"),
        // PREFETCH hints (PREFETCHT0/T1/T2, PREFETCHNTA, and reserved NOP variants):
        // No P-code side effect for decompilation; 0x1F is handled by decode_nop_extended.
        0x18 | 0x19 | 0x1A | 0x1B | 0x1C | 0x1D | 0x1E => Vec::new(),
        0x1F => system::decode_nop_extended(insn, op_idx, prefix, size, address, temp, seq),
        0x30 => system::decode_system_policy(address, seq, X86_WRMSR_POLICY_ID, "WRMSR_POLICY"),
        0x31 => system::decode_rdtsc_policy(address, seq),
        0x32 => system::decode_system_policy(address, seq, X86_RDMSR_POLICY_ID, "RDMSR_POLICY"),
        0x34 => system::decode_system_policy(address, seq, X86_SYSENTER_POLICY_ID, "SYSENTER_POLICY"),
        0x35 => system::decode_system_policy(address, seq, X86_SYSEXIT_POLICY_ID, "SYSEXIT_POLICY"),
        0x38 => escape3byte::decode_three_byte_escape_semantic(insn, op_idx, prefix, size, address, temp, seq, false, 0),
        0x3A => escape3byte::decode_three_byte_escape_semantic(insn, op_idx, prefix, size, address, temp, seq, true, 0),
        0x77 => system::decode_system_policy(address, seq, X86_EMMS_POLICY_ID, "EMMS_POLICY"),
        0xA2 => system::decode_system_policy(address, seq, X86_CPUID_POLICY_ID, "CPUID_POLICY"),
        0xA0 => system::decode_system_policy(address, seq, X86_PUSH_FS_POLICY_ID, "PUSH_FS_POLICY"),
        0xA1 => system::decode_system_policy(address, seq, X86_POP_FS_POLICY_ID, "POP_FS_POLICY"),
        0xA8 => system::decode_system_policy(address, seq, X86_PUSH_GS_POLICY_ID, "PUSH_GS_POLICY"),
        0xA9 => system::decode_system_policy(address, seq, X86_POP_GS_POLICY_ID, "POP_GS_POLICY"),
        0xA4 | 0xA5 | 0xAC | 0xAD => {
            bitshift::decode_shld_shrd(insn, op_idx, prefix, size, address, temp, seq, ext)
        }
        0xAE => system::decode_0fae_group(insn, op_idx, prefix, address, temp, seq),
        // LSS/LFS/LGS: load far pointer (offset → reg, segment → seg register) → CallOther
        0xB2 => decode_lss_lfs_lgs(insn, op_idx, prefix, size, address, temp, seq, X86_LSS_POLICY_ID, "LSS_POLICY"),
        0xB4 => decode_lss_lfs_lgs(insn, op_idx, prefix, size, address, temp, seq, X86_LFS_POLICY_ID, "LFS_POLICY"),
        0xB5 => decode_lss_lfs_lgs(insn, op_idx, prefix, size, address, temp, seq, X86_LGS_POLICY_ID, "LGS_POLICY"),
        0xA3 => decode_bt_family(insn, op_idx, prefix, size, address, temp, seq, BitTestKind::Bt),
        0xAB => decode_bt_family(insn, op_idx, prefix, size, address, temp, seq, BitTestKind::Bts),
        0xB3 => decode_bt_family(insn, op_idx, prefix, size, address, temp, seq, BitTestKind::Btr),
        0xBB => decode_bt_family(insn, op_idx, prefix, size, address, temp, seq, BitTestKind::Btc),
        // XADD: temp = r/m + r; r = r/m; r/m = temp; flags = ADD flags
        0xC0 | 0xC1 => decode_xadd(insn, op_idx, prefix, size, address, temp, seq, ext),
        // CMPXCHG: compare accumulator with r/m; if equal r/m = r; else accumulator = r/m
        0xB0 | 0xB1 => decode_cmpxchg(insn, op_idx, prefix, size, address, temp, seq, ext),
        // CMPXCHG8B/16B: 64/128-bit compare-and-swap → CallOther policy
        0xC7 => decode_cmpxchg8b(insn, op_idx, prefix, address, temp, seq),
        // CMPPS/PD/SS/SD: comparison with imm8 predicate
        0xC2 => simd::decode_simd_semantic(insn, op_idx, prefix, size, address, temp, seq, ext),
        // SHUFPS/PD: shuffle with imm8 control
        0xC6 => simd::decode_simd_semantic(insn, op_idx, prefix, size, address, temp, seq, ext),
        0xC8..=0xCF => bitshift::decode_bswap(prefix, size, ext, address, temp, seq),
        0xB6 | 0xB7 | 0xBE | 0xBF => {
            let src_size = if matches!(ext, 0xB6 | 0xBE) { 1 } else { 2 };
            let is_sign_extend = matches!(ext, 0xBE | 0xBF);
            movmuldiv::decode_movx(insn, op_idx, prefix, size, src_size, is_sign_extend, address, temp, seq)
        }
        0xAF => decode_imul_r_rm(insn, op_idx, prefix, size, address, temp, seq),
        // BT/BTS/BTR/BTC r/m, imm8 (/4–/7 in reg field)
        0xBA => bitops::decode_bt_imm8(insn, op_idx, prefix, size, address, temp, seq),
        0xB8 => decode_popcnt(insn, op_idx, prefix, size, address, temp, seq),
        0xBC => decode_bsf_bsr(insn, op_idx, prefix, size, address, temp, seq, false),
        0xBD => decode_bsf_bsr(insn, op_idx, prefix, size, address, temp, seq, true),
        0x40..=0x4F => decode_cmovcc(insn, op_idx, prefix, size, address, temp, seq, ext - 0x40),
        0x10..=0x17 | 0x28..=0x2F | 0x50..=0x76 | 0x78..=0x7F | 0xD4 | 0xD5
        | 0xE0..=0xEF | 0xF8..=0xFE => {
            simd::decode_simd_semantic(insn, op_idx, prefix, size, address, temp, seq, ext)
        }
        0x90..=0x9F => decode_setcc(insn, op_idx, prefix, address, temp, seq, ext - 0x90),
        // D8-DF: SSE2/MMX — always route to simd (no-prefix = MMX → SIMD_POLICY CallOther)
        0xD8..=0xDF => {
            simd::decode_simd_semantic(insn, op_idx, prefix, size, address, temp, seq, ext)
        }
        _ => Vec::new(),
    }
}

/// 0F 01 group: SGDT/LGDT/SIDT/LIDT/SMSW/LMSW/INVLPG/SWAPGS/etc — all → CallOther.
fn decode_0f01_group(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, 8, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let (policy_id, mnem) = match decoded.reg_field {
        0 => (X86_SGDT_POLICY_ID, "SGDT_POLICY"),
        1 => (X86_SIDT_POLICY_ID, "SIDT_POLICY"),
        2 => (X86_LGDT_POLICY_ID, "LGDT_POLICY"),
        3 => (X86_LIDT_POLICY_ID, "LIDT_POLICY"),
        4 => (X86_SMSW_POLICY_ID, "SMSW_POLICY"),
        6 => (X86_LMSW_POLICY_ID, "LMSW_POLICY"),
        5 => (X86_INVD_POLICY_ID2, "VMXON_POLICY"),
        7 => (X86_INVLPG_POLICY_ID, "INVLPG_POLICY"),
        _ => return Vec::new(),
    };
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: None,
        inputs: vec![const_u64(policy_id, 8)],
        asm_mnemonic: Some(mnem.to_string()),
    });
    ops
}

/// 0F 00 group: SLDT/STR/LLDT/LTR/VERR/VERW — all → CallOther.
fn decode_0f00_group(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, 2, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let (policy_id, mnem) = match decoded.reg_field {
        0 => (X86_SLDT_POLICY_ID, "SLDT_POLICY"),
        1 => (X86_STR_POLICY_ID, "STR_POLICY"),
        2 => (X86_LLDT_POLICY_ID, "LLDT_POLICY"),
        3 => (X86_LTR_POLICY_ID, "LTR_POLICY"),
        4 => (X86_VERR_POLICY_ID, "VERR_POLICY"),
        5 => (X86_VERW_POLICY_ID, "VERW_POLICY"),
        _ => return Vec::new(),
    };
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: None,
        inputs: vec![const_u64(policy_id, 8)],
        asm_mnemonic: Some(mnem.to_string()),
    });
    ops
}

/// LSS/LFS/LGS: far pointer load → CallOther with dst reg hint.
fn decode_lss_lfs_lgs(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    policy_id: u64,
    mnem: &'static str,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, size, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let dst = x86_reg(decoded.reg_index, size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: Some(dst),
        inputs: vec![const_u64(policy_id, 8)],
        asm_mnemonic: Some(mnem.to_string()),
    });
    ops
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
    bitops::decode_bt_family(insn, op_idx, prefix, size, address, temp, seq, kind)
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
    imul::decode_imul_r_rm(insn, op_idx, prefix, size, address, temp, seq)
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
    imul::decode_imul_r_rm_imm(insn, op_idx, prefix, size, address, temp, seq, is_imm8)
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
    bitops::decode_bsf_bsr(insn, op_idx, prefix, size, address, temp, seq, is_reverse)
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
    movmuldiv::emit_mul_one_operand(rm, size, is_signed, address, ops, temp, seq)
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
    movmuldiv::emit_div_one_operand(rm, size, is_signed, address, ops, temp, seq)
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
    cond::decode_setcc(insn, op_idx, prefix, address, temp, seq, cond)
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
    cond::decode_cmovcc(insn, op_idx, prefix, size, address, temp, seq, cond)
}

/// XADD r/m, r: temp = r/m + r; r = old_r/m; r/m = temp; flags from ADD.
fn decode_xadd(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    ext: u8,
) -> Vec<PcodeOp> {
    let xadd_size = if ext == 0xC0 { 1 } else { size };
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, xadd_size, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let rm_val = materialize_rm_value(&decoded.rm, xadd_size, address, &mut ops, temp, seq);
    let reg = x86_reg(decoded.reg_index, xadd_size);

    // sum = r/m + r
    let sum = temp.alloc(xadd_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAdd,
        address,
        output: Some(sum.clone()),
        inputs: vec![rm_val.clone(), reg.clone()],
        asm_mnemonic: Some("XADD_SUM".to_string()),
    });

    // Write r := old r/m
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(reg),
        inputs: vec![rm_val.clone()],
        asm_mnemonic: Some("XADD_OLD_RM".to_string()),
    });

    // Write r/m := sum (write_rm_value drains and returns the full ops vec)
    ops = write_rm_value(&decoded.rm, sum.clone(), address, &mut ops, seq, "XADD");

    // Flags from the ADD result
    ops.extend(emit_xadd_flags(address, xadd_size, rm_val, sum, temp, seq));

    ops
}

fn emit_xadd_flags(
    address: u64,
    size: u32,
    lhs: Varnode,
    result: Varnode,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    // Carry: result < lhs (unsigned overflow)
    let mut ops = Vec::new();
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntCarry,
        address,
        output: Some(x86_flag_cf()),
        inputs: vec![lhs.clone(), result.clone()],
        asm_mnemonic: Some("XADD_CF".to_string()),
    });
    // Overflow: signed overflow
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSCarry,
        address,
        output: Some(x86_flag_of()),
        inputs: vec![lhs.clone(), result.clone()],
        asm_mnemonic: Some("XADD_OF".to_string()),
    });
    // ZF: result == 0
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntEqual,
        address,
        output: Some(x86_flag_zf()),
        inputs: vec![result.clone(), const_u64(0, size)],
        asm_mnemonic: Some("XADD_ZF".to_string()),
    });
    // SF: MSB of result
    let size_bits = u64::from(size.saturating_mul(8));
    let sf_raw = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntRight,
        address,
        output: Some(sf_raw.clone()),
        inputs: vec![result.clone(), const_u64(size_bits - 1, size)],
        asm_mnemonic: Some("XADD_SF_RAW".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::SubPiece,
        address,
        output: Some(x86_flag_sf()),
        inputs: vec![sf_raw, const_u64(0, 4)],
        asm_mnemonic: Some("XADD_SF".to_string()),
    });
    // PF: parity of low 8 bits
    let low8 = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::SubPiece,
        address,
        output: Some(low8.clone()),
        inputs: vec![result, const_u64(0, 4)],
        asm_mnemonic: Some("XADD_PF_LOW8".to_string()),
    });
    let popcount = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::PopCount,
        address,
        output: Some(popcount.clone()),
        inputs: vec![low8],
        asm_mnemonic: Some("XADD_PF_POPCNT".to_string()),
    });
    let pf_raw = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(pf_raw.clone()),
        inputs: vec![popcount, const_u64(1, 1)],
        asm_mnemonic: Some("XADD_PF_LSB".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntEqual,
        address,
        output: Some(x86_flag_pf()),
        inputs: vec![pf_raw, const_u64(0, 1)],
        asm_mnemonic: Some("XADD_PF".to_string()),
    });
    ops
}

/// CMPXCHG r/m, r: compare accumulator with r/m; if equal: ZF=1, r/m=r; else: ZF=0, accum=r/m.
fn decode_cmpxchg(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    ext: u8,
) -> Vec<PcodeOp> {
    let cmp_size = if ext == 0xB0 { 1 } else { size };
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, cmp_size, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let accum = x86_reg(0, cmp_size);
    let rm_val = materialize_rm_value(&decoded.rm, cmp_size, address, &mut ops, temp, seq);
    let reg = x86_reg(decoded.reg_index, cmp_size);

    // Emit CMP flags (accum - r/m)
    let diff = temp.alloc(cmp_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSub,
        address,
        output: Some(diff.clone()),
        inputs: vec![accum.clone(), rm_val.clone()],
        asm_mnemonic: Some("CMPXCHG_CMP".to_string()),
    });
    // ZF = (accum == rm_val)
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntEqual,
        address,
        output: Some(x86_flag_zf()),
        inputs: vec![accum.clone(), rm_val.clone()],
        asm_mnemonic: Some("CMPXCHG_ZF".to_string()),
    });
    // CF = borrow (accum < rm_val unsigned)
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntLess,
        address,
        output: Some(x86_flag_cf()),
        inputs: vec![accum.clone(), rm_val.clone()],
        asm_mnemonic: Some("CMPXCHG_CF".to_string()),
    });
    // SF = MSB(diff)
    let size_bits = u64::from(cmp_size.saturating_mul(8));
    let sf_raw = temp.alloc(cmp_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntRight,
        address,
        output: Some(sf_raw.clone()),
        inputs: vec![diff.clone(), const_u64(size_bits - 1, cmp_size)],
        asm_mnemonic: Some("CMPXCHG_SF_RAW".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::SubPiece,
        address,
        output: Some(x86_flag_sf()),
        inputs: vec![sf_raw, const_u64(0, 4)],
        asm_mnemonic: Some("CMPXCHG_SF".to_string()),
    });
    // OF = signed overflow (accum - rm_val)
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSBorrow,
        address,
        output: Some(x86_flag_of()),
        inputs: vec![accum.clone(), rm_val.clone()],
        asm_mnemonic: Some("CMPXCHG_OF".to_string()),
    });

    // Conditional select using sign-extended ZF mask:
    // zf_mask: all 1s if ZF=1, all 0s if ZF=0
    let zf_ext = temp.alloc(cmp_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSExt,
        address,
        output: Some(zf_ext.clone()),
        inputs: vec![x86_flag_zf()],
        asm_mnemonic: Some("CMPXCHG_ZF_MASK".to_string()),
    });
    let not_zf_ext = temp.alloc(cmp_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntXor,
        address,
        output: Some(not_zf_ext.clone()),
        inputs: vec![zf_ext.clone(), const_u64(u64::MAX, cmp_size)],
        asm_mnemonic: Some("CMPXCHG_NOT_ZF_MASK".to_string()),
    });

    // If ZF=1: r/m = r  (use reg & zf_mask | rm & not_zf_mask)
    let reg_part = temp.alloc(cmp_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(reg_part.clone()),
        inputs: vec![reg, zf_ext.clone()],
        asm_mnemonic: Some("CMPXCHG_REG_PART".to_string()),
    });
    let rm_part = temp.alloc(cmp_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(rm_part.clone()),
        inputs: vec![rm_val.clone(), not_zf_ext.clone()],
        asm_mnemonic: Some("CMPXCHG_RM_PART".to_string()),
    });
    let new_rm_val = temp.alloc(cmp_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntOr,
        address,
        output: Some(new_rm_val.clone()),
        inputs: vec![reg_part, rm_part],
        asm_mnemonic: Some("CMPXCHG_NEW_RM".to_string()),
    });
    ops = write_rm_value(&decoded.rm, new_rm_val, address, &mut ops, seq, "CMPXCHG_RM");

    // If ZF=0: accumulator = r/m  (use rm & not_zf_mask | accum & zf_mask)
    let accum_part = temp.alloc(cmp_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(accum_part.clone()),
        inputs: vec![accum.clone(), zf_ext],
        asm_mnemonic: Some("CMPXCHG_ACCUM_PART".to_string()),
    });
    let rm_part2 = temp.alloc(cmp_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(rm_part2.clone()),
        inputs: vec![rm_val, not_zf_ext],
        asm_mnemonic: Some("CMPXCHG_RM_PART2".to_string()),
    });
    let new_accum = temp.alloc(cmp_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntOr,
        address,
        output: Some(new_accum.clone()),
        inputs: vec![accum_part, rm_part2],
        asm_mnemonic: Some("CMPXCHG_NEW_ACCUM".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(accum),
        inputs: vec![new_accum],
        asm_mnemonic: Some("CMPXCHG_ACCUM_WRITE".to_string()),
    });

    ops
}

/// CMPXCHG8B/16B m64/m128 → CallOther policy (complex 64-bit CAS).
fn decode_cmpxchg8b(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, 8, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };
    if decoded.reg_field != 1 {
        return Vec::new(); // only /1 is CMPXCHG8B
    }
    let addr_vn = match decoded.rm {
        RmOperand::Mem(a) => a,
        RmOperand::Reg(_) => return Vec::new(),
    };
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: None,
        inputs: vec![const_u64(X86_CMPXCHG8B_POLICY_ID, 8), addr_vn],
        asm_mnemonic: Some("CMPXCHG8B_POLICY".to_string()),
    });
    ops
}

/// F3 0F B8: POPCNT r, r/m — count set bits.
/// Requires REP (F3) prefix; without it the encoding is reserved.
fn decode_popcnt(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    if prefix.rep_prefix != Some(RepPrefix::Rep) {
        return Vec::new();
    }
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, size, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let src = materialize_rm_value(&decoded.rm, size, address, &mut ops, temp, seq);
    let src_saved = src.clone();
    let dst = x86_reg(decoded.reg_index, size);

    // result = popcount(src)
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::PopCount,
        address,
        output: Some(dst),
        inputs: vec![src],
        asm_mnemonic: Some("POPCNT".to_string()),
    });

    // ZF = (src == 0); CF/OF/SF/AF/PF = 0
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntEqual,
        address,
        output: Some(x86_flag_zf()),
        inputs: vec![src_saved, const_u64(0, size)],
        asm_mnemonic: Some("POPCNT_ZF".to_string()),
    });
    for (flag_fn, mnem) in [
        (x86_flag_cf(), "POPCNT_CF"),
        (x86_flag_of(), "POPCNT_OF"),
        (x86_flag_sf(), "POPCNT_SF"),
        (x86_flag_af(), "POPCNT_AF"),
        (x86_flag_pf(), "POPCNT_PF"),
    ] {
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::Copy,
            address,
            output: Some(flag_fn),
            inputs: vec![const_u64(0, 1)],
            asm_mnemonic: Some(mnem.to_string()),
        });
    }

    ops
}
