use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use super::super::common::{
    const_u64, x86_flag_cf, x86_flag_df, x86_flag_of, x86_flag_pf, x86_flag_sf, x86_flag_zf,
    x86_reg, x86_xmm_reg,
    RAM_SPACE_ID, UNIQUE_SPACE_ID,
};

#[derive(Debug, Clone, Copy)]
struct PrefixState {
    operand_size_override: bool,
    address_size_override: bool,
    rex: u8,
    rep_prefix: Option<RepPrefix>,
    segment_override: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RepPrefix {
    Rep,
    Repne,
}

#[derive(Debug, Clone)]
struct X86TempFactory {
    next: u64,
}

const X86_DIV_EXCEPTION_POLICY_ID: u64 = 0xF706;
const X86_IDIV_EXCEPTION_POLICY_ID: u64 = 0xF707;
const X86_NOP_HINT_ID: u64 = 0x90;
const X86_PAUSE_HINT_ID: u64 = 0xF390;
const X86_INT3_TRAP_ID: u64 = 0xCC;
const X86_INT_IMM_TRAP_ID: u64 = 0xCD;
const X86_ROTATE_INTRINSIC_BASE_ID: u64 = 0xF0D0;

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
    #[cfg(test)]
    fn base_for_address(address: u64) -> u64 {
        0xE100_0000_0000_0000u64.wrapping_add(address.wrapping_shl(6))
    }

