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

const X86_DIV_EXCEPTION_POLICY_ID: u64 = 0xF706;
const X86_IDIV_EXCEPTION_POLICY_ID: u64 = 0xF707;

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
    let size = operand_size(&prefix);
    let mut seq = 1u32;
    let mut temp = X86TempFactory::new(address);

    if op == 0x0F {
        return decode_extended_semantic(insn, op_idx, &prefix, size, address, &mut temp, &mut seq);
    }

    match op {
        0x50..=0x57 => {
            let slot_size = stack_operand_size(&prefix);
            let reg = u32::from(op - 0x50) + rex_b(&prefix);
            let src = x86_reg(reg, slot_size);
            let mut ops = Vec::new();
            emit_stack_push(
                address,
                src,
                slot_size,
                &mut ops,
                &mut temp,
                &mut seq,
                "PUSH_REG",
            );
            ops
        }
        0x58..=0x5F => {
            let slot_size = stack_operand_size(&prefix);
            let reg = u32::from(op - 0x58) + rex_b(&prefix);
            let mut ops = Vec::new();
            let popped = emit_stack_pop(
                address,
                slot_size,
                &mut ops,
                &mut temp,
                &mut seq,
                "POP_REG",
            );
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(x86_reg(reg, slot_size)),
                inputs: vec![popped],
                asm_mnemonic: Some("POP_REG_WRITE".to_string()),
            });
            ops
        }
        0x68 | 0x6A => {
            let slot_size = stack_operand_size(&prefix);
            let (imm_bytes, imm_sign_extend) = if op == 0x6A {
                (1usize, true)
            } else if slot_size == 2 {
                (2usize, true)
            } else {
                (4usize, true)
            };
            let imm = match decode_immediate(insn, op_idx + 1, imm_bytes, slot_size, imm_sign_extend) {
                Some(v) => v,
                None => return Vec::new(),
            };
            let mut ops = Vec::new();
            emit_stack_push(
                address,
                imm,
                slot_size,
                &mut ops,
                &mut temp,
                &mut seq,
                "PUSH_IMM",
            );
            ops
        }
        0x8F => {
            let slot_size = stack_operand_size(&prefix);
            let mut pre_ops = Vec::new();
            let decoded = match decode_modrm_operand(
                insn,
                op_idx,
                &prefix,
                slot_size,
                address,
                &mut temp,
                &mut pre_ops,
                &mut seq,
            ) {
                Some(v) => v,
                None => return Vec::new(),
            };
            if decoded.reg_field != 0 {
                return Vec::new();
            }
            let mut ops = pre_ops;
            let popped = emit_stack_pop(
                address,
                slot_size,
                &mut ops,
                &mut temp,
                &mut seq,
                "POP_RM",
            );
            write_rm_value(&decoded.rm, popped, address, &mut ops, &mut seq, "POP")
        }
        0xE8 => {
            let slot_size = stack_operand_size(&prefix);
            let ret_addr = const_u64(address.wrapping_add(insn.len() as u64), slot_size);
            let mut ops = Vec::new();
            emit_stack_push(
                address,
                ret_addr,
                slot_size,
                &mut ops,
                &mut temp,
                &mut seq,
                "CALL",
            );
            ops
        }
        0xC3 | 0xCB | 0xC2 | 0xCA => {
            let slot_size = stack_operand_size(&prefix);
            let mut ops = Vec::new();
            let _ret_addr = emit_stack_pop(
                address,
                slot_size,
                &mut ops,
                &mut temp,
                &mut seq,
                "RET",
            );

            if matches!(op, 0xC2 | 0xCA) {
                let pop_imm = match decode_immediate(insn, op_idx + 1, 2, 8, false) {
                    Some(v) => v,
                    None => return Vec::new(),
                };
                let sp = stack_pointer_reg();
                let sp_next = temp.alloc(8);
                ops.push(PcodeOp {
                    seq_num: next_seq(&mut seq),
                    opcode: PcodeOpcode::IntAdd,
                    address,
                    output: Some(sp_next.clone()),
                    inputs: vec![sp.clone(), pop_imm],
                    asm_mnemonic: Some("RET_IMM_SP_ADD".to_string()),
                });
                ops.push(PcodeOp {
                    seq_num: next_seq(&mut seq),
                    opcode: PcodeOpcode::Copy,
                    address,
                    output: Some(sp),
                    inputs: vec![sp_next],
                    asm_mnemonic: Some("RET_IMM_SP_WRITE".to_string()),
                });
            }

            ops
        }
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
        0x88 | 0x89 | 0x8A | 0x8B | 0x8D | 0xC6 | 0xC7 => {
            let mut pre_ops = Vec::new();
            let modrm_size = if matches!(op, 0x88 | 0x8A | 0xC6) { 1 } else { size };
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
                0x88 | 0x89 => {
                    let src = x86_reg(decoded.reg_index, modrm_size);
                    write_rm_value(&decoded.rm, src, address, &mut ops, &mut seq, "MOV")
                }
                0x8A | 0x8B => {
                    let src = materialize_rm_value(
                        &decoded.rm,
                        modrm_size,
                        address,
                        &mut ops,
                        &mut temp,
                        &mut seq,
                    );
                    ops.push(PcodeOp {
                        seq_num: next_seq(&mut seq),
                        opcode: PcodeOpcode::Copy,
                        address,
                        output: Some(x86_reg(decoded.reg_index, modrm_size)),
                        inputs: vec![src],
                        asm_mnemonic: Some("MOV_WRITE".to_string()),
                    });
                    ops
                }
                0x8D => {
                    let addr_vn = match decoded.rm {
                        RmOperand::Mem(addr_vn) => addr_vn,
                        RmOperand::Reg(_) => return Vec::new(),
                    };
                    let src = if size == 8 {
                        addr_vn
                    } else {
                        let truncated = temp.alloc(size);
                        ops.push(PcodeOp {
                            seq_num: next_seq(&mut seq),
                            opcode: PcodeOpcode::SubPiece,
                            address,
                            output: Some(truncated.clone()),
                            inputs: vec![addr_vn, const_u64(0, 4)],
                            asm_mnemonic: Some("LEA_TRUNC".to_string()),
                        });
                        truncated
                    };
                    ops.push(PcodeOp {
                        seq_num: next_seq(&mut seq),
                        opcode: PcodeOpcode::Copy,
                        address,
                        output: Some(x86_reg(decoded.reg_index, size)),
                        inputs: vec![src],
                        asm_mnemonic: Some("LEA_WRITE".to_string()),
                    });
                    ops
                }
                0xC6 | 0xC7 => {
                    if decoded.reg_field != 0 {
                        return Vec::new();
                    }
                    let imm_size = if op == 0xC6 { 1 } else { size };
                    let imm_bytes = if op == 0xC6 {
                        1
                    } else {
                        immediate_bytes_for_operand(size)
                    };
                    let imm = match decode_immediate(
                        insn,
                        decoded.next_idx,
                        imm_bytes,
                        imm_size,
                        op == 0xC7 && size == 8,
                    ) {
                        Some(v) => v,
                        None => return Vec::new(),
                    };
                    write_rm_value(&decoded.rm, imm, address, &mut ops, &mut seq, "MOV_IMM")
                }
                _ => Vec::new(),
            }
        }
        0xB0..=0xB7 => {
            let reg = u32::from(op - 0xB0) + rex_b(&prefix);
            let imm = match decode_immediate(insn, op_idx + 1, 1, 1, false) {
                Some(v) => v,
                None => return Vec::new(),
            };
            vec![PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(x86_reg(reg, 1)),
                inputs: vec![imm],
                asm_mnemonic: Some("MOV_IMM_WRITE".to_string()),
            }]
        }
        0xB8..=0xBF => {
            let reg = u32::from(op - 0xB8) + rex_b(&prefix);
            let imm_bytes = if size == 8 {
                8
            } else {
                immediate_bytes_for_operand(size)
            };
            let imm = match decode_immediate(insn, op_idx + 1, imm_bytes, size, false) {
                Some(v) => v,
                None => return Vec::new(),
            };
            vec![PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(x86_reg(reg, size)),
                inputs: vec![imm],
                asm_mnemonic: Some("MOV_IMM_WRITE".to_string()),
            }]
        }
        0x69 | 0x6B => decode_imul_r_rm_imm(
            insn,
            op_idx,
            &prefix,
            size,
            address,
            &mut temp,
            &mut seq,
            op == 0x6B,
        ),
        0x01 | 0x03 | 0x09 | 0x0B | 0x11 | 0x13 | 0x19 | 0x1B | 0x21 | 0x23 | 0x29 | 0x2B
        | 0x31 | 0x33 | 0x39 | 0x3B | 0x81 | 0x83 | 0x85 | 0xF6 | 0xF7 | 0xFF | 0xC0 | 0xD0 | 0xD1
        | 0xD2 | 0xC1 | 0xD3 => {
            let mut pre_ops = Vec::new();
            let ff_group = if op == 0xFF {
                insn.get(op_idx + 1).map(|b| (b >> 3) & 0x7)
            } else {
                None
            };
            let modrm_size = if matches!(op, 0xC0 | 0xD0 | 0xD2 | 0xF6) {
                1
            } else if ff_group == Some(6) {
                stack_operand_size(&prefix)
            } else {
                size
            };
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
                0xF6 | 0xF7 => {
                    let group_size = if op == 0xF6 { 1 } else { size };
                    if decoded.reg_field == 0 {
                        let rhs = match decode_immediate(
                            insn,
                            decoded.next_idx,
                            if op == 0xF6 {
                                1
                            } else {
                                immediate_bytes_for_operand(group_size)
                            },
                            group_size,
                            op == 0xF7 && group_size == 8,
                        ) {
                            Some(v) => v,
                            None => return Vec::new(),
                        };
                        let lhs = materialize_rm_value(
                            &decoded.rm,
                            group_size,
                            address,
                            &mut ops,
                            &mut temp,
                            &mut seq,
                        );
                        ops.extend(emit_alu_ops(
                            address,
                            group_size,
                            lhs,
                            rhs,
                            Destination::None,
                            AluKind::Test,
                            &mut temp,
                            &mut seq,
                        ));
                    } else if decoded.reg_field == 3 {
                        let lhs = materialize_rm_value(
                            &decoded.rm,
                            group_size,
                            address,
                            &mut ops,
                            &mut temp,
                            &mut seq,
                        );
                        ops.extend(emit_alu_ops(
                            address,
                            group_size,
                            lhs,
                            const_u64(0, group_size),
                            destination_from_rm(&decoded.rm),
                            AluKind::Neg,
                            &mut temp,
                            &mut seq,
                        ));
                    } else if decoded.reg_field == 4 {
                        emit_mul_one_operand(
                            &decoded.rm,
                            group_size,
                            false,
                            address,
                            &mut ops,
                            &mut temp,
                            &mut seq,
                        );
                    } else if decoded.reg_field == 5 {
                        emit_mul_one_operand(
                            &decoded.rm,
                            group_size,
                            true,
                            address,
                            &mut ops,
                            &mut temp,
                            &mut seq,
                        );
                    } else if decoded.reg_field == 6 {
                        emit_div_one_operand(
                            &decoded.rm,
                            group_size,
                            false,
                            address,
                            &mut ops,
                            &mut temp,
                            &mut seq,
                        );
                    } else if decoded.reg_field == 7 {
                        emit_div_one_operand(
                            &decoded.rm,
                            group_size,
                            true,
                            address,
                            &mut ops,
                            &mut temp,
                            &mut seq,
                        );
                    } else {
                        return Vec::new();
                    }
                    ops
                }
                0xFF => {
                    if decoded.reg_field == 6 {
                        let slot_size = stack_operand_size(&prefix);
                        let src = materialize_rm_value(
                            &decoded.rm,
                            slot_size,
                            address,
                            &mut ops,
                            &mut temp,
                            &mut seq,
                        );
                        emit_stack_push(
                            address,
                            src,
                            slot_size,
                            &mut ops,
                            &mut temp,
                            &mut seq,
                            "PUSH_RM",
                        );
                    } else {
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
                    }
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
mod ext;
#[cfg(test)]
mod tests;

use self::alu::emit_alu_ops;
use self::addressing::{
    decode_immediate, decode_modrm_operand, immediate_bytes_for_operand, operand_size, parse_prefixes,
};
use self::ext::{decode_extended_semantic, decode_imul_r_rm_imm, emit_div_one_operand, emit_mul_one_operand};

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

fn write_rm_value(
    rm: &RmOperand,
    value: Varnode,
    address: u64,
    ops: &mut Vec<PcodeOp>,
    seq: &mut u32,
    mnemonic_prefix: &str,
) -> Vec<PcodeOp> {
    match rm {
        RmOperand::Reg(dst) => {
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(dst.clone()),
                inputs: vec![value],
                asm_mnemonic: Some(format!("{mnemonic_prefix}_WRITE")),
            });
        }
        RmOperand::Mem(addr_vn) => {
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Store,
                address,
                output: None,
                inputs: vec![const_u64(RAM_SPACE_ID, 8), addr_vn.clone(), value],
                asm_mnemonic: Some(format!("{mnemonic_prefix}_STORE")),
            });
        }
    }

    std::mem::take(ops)
}

