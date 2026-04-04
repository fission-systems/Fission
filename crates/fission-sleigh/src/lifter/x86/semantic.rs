use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use super::super::common::{
    const_u64, x86_flag_cf, x86_flag_of, x86_flag_pf, x86_flag_sf, x86_flag_zf, x86_reg,
    RAM_SPACE_ID, UNIQUE_SPACE_ID,
};

#[derive(Debug, Clone, Copy)]
struct PrefixState {
    operand_size_override: bool,
    address_size_override: bool,
    rex: u8,
}

#[derive(Debug, Clone)]
struct X86TempFactory {
    next: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AluKind {
    Add,
    Adc,
    Sub,
    Sbb,
    And,
    Or,
    Xor,
    Cmp,
    Test,
    Inc,
    Dec,
    Neg,
    Shl,
    Shr,
    Sar,
}

#[derive(Debug, Clone)]
enum RmOperand {
    Reg(Varnode),
    Mem(Varnode),
}

#[derive(Debug, Clone)]
struct DecodedModrm {
    reg_index: u32,
    reg_field: u8,
    rm: RmOperand,
    next_idx: usize,
}

#[derive(Debug, Clone)]
enum Destination {
    Reg(Varnode),
    Mem(Varnode),
    None,
}

impl X86TempFactory {
    fn new(address: u64) -> Self {
        Self {
            next: 0xE100_0000_0000_0000u64.wrapping_add(address.wrapping_shl(6)),
        }
    }

    fn alloc(&mut self, size: u32) -> Varnode {
        let vn = Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset: self.next,
            size,
            is_constant: false,
            constant_val: 0,
        };
        self.next = self.next.wrapping_add(8);
        vn
    }
}