    fn with_base(base: u64) -> Self {
        Self { next: base }
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

#[cfg(test)]
pub(crate) fn decode_semantic(insn: &[u8], address: u64) -> Vec<PcodeOp> {
    decode_semantic_with_state(insn, address, 1, X86TempFactory::base_for_address(address))
}

pub(crate) fn decode_semantic_with_state(
    insn: &[u8],
    address: u64,
    seq_start: u32,
    temp_base: u64,
) -> Vec<PcodeOp> {
    if insn.is_empty() {
        return Vec::new();
    }

    let (op_idx, prefix) = parse_prefixes(insn);
    if op_idx >= insn.len() {
        return Vec::new();
    }

    let op = insn[op_idx];
    let size = operand_size(&prefix);
    let mut seq = seq_start;
    let mut temp = X86TempFactory::with_base(temp_base);

    if op == 0x0F {
        return decode_extended_semantic(insn, op_idx, &prefix, size, address, &mut temp, &mut seq);
    }

    match op {
        0xD8..=0xDF => {
            self::ext::decode_x87_policy(insn, op_idx, &prefix, address, &mut temp, &mut seq, op - 0xD8)
        }
        0xCC => {
            vec![PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::CallOther,
                address,
                output: None,
                inputs: vec![const_u64(X86_INT3_TRAP_ID, 8)],
                asm_mnemonic: Some("INT3_TRAP".to_string()),
            }]
        }
        0xCD => {
            let vector = match decode_immediate(insn, op_idx + 1, 1, 1, false) {
                Some(v) => v,
                None => return Vec::new(),
            };
            vec![PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::CallOther,
                address,
                output: None,
                inputs: vec![const_u64(X86_INT_IMM_TRAP_ID, 8), vector],
                asm_mnemonic: Some("INT_IMM_TRAP".to_string()),
            }]
        }
        0x90 => {
            let (hint_id, mnemonic) = if prefix.rep_prefix == Some(RepPrefix::Rep) {
                (X86_PAUSE_HINT_ID, "PAUSE_HINT")
            } else {
                (X86_NOP_HINT_ID, "NOP_HINT")
            };
            let hint = temp.alloc(8);
            vec![PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(hint),
                inputs: vec![const_u64(hint_id, 8)],
                asm_mnemonic: Some(mnemonic.to_string()),
            }]
        }
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
        0xA8 => {
            let imm = match decode_immediate(insn, op_idx + 1, 1, 1, false) {
                Some(v) => v,
                None => return Vec::new(),
            };
            emit_alu_ops(
                address,
                1,
                x86_reg(0, 1),
                imm,
                Destination::None,
                AluKind::Test,
                &mut temp,
                &mut seq,
            )
        }
        0xA4 | 0xA5 | 0xA6 | 0xA7 | 0xAA | 0xAB | 0xAC | 0xAD | 0xAE | 0xAF => {
            decode_string_semantic(op, &prefix, size, address, &mut temp, &mut seq)
        }
        0x98 => decode_op_98_sign_extend_accumulator(size, address, &mut temp, &mut seq),
        0x99 => decode_op_99_sign_extend_high_half(size, address, &mut temp, &mut seq),
        0x04 | 0x05 | 0x0C | 0x0D | 0x14 | 0x15 | 0x1C | 0x1D | 0x24 | 0x25 | 0x2C | 0x2D
        | 0x34 | 0x35 | 0x3C | 0x3D => {
            let is_byte = matches!(
                op,
                0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x34 | 0x3C
            );
            let op_size = if is_byte { 1 } else { size };
            let imm = match decode_immediate(
                insn,
                op_idx + 1,
                if is_byte { 1 } else { immediate_bytes_for_operand(op_size) },
                op_size,
                !is_byte && op_size == 8,
            ) {
                Some(v) => v,
                None => return Vec::new(),
            };

            let kind = match op {
                0x04 | 0x05 => AluKind::Add,
                0x0C | 0x0D => AluKind::Or,
                0x14 | 0x15 => AluKind::Adc,
                0x1C | 0x1D => AluKind::Sbb,
                0x24 | 0x25 => AluKind::And,
                0x2C | 0x2D => AluKind::Sub,
                0x34 | 0x35 => AluKind::Xor,
                0x3C | 0x3D => AluKind::Cmp,
                _ => return Vec::new(),
            };

            let dst = if kind == AluKind::Cmp {
                Destination::None
            } else {
                Destination::Reg(x86_reg(0, op_size))
            };

            emit_alu_ops(
                address,
                op_size,
                x86_reg(0, op_size),
                imm,
                dst,
                kind,
                &mut temp,
                &mut seq,
            )
        }
        0x86 | 0x87 | 0x88 | 0x89 | 0x8A | 0x8B | 0x8D | 0xC6 | 0xC7 => {
            let mut pre_ops = Vec::new();
            let modrm_size = if matches!(op, 0x86 | 0x88 | 0x8A | 0xC6) {
                1
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
                0x86 | 0x87 => {
                    let reg = x86_reg(decoded.reg_index, modrm_size);
                    let rm_value =
                        materialize_rm_value(&decoded.rm, modrm_size, address, &mut ops, &mut temp, &mut seq);
                    let reg_saved = temp.alloc(modrm_size);
                    ops.push(PcodeOp {
                        seq_num: next_seq(&mut seq),
                        opcode: PcodeOpcode::Copy,
                        address,
                        output: Some(reg_saved.clone()),
                        inputs: vec![reg.clone()],
                        asm_mnemonic: Some("XCHG_REG_SAVE".to_string()),
                    });
                    ops.push(PcodeOp {
                        seq_num: next_seq(&mut seq),
                        opcode: PcodeOpcode::Copy,
                        address,
                        output: Some(reg),
                        inputs: vec![rm_value],
                        asm_mnemonic: Some("XCHG_REG_WRITE".to_string()),
                    });
                    write_rm_value(&decoded.rm, reg_saved, address, &mut ops, &mut seq, "XCHG")
                }
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
                    let group_size = if op == 0xF6 { 1 } else { size };
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
                    } else if decoded.reg_field == 2 || decoded.reg_field == 4 || decoded.reg_field == 3 || decoded.reg_field == 5 {
                        // CALL/JMP indirect (near/far)
                        let target = materialize_rm_value(
                            &decoded.rm,
                            group_size,
                            address,
                            &mut ops,
                            &mut temp,
                            &mut seq,
                        );
                        if decoded.reg_field == 2 || decoded.reg_field == 3 {
                            // CALL: push return address
                            let slot_size = stack_operand_size(&prefix);
                            let ret_addr = const_u64(address + insn.len() as u64, slot_size);
                            emit_stack_push(
                                address,
                                ret_addr,
                                slot_size,
                                &mut ops,
                                &mut temp,
                                &mut seq,
                                "CALL_RET",
                            );
                            ops.push(PcodeOp {
                                seq_num: next_seq(&mut seq),
                                opcode: PcodeOpcode::CallInd,
                                address,
                                output: None,
                                inputs: vec![target],
                                asm_mnemonic: Some("CALL_IND".to_string()),
                            });
                        } else {
                            // JMP near indirect
                            ops.push(PcodeOp {
                                seq_num: next_seq(&mut seq),
                                opcode: PcodeOpcode::BranchInd,
                                address,
                                output: None,
                                inputs: vec![target],
                                asm_mnemonic: Some("JMP_IND".to_string()),
                            });
                        }
                    } else {
                        let kind = match decoded.reg_field {
                            0 => AluKind::Inc,
                            1 => AluKind::Dec,
                            _ => return Vec::new(),
                        };
                        let lhs = materialize_rm_value(&decoded.rm, group_size, address, &mut ops, &mut temp, &mut seq);
                        ops.extend(emit_alu_ops(
                            address,
                            group_size,
                            lhs,
                            const_u64(1, group_size),
                            destination_from_rm(&decoded.rm),
                            kind,
                            &mut temp,
                            &mut seq,
                        ));
                    }
                    ops
                }
                0xC0 | 0xD0 | 0xD1 | 0xD2 | 0xC1 | 0xD3 => {
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
                    match decoded.reg_field {
                        0 | 1 => {
                            ops.extend(emit_rotate_intrinsic_ops(
                                address,
                                shift_size,
                                lhs,
                                count,
                                destination_from_rm(&decoded.rm),
                                decoded.reg_field == 0,
                                &mut temp,
                                &mut seq,
                            ));
                        }
                        4 | 5 | 7 => {
                            let kind = match decoded.reg_field {
                                4 => AluKind::Shl,
                                5 => AluKind::Shr,
                                7 => AluKind::Sar,
                                _ => unreachable!("checked above"),
                            };
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
                        }
                        _ => return Vec::new(),
                    }
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

fn decode_op_98_sign_extend_accumulator(
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let src_size = match size {
        2 => 1,
        4 => 2,
        8 => 4,
        _ => return Vec::new(),
    };

    let src = x86_reg(0, src_size);
    let mut ops = Vec::new();
    if src_size == size {
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::Copy,
            address,
            output: Some(x86_reg(0, size)),
            inputs: vec![src],
            asm_mnemonic: Some("CBW_CWDE_CDQE_WRITE".to_string()),
        });
        return ops;
    }