fn stack_operand_size(prefix: &PrefixState) -> u32 {
    if prefix.operand_size_override {
        2
    } else {
        8
    }
}

fn stack_pointer_reg() -> Varnode {
    x86_reg(4, 8)
}

fn emit_stack_push(
    address: u64,
    value: Varnode,
    slot_size: u32,
    ops: &mut Vec<PcodeOp>,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    mnemonic_prefix: &str,
) {
    let sp = stack_pointer_reg();
    let sp_next = temp.alloc(8);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSub,
        address,
        output: Some(sp_next.clone()),
        inputs: vec![sp.clone(), const_u64(u64::from(slot_size), 8)],
        asm_mnemonic: Some(format!("{mnemonic_prefix}_SP_SUB")),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(sp),
        inputs: vec![sp_next.clone()],
        asm_mnemonic: Some(format!("{mnemonic_prefix}_SP_WRITE")),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Store,
        address,
        output: None,
        inputs: vec![const_u64(RAM_SPACE_ID, 8), sp_next, value],
        asm_mnemonic: Some(format!("{mnemonic_prefix}_STORE")),
    });
}

fn emit_stack_pop(
    address: u64,
    slot_size: u32,
    ops: &mut Vec<PcodeOp>,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    mnemonic_prefix: &str,
) -> Varnode {
    let sp = stack_pointer_reg();
    let popped = temp.alloc(slot_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Load,
        address,
        output: Some(popped.clone()),
        inputs: vec![const_u64(RAM_SPACE_ID, 8), sp.clone()],
        asm_mnemonic: Some(format!("{mnemonic_prefix}_LOAD")),
    });
    let sp_next = temp.alloc(8);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAdd,
        address,
        output: Some(sp_next.clone()),
        inputs: vec![sp.clone(), const_u64(u64::from(slot_size), 8)],
        asm_mnemonic: Some(format!("{mnemonic_prefix}_SP_ADD")),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(sp),
        inputs: vec![sp_next],
        asm_mnemonic: Some(format!("{mnemonic_prefix}_SP_WRITE")),
    });
    popped
}

fn rex_b(prefix: &PrefixState) -> u32 {
    if (prefix.rex & 0x01) != 0 {
        8
    } else {
        0
    }
}