pub(crate) fn decode_semantic(insn: &[u8], address: u64) -> Vec<PcodeOp> {
    if insn.is_empty() {
        return Vec::new();
    }

    let (op_idx, prefix) = parse_prefixes(insn);
    if op_idx >= insn.len() {
        return Vec::new();
    }

    let op = insn[op_idx];
    if op == 0x0F {
        return Vec::new();
    }

    let size = operand_size(&prefix);
    let mut seq = 1u32;
    let mut temp = X86TempFactory::new(address);

    match op {
        0xA9 => {
            let imm = match decode_immediate(insn, op_idx + 1, immediate_bytes_for_operand(size), size, size == 8) {
                Some(v) => v,
                None => return Vec::new(),
            };
            emit_alu_ops(
                address,
                size,
                x86_reg(0, size),
                imm,
                Destination::None,
                AluKind::Test,
                &mut temp,
                &mut seq,
            )
        }
        0x01 | 0x03 | 0x09 | 0x0B | 0x11 | 0x13 | 0x19 | 0x1B | 0x21 | 0x23 | 0x29 | 0x2B
        | 0x31 | 0x33 | 0x39 | 0x3B | 0x81 | 0x83 | 0x85 | 0xF7 | 0xFF | 0xC0 | 0xD0 | 0xD1
        | 0xD2 | 0xC1 | 0xD3 => {
            let mut pre_ops = Vec::new();
            let modrm_size = if matches!(op, 0xC0 | 0xD0 | 0xD2) { 1 } else { size };
            let decoded = match decode_modrm_operand(
                insn,
                op_idx,
                &prefix,
                modrm_size,
                address,
                &mut temp,
                &mut pre_ops,
                &mut seq,
            ) {
                Some(v) => v,
                None => return Vec::new(),
            };

            let mut ops = pre_ops;
            match op {
                0x01 | 0x09 | 0x21 | 0x29 | 0x31 => {
                    let kind = match op {
                        0x01 => AluKind::Add,
                        0x09 => AluKind::Or,
                        0x21 => AluKind::And,
                        0x29 => AluKind::Sub,
                        _ => AluKind::Xor,
                    };
                    let lhs = materialize_rm_value(&decoded.rm, size, address, &mut ops, &mut temp, &mut seq);
                    let rhs = x86_reg(decoded.reg_index, size);
                    ops.extend(emit_alu_ops(
                        address,
                        size,
                        lhs,
                        rhs,
                        destination_from_rm(&decoded.rm),
                        kind,
                        &mut temp,
                        &mut seq,
                    ));
                    ops
                }
                0x11 | 0x19 => {
                    let kind = if op == 0x11 { AluKind::Adc } else { AluKind::Sbb };
                    let lhs = materialize_rm_value(&decoded.rm, size, address, &mut ops, &mut temp, &mut seq);
                    let rhs = x86_reg(decoded.reg_index, size);
                    ops.extend(emit_alu_ops(
                        address,
                        size,
                        lhs,
                        rhs,
                        destination_from_rm(&decoded.rm),
                        kind,
                        &mut temp,
                        &mut seq,
                    ));
                    ops
                }
                0x03 | 0x0B | 0x23 | 0x2B | 0x33 => {
                    let kind = match op {
                        0x03 => AluKind::Add,
                        0x0B => AluKind::Or,
                        0x23 => AluKind::And,
                        0x2B => AluKind::Sub,
                        _ => AluKind::Xor,
                    };
                    let lhs = x86_reg(decoded.reg_index, size);
                    let rhs = materialize_rm_value(&decoded.rm, size, address, &mut ops, &mut temp, &mut seq);
                    ops.extend(emit_alu_ops(
                        address,
                        size,
                        lhs,
                        rhs,
                        Destination::Reg(x86_reg(decoded.reg_index, size)),
                        kind,
                        &mut temp,
                        &mut seq,
                    ));
                    ops
                }
                0x13 | 0x1B => {
                    let kind = if op == 0x13 { AluKind::Adc } else { AluKind::Sbb };
                    let lhs = x86_reg(decoded.reg_index, size);
                    let rhs = materialize_rm_value(&decoded.rm, size, address, &mut ops, &mut temp, &mut seq);
                    ops.extend(emit_alu_ops(
                        address,
                        size,
                        lhs,
                        rhs,
                        Destination::Reg(x86_reg(decoded.reg_index, size)),
                        kind,
                        &mut temp,
                        &mut seq,
                    ));
                    ops
                }
                0x39 => {
                    let lhs = materialize_rm_value(&decoded.rm, size, address, &mut ops, &mut temp, &mut seq);
                    let rhs = x86_reg(decoded.reg_index, size);
                    ops.extend(emit_alu_ops(
                        address,
                        size,
                        lhs,
                        rhs,
                        Destination::None,
                        AluKind::Cmp,
                        &mut temp,
                        &mut seq,
                    ));
                    ops
                }
                0x3B => {
                    let lhs = x86_reg(decoded.reg_index, size);
                    let rhs = materialize_rm_value(&decoded.rm, size, address, &mut ops, &mut temp, &mut seq);
                    ops.extend(emit_alu_ops(
                        address,
                        size,
                        lhs,
                        rhs,
                        Destination::None,
                        AluKind::Cmp,
                        &mut temp,
                        &mut seq,
                    ));
                    ops
                }
                0x85 => {
                    let lhs = materialize_rm_value(&decoded.rm, size, address, &mut ops, &mut temp, &mut seq);
                    let rhs = x86_reg(decoded.reg_index, size);
                    ops.extend(emit_alu_ops(
                        address,
                        size,
                        lhs,
                        rhs,
                        Destination::None,
                        AluKind::Test,
                        &mut temp,
                        &mut seq,
                    ));
                    ops
                }
                0x81 | 0x83 => {
                    let kind = match decoded.reg_field {
                        0 => AluKind::Add,
                        1 => AluKind::Or,
                        2 => AluKind::Adc,
                        3 => AluKind::Sbb,
                        4 => AluKind::And,
                        5 => AluKind::Sub,
                        6 => AluKind::Xor,
                        7 => AluKind::Cmp,
                        _ => return Vec::new(),
                    };
                    let imm = if op == 0x81 {
                        decode_immediate(
                            insn,
                            decoded.next_idx,
                            immediate_bytes_for_operand(size),
                            size,
                            size == 8,
                        )
                    } else {
                        decode_immediate(insn, decoded.next_idx, 1, size, true)
                    };
                    let rhs = match imm {
                        Some(v) => v,
                        None => return Vec::new(),
                    };
                    let lhs = materialize_rm_value(&decoded.rm, size, address, &mut ops, &mut temp, &mut seq);
                    let dst = if kind == AluKind::Cmp {
                        Destination::None
                    } else {
                        destination_from_rm(&decoded.rm)
                    };
                    ops.extend(emit_alu_ops(
                        address,
                        size,
                        lhs,
                        rhs,
                        dst,
                        kind,
                        &mut temp,
                        &mut seq,
                    ));
                    ops
                }
                0xF7 => {
                    if decoded.reg_field == 0 {
                        let rhs = match decode_immediate(
                            insn,
                            decoded.next_idx,
                            immediate_bytes_for_operand(size),
                            size,
                            size == 8,
                        ) {
                            Some(v) => v,
                            None => return Vec::new(),
                        };
                        let lhs = materialize_rm_value(&decoded.rm, size, address, &mut ops, &mut temp, &mut seq);
                        ops.extend(emit_alu_ops(
                            address,
                            size,
                            lhs,
                            rhs,
                            Destination::None,
                            AluKind::Test,
                            &mut temp,
                            &mut seq,
                        ));
                    } else if decoded.reg_field == 3 {
                        let lhs = materialize_rm_value(&decoded.rm, size, address, &mut ops, &mut temp, &mut seq);
                        ops.extend(emit_alu_ops(
                            address,
                            size,
                            lhs,
                            const_u64(0, size),
                            destination_from_rm(&decoded.rm),
                            AluKind::Neg,
                            &mut temp,
                            &mut seq,
                        ));
                    } else {
                        return Vec::new();
                    }
                    ops
                }
                0xFF => {
                    let kind = match decoded.reg_field {
                        0 => AluKind::Inc,
                        1 => AluKind::Dec,
                        _ => return Vec::new(),
                    };
                    let lhs = materialize_rm_value(&decoded.rm, size, address, &mut ops, &mut temp, &mut seq);
                    ops.extend(emit_alu_ops(
                        address,
                        size,
                        lhs,
                        const_u64(1, size),
                        destination_from_rm(&decoded.rm),
                        kind,
                        &mut temp,
                        &mut seq,
                    ));
                    ops
                }
                0xC0 | 0xD0 | 0xD1 | 0xD2 | 0xC1 | 0xD3 => {
                    let kind = match decoded.reg_field {
                        4 => AluKind::Shl,
                        5 => AluKind::Shr,
                        7 => AluKind::Sar,
                        _ => return Vec::new(),
                    };
                    let shift_size = if matches!(op, 0xC0 | 0xD0 | 0xD2) {
                        1
                    } else {
                        size
                    };
                    let count = if op == 0xD1 || op == 0xD0 {
                        const_u64(1, shift_size)
                    } else if op == 0xD3 || op == 0xD2 {
                        x86_reg(1, 1)
                    } else {
                        match decode_immediate(insn, decoded.next_idx, 1, shift_size, false) {
                            Some(v) => v,
                            None => return Vec::new(),
                        }
                    };
                    let lhs = materialize_rm_value(
                        &decoded.rm,
                        shift_size,
                        address,
                        &mut ops,
                        &mut temp,
                        &mut seq,
                    );
                    ops.extend(emit_alu_ops(
                        address,
                        shift_size,
                        lhs,
                        count,
                        destination_from_rm(&decoded.rm),
                        kind,
                        &mut temp,
                        &mut seq,
                    ));
                    ops
                }
                _ => Vec::new(),
            }
        }
        _ => Vec::new(),
    }
}


mod alu;
mod addressing;
#[cfg(test)]
mod tests;

use self::alu::emit_alu_ops;
use self::addressing::{
    decode_immediate, decode_modrm_operand, immediate_bytes_for_operand, operand_size, parse_prefixes,
};

fn next_seq(seq: &mut u32) -> u32 {
    let cur = *seq;
    *seq = seq.saturating_add(1);
    cur
}

fn destination_from_rm(rm: &RmOperand) -> Destination {
    match rm {
        RmOperand::Reg(v) => Destination::Reg(v.clone()),
        RmOperand::Mem(addr) => Destination::Mem(addr.clone()),
    }
}

fn materialize_rm_value(
    rm: &RmOperand,
    size: u32,
    address: u64,
    ops: &mut Vec<PcodeOp>,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Varnode {
    match rm {
        RmOperand::Reg(v) => v.clone(),
        RmOperand::Mem(addr) => {
            let out = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Load,
                address,
                output: Some(out.clone()),
                inputs: vec![const_u64(RAM_SPACE_ID, 8), addr.clone()],
                asm_mnemonic: Some("RM_LOAD".to_string()),
            });
            out
        }
    }
}