    let extended = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSExt,
        address,
        output: Some(extended.clone()),
        inputs: vec![src],
        asm_mnemonic: Some("CBW_CWDE_CDQE_SEXT".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_reg(0, size)),
        inputs: vec![extended],
        asm_mnemonic: Some("CBW_CWDE_CDQE_WRITE".to_string()),
    });
    ops
}

fn decode_op_99_sign_extend_high_half(
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let src = x86_reg(0, size);
    let wide_size = size.saturating_mul(2);
    if wide_size == 0 {
        return Vec::new();
    }

    let mut ops = Vec::new();
    let extended = temp.alloc(wide_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSExt,
        address,
        output: Some(extended.clone()),
        inputs: vec![src],
        asm_mnemonic: Some("CWD_CDQ_CQO_SEXT".to_string()),
    });

    let high = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::SubPiece,
        address,
        output: Some(high.clone()),
        inputs: vec![extended, const_u64(u64::from(size), 4)],
        asm_mnemonic: Some("CWD_CDQ_CQO_HIGH".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_reg(2, size)),
        inputs: vec![high],
        asm_mnemonic: Some("CWD_CDQ_CQO_WRITE".to_string()),
    });
    ops
}

fn emit_rotate_intrinsic_ops(
    address: u64,
    size: u32,
    lhs: Varnode,
    count: Varnode,
    dst: Destination,
    is_left: bool,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let count_mask = if size == 8 { 0x3F } else { 0x1F };
    let count_input = if count.is_constant {
        let masked = (count.constant_val as u64) & count_mask;
        if masked == 0 {
            return Vec::new();
        }
        const_u64(masked, size)
    } else if count.size == size {
        count
    } else {
        let mut ops = Vec::new();
        let ext = temp.alloc(size);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntZExt,
            address,
            output: Some(ext.clone()),
            inputs: vec![count],
            asm_mnemonic: Some("ROT_COUNT_ZEXT".to_string()),
        });
        let result = match &dst {
            Destination::Reg(v) => v.clone(),
            Destination::Mem(_) | Destination::None => temp.alloc(size),
        };
        let policy_id = X86_ROTATE_INTRINSIC_BASE_ID + if is_left { 0 } else { 1 };
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: Some(result.clone()),
            inputs: vec![const_u64(policy_id, 8), lhs, ext],
            asm_mnemonic: Some(if is_left {
                "ROL_INTRINSIC".to_string()
            } else {
                "ROR_INTRINSIC".to_string()
            }),
        });
        if let Destination::Mem(addr_vn) = dst {
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Store,
                address,
                output: None,
                inputs: vec![const_u64(RAM_SPACE_ID, 8), addr_vn, result],
                asm_mnemonic: Some("ROT_STORE".to_string()),
            });
        }
        return ops;
    };

    let mut ops = Vec::new();
    let result = match &dst {
        Destination::Reg(v) => v.clone(),
        Destination::Mem(_) | Destination::None => temp.alloc(size),
    };
    let policy_id = X86_ROTATE_INTRINSIC_BASE_ID + if is_left { 0 } else { 1 };
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: Some(result.clone()),
        inputs: vec![const_u64(policy_id, 8), lhs, count_input],
        asm_mnemonic: Some(if is_left {
            "ROL_INTRINSIC".to_string()
        } else {
            "ROR_INTRINSIC".to_string()
        }),
    });
    if let Destination::Mem(addr_vn) = dst {
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::Store,
            address,
            output: None,
            inputs: vec![const_u64(RAM_SPACE_ID, 8), addr_vn, result],
            asm_mnemonic: Some("ROT_STORE".to_string()),
        });
    }
    ops
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

