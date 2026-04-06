use super::*;

pub(super) fn decode_system_policy(address: u64, seq: &mut u32, policy_id: u64, mnemonic: &str) -> Vec<PcodeOp> {
    vec![PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: None,
        inputs: vec![const_u64(policy_id, 8)],
        asm_mnemonic: Some(mnemonic.to_string()),
    }]
}

pub(super) fn decode_rdtsc_policy(address: u64, seq: &mut u32) -> Vec<PcodeOp> {
    vec![PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: None,
        inputs: vec![const_u64(X86_RDTSC_POLICY_ID, 8)],
        asm_mnemonic: Some("RDTSC_POLICY".to_string()),
    }]
}

pub(super) fn decode_clflush_policy(
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

pub(super) fn decode_nop_extended(
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

pub(super) fn decode_x87_policy(address: u64, seq: &mut u32, ext: u8) -> Vec<PcodeOp> {
    vec![PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: None,
        inputs: vec![const_u64(X86_X87_POLICY_BASE_ID + u64::from(ext), 8)],
        asm_mnemonic: Some("X87_POLICY".to_string()),
    }]
}
