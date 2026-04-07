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
const X86_SIMD_SCALAR_INTRINSIC_BASE_ID: u64 = 0x0F80_00;
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
        0x05 => system::decode_system_policy(address, seq, X86_SYSCALL_POLICY_ID, "SYSCALL_POLICY"),
        0x06 => system::decode_system_policy(address, seq, X86_CLTS_POLICY_ID, "CLTS_POLICY"),
        0x07 => system::decode_system_policy(address, seq, X86_SYSRET_POLICY_ID, "SYSRET_POLICY"),
        0x08 => system::decode_system_policy(address, seq, X86_INVD_POLICY_ID, "INVD_POLICY"),
        0x09 => system::decode_system_policy(address, seq, X86_WBINVD_POLICY_ID, "WBINVD_POLICY"),
        0x0B => system::decode_system_policy(address, seq, X86_UD2_POLICY_ID, "UD2_POLICY"),
        0x1F => system::decode_nop_extended(insn, op_idx, prefix, size, address, temp, seq),
        0x30 => system::decode_system_policy(address, seq, X86_WRMSR_POLICY_ID, "WRMSR_POLICY"),
        0x31 => system::decode_rdtsc_policy(address, seq),
        0x32 => system::decode_system_policy(address, seq, X86_RDMSR_POLICY_ID, "RDMSR_POLICY"),
        0x34 => system::decode_system_policy(address, seq, X86_SYSENTER_POLICY_ID, "SYSENTER_POLICY"),
        0x35 => system::decode_system_policy(address, seq, X86_SYSEXIT_POLICY_ID, "SYSEXIT_POLICY"),
        0x38 => escape3byte::decode_three_byte_escape_semantic(insn, op_idx, prefix, size, address, temp, seq, false),
        0x3A => escape3byte::decode_three_byte_escape_semantic(insn, op_idx, prefix, size, address, temp, seq, true),
        0x77 => system::decode_system_policy(address, seq, X86_EMMS_POLICY_ID, "EMMS_POLICY"),
        0xA2 => system::decode_system_policy(address, seq, X86_CPUID_POLICY_ID, "CPUID_POLICY"),
        0xA0 => system::decode_system_policy(address, seq, X86_PUSH_FS_POLICY_ID, "PUSH_FS_POLICY"),
        0xA1 => system::decode_system_policy(address, seq, X86_POP_FS_POLICY_ID, "POP_FS_POLICY"),
        0xA4 | 0xA5 | 0xAC | 0xAD => {
            bitshift::decode_shld_shrd(insn, op_idx, prefix, size, address, temp, seq, ext)
        }
        0xAE => system::decode_clflush_policy(insn, op_idx, prefix, address, temp, seq),
        0xA3 => decode_bt_family(insn, op_idx, prefix, size, address, temp, seq, BitTestKind::Bt),
        0xAB => decode_bt_family(insn, op_idx, prefix, size, address, temp, seq, BitTestKind::Bts),
        0xB3 => decode_bt_family(insn, op_idx, prefix, size, address, temp, seq, BitTestKind::Btr),
        0xBB => decode_bt_family(insn, op_idx, prefix, size, address, temp, seq, BitTestKind::Btc),
        0xC8..=0xCF => bitshift::decode_bswap(prefix, size, ext, address, temp, seq),
        0xB6 | 0xB7 | 0xBE | 0xBF => {
            let src_size = if matches!(ext, 0xB6 | 0xBE) { 1 } else { 2 };
            let is_sign_extend = matches!(ext, 0xBE | 0xBF);
            movmuldiv::decode_movx(insn, op_idx, prefix, size, src_size, is_sign_extend, address, temp, seq)
        }
        0xAF => decode_imul_r_rm(insn, op_idx, prefix, size, address, temp, seq),
        0xBC => decode_bsf_bsr(insn, op_idx, prefix, size, address, temp, seq, false),
        0xBD => decode_bsf_bsr(insn, op_idx, prefix, size, address, temp, seq, true),
        0x40..=0x4F => decode_cmovcc(insn, op_idx, prefix, size, address, temp, seq, ext - 0x40),
        0x10..=0x17 | 0x28..=0x2F | 0x50..=0x76 | 0x78..=0x7F | 0xD4 | 0xD5 | 0xEB | 0xEF
        | 0xF8..=0xFE => {
            simd::decode_simd_semantic(insn, op_idx, prefix, size, address, temp, seq, ext)
        }
        0x90..=0x9F => decode_setcc(insn, op_idx, prefix, address, temp, seq, ext - 0x90),
        0xD8..=0xDF => {
            if prefix.operand_size_override || prefix.rep_prefix.is_some() {
                simd::decode_simd_semantic(insn, op_idx, prefix, size, address, temp, seq, ext)
            } else {
                Vec::new() // MMX unsupported for now
            }
        }
        _ => Vec::new(),
    }
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