fn decode_string_semantic(
    op: u8,
    prefix: &PrefixState,
    operand_size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let data_size = match string_data_size(op, operand_size) {
        Some(v) => v,
        None => return Vec::new(),
    };
    let index_size = string_index_size(prefix);
    let step = u64::from(data_size);
    let mut ops = Vec::new();

    match op {
        0xA4 | 0xA5 => {
            let src_idx = x86_reg(6, index_size);
            let dst_idx = x86_reg(7, index_size);
            let src_addr = widen_index_to_address(src_idx, address, &mut ops, temp, seq, "MOVS_SRC");
            let dst_addr = widen_index_to_address(dst_idx, address, &mut ops, temp, seq, "MOVS_DST");
            let loaded = temp.alloc(data_size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Load,
                address,
                output: Some(loaded.clone()),
                inputs: vec![const_u64(RAM_SPACE_ID, 8), src_addr],
                asm_mnemonic: Some("MOVS_LOAD".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Store,
                address,
                output: None,
                inputs: vec![const_u64(RAM_SPACE_ID, 8), dst_addr, loaded],
                asm_mnemonic: Some("MOVS_STORE".to_string()),
            });
            emit_df_index_update(6, index_size, step, address, &mut ops, temp, seq, "MOVS_SRC");
            emit_df_index_update(7, index_size, step, address, &mut ops, temp, seq, "MOVS_DST");
        }
        0xA6 | 0xA7 => {
            let src_idx = x86_reg(6, index_size);
            let dst_idx = x86_reg(7, index_size);
            let src_addr = widen_index_to_address(src_idx, address, &mut ops, temp, seq, "CMPS_SRC");
            let dst_addr = widen_index_to_address(dst_idx, address, &mut ops, temp, seq, "CMPS_DST");
            let lhs = temp.alloc(data_size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Load,
                address,
                output: Some(lhs.clone()),
                inputs: vec![const_u64(RAM_SPACE_ID, 8), src_addr],
                asm_mnemonic: Some("CMPS_LOAD_LHS".to_string()),
            });
            let rhs = temp.alloc(data_size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Load,
                address,
                output: Some(rhs.clone()),
                inputs: vec![const_u64(RAM_SPACE_ID, 8), dst_addr],
                asm_mnemonic: Some("CMPS_LOAD_RHS".to_string()),
            });
            ops.extend(emit_alu_ops(
                address,
                data_size,
                lhs,
                rhs,
                Destination::None,
                AluKind::Cmp,
                temp,
                seq,
            ));
            emit_df_index_update(6, index_size, step, address, &mut ops, temp, seq, "CMPS_SRC");
            emit_df_index_update(7, index_size, step, address, &mut ops, temp, seq, "CMPS_DST");
        }
        0xAA | 0xAB => {
            let dst_idx = x86_reg(7, index_size);
            let dst_addr = widen_index_to_address(dst_idx, address, &mut ops, temp, seq, "STOS_DST");
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Store,
                address,
                output: None,
                inputs: vec![const_u64(RAM_SPACE_ID, 8), dst_addr, x86_reg(0, data_size)],
                asm_mnemonic: Some("STOS_STORE".to_string()),
            });
            emit_df_index_update(7, index_size, step, address, &mut ops, temp, seq, "STOS_DST");
        }
        0xAC | 0xAD => {
            let src_idx = x86_reg(6, index_size);
            let src_addr = widen_index_to_address(src_idx, address, &mut ops, temp, seq, "LODS_SRC");
            let loaded = temp.alloc(data_size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Load,
                address,
                output: Some(loaded.clone()),
                inputs: vec![const_u64(RAM_SPACE_ID, 8), src_addr],
                asm_mnemonic: Some("LODS_LOAD".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(x86_reg(0, data_size)),
                inputs: vec![loaded],
                asm_mnemonic: Some("LODS_WRITE".to_string()),
            });
            emit_df_index_update(6, index_size, step, address, &mut ops, temp, seq, "LODS_SRC");
        }
        0xAE | 0xAF => {
            let dst_idx = x86_reg(7, index_size);
            let dst_addr = widen_index_to_address(dst_idx, address, &mut ops, temp, seq, "SCAS_DST");
            let rhs = temp.alloc(data_size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Load,
                address,
                output: Some(rhs.clone()),
                inputs: vec![const_u64(RAM_SPACE_ID, 8), dst_addr],
                asm_mnemonic: Some("SCAS_LOAD".to_string()),
            });
            ops.extend(emit_alu_ops(
                address,
                data_size,
                x86_reg(0, data_size),
                rhs,
                Destination::None,
                AluKind::Cmp,
                temp,
                seq,
            ));
            emit_df_index_update(7, index_size, step, address, &mut ops, temp, seq, "SCAS_DST");
        }
        _ => return Vec::new(),
    }

    emit_rep_count_step(prefix, index_size, address, &mut ops, temp, seq);
    ops
}

fn string_data_size(op: u8, operand_size: u32) -> Option<u32> {
    match op {
        0xA4 | 0xA6 | 0xAA | 0xAC | 0xAE => Some(1),
        0xA5 | 0xA7 | 0xAB | 0xAD | 0xAF => Some(operand_size),
        _ => None,
    }
}

fn string_index_size(prefix: &PrefixState) -> u32 {
    if prefix.address_size_override {
        4
    } else {
        8
    }
}

fn widen_index_to_address(
    index: Varnode,
    address: u64,
    ops: &mut Vec<PcodeOp>,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    tag: &str,
) -> Varnode {
    if index.size == 8 {
        return index;
    }
    let out = temp.alloc(8);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntZExt,
        address,
        output: Some(out.clone()),
        inputs: vec![index],
        asm_mnemonic: Some(format!("{tag}_ADDR_ZEXT")),
    });
    out
}

fn emit_df_index_update(
    reg_index: u32,
    reg_size: u32,
    step: u64,
    address: u64,
    ops: &mut Vec<PcodeOp>,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    tag: &str,
) {
    let reg = x86_reg(reg_index, reg_size);
    let plus = temp.alloc(reg_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAdd,
        address,
        output: Some(plus.clone()),
        inputs: vec![reg.clone(), const_u64(step, reg_size)],
        asm_mnemonic: Some(format!("{tag}_STEP_ADD")),
    });
    let minus = temp.alloc(reg_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSub,
        address,
        output: Some(minus.clone()),
        inputs: vec![reg.clone(), const_u64(step, reg_size)],
        asm_mnemonic: Some(format!("{tag}_STEP_SUB")),
    });

    let selected = emit_select_with_flag(
        x86_flag_df(),
        minus,
        plus,
        reg_size,
        address,
        ops,
        temp,
        seq,
        tag,
    );
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(reg),
        inputs: vec![selected],
        asm_mnemonic: Some(format!("{tag}_WRITE")),
    });
}

fn emit_select_with_flag(
    cond: Varnode,
    when_true: Varnode,
    when_false: Varnode,
    out_size: u32,
    address: u64,
    ops: &mut Vec<PcodeOp>,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    tag: &str,
) -> Varnode {
    let cond_ext = if out_size == 1 {
        cond
    } else {
        let out = temp.alloc(out_size);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntZExt,
            address,
            output: Some(out.clone()),
            inputs: vec![cond],
            asm_mnemonic: Some(format!("{tag}_COND_ZEXT")),
        });
        out
    };

    let mask = temp.alloc(out_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSub,
        address,
        output: Some(mask.clone()),
        inputs: vec![const_u64(0, out_size), cond_ext],
        asm_mnemonic: Some(format!("{tag}_COND_MASK")),
    });

    let inv_mask = temp.alloc(out_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntNegate,
        address,
        output: Some(inv_mask.clone()),
        inputs: vec![mask.clone()],
        asm_mnemonic: Some(format!("{tag}_COND_NMASK")),
    });

    let true_part = temp.alloc(out_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(true_part.clone()),
        inputs: vec![when_true, mask],
        asm_mnemonic: Some(format!("{tag}_COND_TRUE")),
    });

    let false_part = temp.alloc(out_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(false_part.clone()),
        inputs: vec![when_false, inv_mask],
        asm_mnemonic: Some(format!("{tag}_COND_FALSE")),
    });

    let merged = temp.alloc(out_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntOr,
        address,
        output: Some(merged.clone()),
        inputs: vec![true_part, false_part],
        asm_mnemonic: Some(format!("{tag}_COND_MERGE")),
    });
    merged
}

fn emit_rep_count_step(
    prefix: &PrefixState,
    count_size: u32,
    address: u64,
    ops: &mut Vec<PcodeOp>,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) {
    let mnemonic_base = match prefix.rep_prefix {
        Some(RepPrefix::Rep) => "REP_COUNT",
        Some(RepPrefix::Repne) => "REPNE_COUNT",
        None => return,
    };
    let count = x86_reg(1, count_size);
    let dec = temp.alloc(count_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSub,
        address,
        output: Some(dec.clone()),
        inputs: vec![count.clone(), const_u64(1, count_size)],
        asm_mnemonic: Some(format!("{mnemonic_base}_DEC")),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(count),
        inputs: vec![dec],
        asm_mnemonic: Some(format!("{mnemonic_base}_WRITE")),
    });
}

