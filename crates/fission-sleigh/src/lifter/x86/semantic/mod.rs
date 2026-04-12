use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use super::common::{
    const_u64, x86_flag_af, x86_flag_cf, x86_flag_df, x86_flag_if, x86_flag_of, x86_flag_pf,
    x86_flag_sf, x86_flag_zf, x86_mxcsr, x86_reg, x86_seg, x86_xmm_reg, x86_ymm_reg,
    X86TempFactory, RAM_SPACE_ID,
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

const X86_DIV_EXCEPTION_POLICY_ID: u64 = 0xF706;
const X86_IDIV_EXCEPTION_POLICY_ID: u64 = 0xF707;
const X86_NOP_HINT_ID: u64 = 0x90;
const X86_PAUSE_HINT_ID: u64 = 0xF390;
const X86_INT3_TRAP_ID: u64 = 0xCC;
const X86_INT_IMM_TRAP_ID: u64 = 0xCD;
const X86_IN_POLICY_ID: u64 = 0xE4_00;
const X86_OUT_POLICY_ID: u64 = 0xE6_00;
const X86_FAR_CALL_POLICY_ID: u64 = 0x9A_00;
const X86_FAR_JMP_POLICY_ID: u64 = 0xEA_00;
const X86_HLT_POLICY_ID: u64 = 0xF4_00;
const X86_DAA_POLICY_ID: u64 = 0x27_00;
const X86_DAS_POLICY_ID: u64 = 0x2F_00;
const X86_AAA_POLICY_ID: u64 = 0x37_00;
const X86_AAS_POLICY_ID: u64 = 0x3F_00;
const X86_INS_POLICY_ID: u64 = 0x6C_00;
const X86_OUTS_POLICY_ID: u64 = 0x6E_00;
const X86_WAIT_POLICY_ID: u64 = 0x9B_00;
const X86_IRET_POLICY_ID: u64 = 0xCF_00;
const X86_INTO_POLICY_ID: u64 = 0xCE_00;
const X86_INT1_POLICY_ID: u64 = 0xF1_00;

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

    // VEX-encoded AVX/AVX2 instructions: route through existing SSE/3-byte decoders.
    if op == 0xC5 || op == 0xC4 {
        return decode_vex_semantic(insn, op_idx, &prefix, size, address, &mut temp, &mut seq);
    }

    match op {
        0xD8..=0xDF => self::ext::decode_x87_policy(
            insn,
            op_idx,
            &prefix,
            address,
            &mut temp,
            &mut seq,
            op - 0xD8,
        ),
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
                address, src, slot_size, &mut ops, &mut temp, &mut seq, "PUSH_REG",
            );
            ops
        }
        0x58..=0x5F => {
            let slot_size = stack_operand_size(&prefix);
            let reg = u32::from(op - 0x58) + rex_b(&prefix);
            let mut ops = Vec::new();
            let popped =
                emit_stack_pop(address, slot_size, &mut ops, &mut temp, &mut seq, "POP_REG");
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
            let imm =
                match decode_immediate(insn, op_idx + 1, imm_bytes, slot_size, imm_sign_extend) {
                    Some(v) => v,
                    None => return Vec::new(),
                };
            let mut ops = Vec::new();
            emit_stack_push(
                address, imm, slot_size, &mut ops, &mut temp, &mut seq, "PUSH_IMM",
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
            let popped =
                emit_stack_pop(address, slot_size, &mut ops, &mut temp, &mut seq, "POP_RM");
            write_rm_value(&decoded.rm, popped, address, &mut ops, &mut seq, "POP")
        }
        0xE8 => {
            let slot_size = stack_operand_size(&prefix);
            let ret_addr = const_u64(address.wrapping_add(insn.len() as u64), slot_size);
            let mut ops = Vec::new();
            emit_stack_push(
                address, ret_addr, slot_size, &mut ops, &mut temp, &mut seq, "CALL",
            );
            ops
        }
        0xC3 | 0xCB | 0xC2 | 0xCA => {
            let slot_size = stack_operand_size(&prefix);
            let mut ops = Vec::new();
            let _ret_addr =
                emit_stack_pop(address, slot_size, &mut ops, &mut temp, &mut seq, "RET");

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
            let imm = match decode_immediate(
                insn,
                op_idx + 1,
                immediate_bytes_for_operand(size),
                size,
                size == 8,
            ) {
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
            let is_byte = matches!(op, 0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x34 | 0x3C);
            let op_size = if is_byte { 1 } else { size };
            let imm = match decode_immediate(
                insn,
                op_idx + 1,
                if is_byte {
                    1
                } else {
                    immediate_bytes_for_operand(op_size)
                },
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
                    let rm_value = materialize_rm_value(
                        &decoded.rm,
                        modrm_size,
                        address,
                        &mut ops,
                        &mut temp,
                        &mut seq,
                    );
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
        // Byte-form ALU + 0x80/0x84/0xFE are merged into the same ModRM-parsing block.
        0x00 | 0x02 | 0x08 | 0x0A | 0x10 | 0x12 | 0x18 | 0x1A | 0x20 | 0x22 | 0x28 | 0x2A
        | 0x30 | 0x32 | 0x38 | 0x3A | 0x80 | 0x84 | 0xFE | 0x01 | 0x03 | 0x09 | 0x0B | 0x11
        | 0x13 | 0x19 | 0x1B | 0x21 | 0x23 | 0x29 | 0x2B | 0x31 | 0x33 | 0x39 | 0x3B | 0x81
        | 0x83 | 0x85 | 0xF6 | 0xF7 | 0xFF | 0xC0 | 0xD0 | 0xD1 | 0xD2 | 0xC1 | 0xD3 => {
            let mut pre_ops = Vec::new();
            let ff_group = if op == 0xFF {
                insn.get(op_idx + 1).map(|b| (b >> 3) & 0x7)
            } else {
                None
            };
            let modrm_size = if matches!(
                op,
                0xC0 | 0xD0
                    | 0xD2
                    | 0xF6
                    | 0x00
                    | 0x02
                    | 0x08
                    | 0x0A
                    | 0x10
                    | 0x12
                    | 0x18
                    | 0x1A
                    | 0x20
                    | 0x22
                    | 0x28
                    | 0x2A
                    | 0x30
                    | 0x32
                    | 0x38
                    | 0x3A
                    | 0x80
                    | 0x84
                    | 0xFE
            ) {
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
                    let lhs = materialize_rm_value(
                        &decoded.rm,
                        size,
                        address,
                        &mut ops,
                        &mut temp,
                        &mut seq,
                    );
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
                    let kind = if op == 0x11 {
                        AluKind::Adc
                    } else {
                        AluKind::Sbb
                    };
                    let lhs = materialize_rm_value(
                        &decoded.rm,
                        size,
                        address,
                        &mut ops,
                        &mut temp,
                        &mut seq,
                    );
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
                    let rhs = materialize_rm_value(
                        &decoded.rm,
                        size,
                        address,
                        &mut ops,
                        &mut temp,
                        &mut seq,
                    );
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
                    let kind = if op == 0x13 {
                        AluKind::Adc
                    } else {
                        AluKind::Sbb
                    };
                    let lhs = x86_reg(decoded.reg_index, size);
                    let rhs = materialize_rm_value(
                        &decoded.rm,
                        size,
                        address,
                        &mut ops,
                        &mut temp,
                        &mut seq,
                    );
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
                    let lhs = materialize_rm_value(
                        &decoded.rm,
                        size,
                        address,
                        &mut ops,
                        &mut temp,
                        &mut seq,
                    );
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
                    let rhs = materialize_rm_value(
                        &decoded.rm,
                        size,
                        address,
                        &mut ops,
                        &mut temp,
                        &mut seq,
                    );
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
                    let lhs = materialize_rm_value(
                        &decoded.rm,
                        size,
                        address,
                        &mut ops,
                        &mut temp,
                        &mut seq,
                    );
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
                    let lhs = materialize_rm_value(
                        &decoded.rm,
                        size,
                        address,
                        &mut ops,
                        &mut temp,
                        &mut seq,
                    );
                    let dst = if kind == AluKind::Cmp {
                        Destination::None
                    } else {
                        destination_from_rm(&decoded.rm)
                    };
                    ops.extend(emit_alu_ops(
                        address, size, lhs, rhs, dst, kind, &mut temp, &mut seq,
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
                    } else if decoded.reg_field == 2 {
                        // NOT: r/m = ~r/m — no flag updates
                        let lhs = materialize_rm_value(
                            &decoded.rm,
                            group_size,
                            address,
                            &mut ops,
                            &mut temp,
                            &mut seq,
                        );
                        let result = temp.alloc(group_size);
                        ops.push(PcodeOp {
                            seq_num: next_seq(&mut seq),
                            opcode: PcodeOpcode::IntNegate,
                            address,
                            output: Some(result.clone()),
                            inputs: vec![lhs],
                            asm_mnemonic: Some("NOT_RM".to_string()),
                        });
                        ops =
                            write_rm_value(&decoded.rm, result, address, &mut ops, &mut seq, "NOT");
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
                            address, src, slot_size, &mut ops, &mut temp, &mut seq, "PUSH_RM",
                        );
                    } else if decoded.reg_field == 2
                        || decoded.reg_field == 4
                        || decoded.reg_field == 3
                        || decoded.reg_field == 5
                    {
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
                                address, ret_addr, slot_size, &mut ops, &mut temp, &mut seq,
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
                        0 | 1 | 2 | 3 => {
                            // ROL(/0), ROR(/1), RCL(/2), RCR(/3)
                            ops.extend(emit_rotate_ops(
                                address,
                                shift_size,
                                lhs,
                                count,
                                destination_from_rm(&decoded.rm),
                                decoded.reg_field,
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
                // A1: Even byte-form opcodes — r/m8 is destination, r8 is source
                0x00 | 0x08 | 0x10 | 0x18 | 0x20 | 0x28 | 0x30 | 0x38 => {
                    let kind = match op {
                        0x00 => AluKind::Add,
                        0x08 => AluKind::Or,
                        0x10 => AluKind::Adc,
                        0x18 => AluKind::Sbb,
                        0x20 => AluKind::And,
                        0x28 => AluKind::Sub,
                        0x30 => AluKind::Xor,
                        _ => AluKind::Cmp, // 0x38
                    };
                    let lhs = materialize_rm_value(
                        &decoded.rm,
                        1,
                        address,
                        &mut ops,
                        &mut temp,
                        &mut seq,
                    );
                    let rhs = x86_reg(decoded.reg_index, 1);
                    let dst = if kind == AluKind::Cmp {
                        Destination::None
                    } else {
                        destination_from_rm(&decoded.rm)
                    };
                    ops.extend(emit_alu_ops(
                        address, 1, lhs, rhs, dst, kind, &mut temp, &mut seq,
                    ));
                    ops
                }
                // A2: Odd byte-form opcodes — r8 is destination, r/m8 is source
                0x02 | 0x0A | 0x12 | 0x1A | 0x22 | 0x2A | 0x32 | 0x3A => {
                    let kind = match op {
                        0x02 => AluKind::Add,
                        0x0A => AluKind::Or,
                        0x12 => AluKind::Adc,
                        0x1A => AluKind::Sbb,
                        0x22 => AluKind::And,
                        0x2A => AluKind::Sub,
                        0x32 => AluKind::Xor,
                        _ => AluKind::Cmp, // 0x3A
                    };
                    let lhs = x86_reg(decoded.reg_index, 1);
                    let rhs = materialize_rm_value(
                        &decoded.rm,
                        1,
                        address,
                        &mut ops,
                        &mut temp,
                        &mut seq,
                    );
                    let dst = if kind == AluKind::Cmp {
                        Destination::None
                    } else {
                        Destination::Reg(x86_reg(decoded.reg_index, 1))
                    };
                    ops.extend(emit_alu_ops(
                        address, 1, lhs, rhs, dst, kind, &mut temp, &mut seq,
                    ));
                    ops
                }
                // A3: 0x80 — ALU r/m8, imm8 (/0–/7)
                0x80 => {
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
                    let rhs = match decode_immediate(insn, decoded.next_idx, 1, 1, false) {
                        Some(v) => v,
                        None => return Vec::new(),
                    };
                    let lhs = materialize_rm_value(
                        &decoded.rm,
                        1,
                        address,
                        &mut ops,
                        &mut temp,
                        &mut seq,
                    );
                    let dst = if kind == AluKind::Cmp {
                        Destination::None
                    } else {
                        destination_from_rm(&decoded.rm)
                    };
                    ops.extend(emit_alu_ops(
                        address, 1, lhs, rhs, dst, kind, &mut temp, &mut seq,
                    ));
                    ops
                }
                // A4a: 0x84 — TEST r/m8, r8
                0x84 => {
                    let lhs = materialize_rm_value(
                        &decoded.rm,
                        1,
                        address,
                        &mut ops,
                        &mut temp,
                        &mut seq,
                    );
                    let rhs = x86_reg(decoded.reg_index, 1);
                    ops.extend(emit_alu_ops(
                        address,
                        1,
                        lhs,
                        rhs,
                        Destination::None,
                        AluKind::Test,
                        &mut temp,
                        &mut seq,
                    ));
                    ops
                }
                // A4b: 0xFE — INC(/0) or DEC(/1) r/m8
                0xFE => {
                    let kind = match decoded.reg_field {
                        0 => AluKind::Inc,
                        1 => AluKind::Dec,
                        _ => return Vec::new(),
                    };
                    let lhs = materialize_rm_value(
                        &decoded.rm,
                        1,
                        address,
                        &mut ops,
                        &mut temp,
                        &mut seq,
                    );
                    ops.extend(emit_alu_ops(
                        address,
                        1,
                        lhs,
                        const_u64(1, 1),
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
        // --- Phase 2: Primary 1-byte opcode gaps ---

        // Flag-set/clear/toggle instructions
        0xF5 => {
            // CMC: CF = CF XOR 1  (toggle carry flag)
            let cf = x86_flag_cf();
            vec![PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntXor,
                address,
                output: Some(cf.clone()),
                inputs: vec![cf, const_u64(1, 1)],
                asm_mnemonic: Some("CMC".to_string()),
            }]
        }
        0xF8 => vec![PcodeOp {
            // CLC: CF = 0
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::Copy,
            address,
            output: Some(x86_flag_cf()),
            inputs: vec![const_u64(0, 1)],
            asm_mnemonic: Some("CLC".to_string()),
        }],
        0xF9 => vec![PcodeOp {
            // STC: CF = 1
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::Copy,
            address,
            output: Some(x86_flag_cf()),
            inputs: vec![const_u64(1, 1)],
            asm_mnemonic: Some("STC".to_string()),
        }],
        0xFA => vec![PcodeOp {
            // CLI: IF = 0
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::Copy,
            address,
            output: Some(x86_flag_if()),
            inputs: vec![const_u64(0, 1)],
            asm_mnemonic: Some("CLI".to_string()),
        }],
        0xFB => vec![PcodeOp {
            // STI: IF = 1
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::Copy,
            address,
            output: Some(x86_flag_if()),
            inputs: vec![const_u64(1, 1)],
            asm_mnemonic: Some("STI".to_string()),
        }],
        0xFC => vec![PcodeOp {
            // CLD: DF = 0
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::Copy,
            address,
            output: Some(x86_flag_df()),
            inputs: vec![const_u64(0, 1)],
            asm_mnemonic: Some("CLD".to_string()),
        }],
        0xFD => vec![PcodeOp {
            // STD: DF = 1
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::Copy,
            address,
            output: Some(x86_flag_df()),
            inputs: vec![const_u64(1, 1)],
            asm_mnemonic: Some("STD".to_string()),
        }],

        // SAHF/LAHF: AH ↔ SF:ZF:0:AF:0:PF:1:CF (bits 7:6:5:4:3:2:1:0)
        0x9E => {
            // SAHF: load AH → SF/ZF/AF/PF/CF
            // AH = x86_reg(0, 1) offset by 1 byte = bits 8-15 of RAX
            // Model: extract each flag bit from AH
            let mut ops = Vec::new();
            let ah = temp.alloc(1);
            // AH is high byte of AX: SubPiece(rax, 1) → 1 byte
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::SubPiece,
                address,
                output: Some(ah.clone()),
                inputs: vec![x86_reg(0, 2), const_u64(1, 4)],
                asm_mnemonic: Some("SAHF_AH".to_string()),
            });
            // CF = bit 0 of AH
            let cf_bit = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntAnd,
                address,
                output: Some(cf_bit.clone()),
                inputs: vec![ah.clone(), const_u64(0x01, 1)],
                asm_mnemonic: Some("SAHF_CF".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(x86_flag_cf()),
                inputs: vec![cf_bit],
                asm_mnemonic: Some("SAHF_CF_WRITE".to_string()),
            });
            // PF = bit 2 of AH
            let pf_raw = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntRight,
                address,
                output: Some(pf_raw.clone()),
                inputs: vec![ah.clone(), const_u64(2, 1)],
                asm_mnemonic: Some("SAHF_PF_SHIFT".to_string()),
            });
            let pf_bit = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntAnd,
                address,
                output: Some(pf_bit.clone()),
                inputs: vec![pf_raw, const_u64(1, 1)],
                asm_mnemonic: Some("SAHF_PF".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(x86_flag_pf()),
                inputs: vec![pf_bit],
                asm_mnemonic: Some("SAHF_PF_WRITE".to_string()),
            });
            // AF = bit 4 of AH
            let af_raw = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntRight,
                address,
                output: Some(af_raw.clone()),
                inputs: vec![ah.clone(), const_u64(4, 1)],
                asm_mnemonic: Some("SAHF_AF_SHIFT".to_string()),
            });
            let af_bit = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntAnd,
                address,
                output: Some(af_bit.clone()),
                inputs: vec![af_raw, const_u64(1, 1)],
                asm_mnemonic: Some("SAHF_AF".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(x86_flag_af()),
                inputs: vec![af_bit],
                asm_mnemonic: Some("SAHF_AF_WRITE".to_string()),
            });
            // ZF = bit 6 of AH
            let zf_raw = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntRight,
                address,
                output: Some(zf_raw.clone()),
                inputs: vec![ah.clone(), const_u64(6, 1)],
                asm_mnemonic: Some("SAHF_ZF_SHIFT".to_string()),
            });
            let zf_bit = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntAnd,
                address,
                output: Some(zf_bit.clone()),
                inputs: vec![zf_raw, const_u64(1, 1)],
                asm_mnemonic: Some("SAHF_ZF".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(x86_flag_zf()),
                inputs: vec![zf_bit],
                asm_mnemonic: Some("SAHF_ZF_WRITE".to_string()),
            });
            // SF = bit 7 of AH
            let sf_raw = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntRight,
                address,
                output: Some(sf_raw.clone()),
                inputs: vec![ah, const_u64(7, 1)],
                asm_mnemonic: Some("SAHF_SF_SHIFT".to_string()),
            });
            let sf_bit = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntAnd,
                address,
                output: Some(sf_bit.clone()),
                inputs: vec![sf_raw, const_u64(1, 1)],
                asm_mnemonic: Some("SAHF_SF".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(x86_flag_sf()),
                inputs: vec![sf_bit],
                asm_mnemonic: Some("SAHF_SF_WRITE".to_string()),
            });
            ops
        }
        0x9F => {
            // LAHF: AH = SF:ZF:0:AF:0:PF:1:CF (bits 7..0)
            // Build AH as: (SF<<7)|(ZF<<6)|(AF<<4)|(PF<<2)|0x02|CF
            let mut ops = Vec::new();
            let cf_ext = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(cf_ext.clone()),
                inputs: vec![x86_flag_cf()],
                asm_mnemonic: Some("LAHF_CF".to_string()),
            });
            let pf_shifted = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntLeft,
                address,
                output: Some(pf_shifted.clone()),
                inputs: vec![x86_flag_pf(), const_u64(2, 1)],
                asm_mnemonic: Some("LAHF_PF_SHIFT".to_string()),
            });
            let af_shifted = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntLeft,
                address,
                output: Some(af_shifted.clone()),
                inputs: vec![x86_flag_af(), const_u64(4, 1)],
                asm_mnemonic: Some("LAHF_AF_SHIFT".to_string()),
            });
            let zf_shifted = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntLeft,
                address,
                output: Some(zf_shifted.clone()),
                inputs: vec![x86_flag_zf(), const_u64(6, 1)],
                asm_mnemonic: Some("LAHF_ZF_SHIFT".to_string()),
            });
            let sf_shifted = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntLeft,
                address,
                output: Some(sf_shifted.clone()),
                inputs: vec![x86_flag_sf(), const_u64(7, 1)],
                asm_mnemonic: Some("LAHF_SF_SHIFT".to_string()),
            });
            // Combine all bits: start with bit1=1 (reserved)
            let t0 = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(t0.clone()),
                inputs: vec![cf_ext, const_u64(0x02, 1)],
                asm_mnemonic: Some("LAHF_OR0".to_string()),
            });
            let t1 = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(t1.clone()),
                inputs: vec![t0, pf_shifted],
                asm_mnemonic: Some("LAHF_OR1".to_string()),
            });
            let t2 = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(t2.clone()),
                inputs: vec![t1, af_shifted],
                asm_mnemonic: Some("LAHF_OR2".to_string()),
            });
            let t3 = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(t3.clone()),
                inputs: vec![t2, zf_shifted],
                asm_mnemonic: Some("LAHF_OR3".to_string()),
            });
            let ah_val = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(ah_val.clone()),
                inputs: vec![t3, sf_shifted],
                asm_mnemonic: Some("LAHF_OR4".to_string()),
            });
            // Write AH: deposit byte 1 of AX register (AH = bits 8-15 of RAX)
            // Use a piece-concat: new_ax = (ah_val << 8) | (AL & 0xFF)
            let al_val = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::SubPiece,
                address,
                output: Some(al_val.clone()),
                inputs: vec![x86_reg(0, 2), const_u64(0, 4)],
                asm_mnemonic: Some("LAHF_GET_AL".to_string()),
            });
            let ah_ext = temp.alloc(2);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntZExt,
                address,
                output: Some(ah_ext.clone()),
                inputs: vec![ah_val],
                asm_mnemonic: Some("LAHF_AH_ZEXT".to_string()),
            });
            let ah_shifted_16 = temp.alloc(2);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntLeft,
                address,
                output: Some(ah_shifted_16.clone()),
                inputs: vec![ah_ext, const_u64(8, 2)],
                asm_mnemonic: Some("LAHF_AH_SHIFT16".to_string()),
            });
            let al_ext = temp.alloc(2);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntZExt,
                address,
                output: Some(al_ext.clone()),
                inputs: vec![al_val],
                asm_mnemonic: Some("LAHF_AL_ZEXT".to_string()),
            });
            let new_ax = temp.alloc(2);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(new_ax.clone()),
                inputs: vec![ah_shifted_16, al_ext],
                asm_mnemonic: Some("LAHF_NEW_AX".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(x86_reg(0, 2)),
                inputs: vec![new_ax],
                asm_mnemonic: Some("LAHF_WRITE_AX".to_string()),
            });
            ops
        }

        // XLAT/XLATB: AL = [RBX + zero_extend(AL)]
        0xD7 => {
            let mut ops = Vec::new();
            let al = x86_reg(0, 1);
            let rbx = x86_reg(3, 8);
            let al_ext = temp.alloc(8);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntZExt,
                address,
                output: Some(al_ext.clone()),
                inputs: vec![al.clone()],
                asm_mnemonic: Some("XLAT_AL_ZEXT".to_string()),
            });
            let xlat_addr = temp.alloc(8);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntAdd,
                address,
                output: Some(xlat_addr.clone()),
                inputs: vec![rbx, al_ext],
                asm_mnemonic: Some("XLAT_ADDR".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Load,
                address,
                output: Some(al),
                inputs: vec![const_u64(RAM_SPACE_ID, 8), xlat_addr],
                asm_mnemonic: Some("XLAT_LOAD".to_string()),
            });
            ops
        }

        // IN/OUT (port I/O) → use CallOther policies
        0xE4 => {
            // IN AL, imm8
            let port = match decode_immediate(insn, op_idx + 1, 1, 1, false) {
                Some(v) => v,
                None => return Vec::new(),
            };
            vec![PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::CallOther,
                address,
                output: Some(x86_reg(0, 1)),
                inputs: vec![const_u64(X86_IN_POLICY_ID, 8), port],
                asm_mnemonic: Some("IN_AL_IMM8".to_string()),
            }]
        }
        0xE5 => {
            // IN EAX, imm8
            let port = match decode_immediate(insn, op_idx + 1, 1, 1, false) {
                Some(v) => v,
                None => return Vec::new(),
            };
            vec![PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::CallOther,
                address,
                output: Some(x86_reg(0, size)),
                inputs: vec![const_u64(X86_IN_POLICY_ID, 8), port],
                asm_mnemonic: Some("IN_EAX_IMM8".to_string()),
            }]
        }
        0xE6 => {
            // OUT imm8, AL
            let port = match decode_immediate(insn, op_idx + 1, 1, 1, false) {
                Some(v) => v,
                None => return Vec::new(),
            };
            vec![PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::CallOther,
                address,
                output: None,
                inputs: vec![const_u64(X86_OUT_POLICY_ID, 8), port, x86_reg(0, 1)],
                asm_mnemonic: Some("OUT_IMM8_AL".to_string()),
            }]
        }
        0xE7 => {
            // OUT imm8, EAX
            let port = match decode_immediate(insn, op_idx + 1, 1, 1, false) {
                Some(v) => v,
                None => return Vec::new(),
            };
            vec![PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::CallOther,
                address,
                output: None,
                inputs: vec![const_u64(X86_OUT_POLICY_ID, 8), port, x86_reg(0, size)],
                asm_mnemonic: Some("OUT_IMM8_EAX".to_string()),
            }]
        }
        0xEC => {
            // IN AL, DX
            vec![PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::CallOther,
                address,
                output: Some(x86_reg(0, 1)),
                inputs: vec![const_u64(X86_IN_POLICY_ID, 8), x86_reg(2, 2)],
                asm_mnemonic: Some("IN_AL_DX".to_string()),
            }]
        }
        0xED => {
            // IN EAX, DX
            vec![PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::CallOther,
                address,
                output: Some(x86_reg(0, size)),
                inputs: vec![const_u64(X86_IN_POLICY_ID, 8), x86_reg(2, 2)],
                asm_mnemonic: Some("IN_EAX_DX".to_string()),
            }]
        }
        0xEE => {
            // OUT DX, AL
            vec![PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::CallOther,
                address,
                output: None,
                inputs: vec![
                    const_u64(X86_OUT_POLICY_ID, 8),
                    x86_reg(2, 2),
                    x86_reg(0, 1),
                ],
                asm_mnemonic: Some("OUT_DX_AL".to_string()),
            }]
        }
        0xEF => {
            // OUT DX, EAX
            vec![PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::CallOther,
                address,
                output: None,
                inputs: vec![
                    const_u64(X86_OUT_POLICY_ID, 8),
                    x86_reg(2, 2),
                    x86_reg(0, size),
                ],
                asm_mnemonic: Some("OUT_DX_EAX".to_string()),
            }]
        }

        // PUSHA/POPA: 32-bit only (push/pop EAX, ECX, EDX, EBX, ESP, EBP, ESI, EDI)
        0x60 => {
            // PUSHA: push EAX, ECX, EDX, EBX, (orig ESP), EBP, ESI, EDI
            let slot_size = 4u32; // 32-bit only
            let mut ops = Vec::new();
            let orig_esp = temp.alloc(8);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(orig_esp.clone()),
                inputs: vec![x86_reg(4, 8)],
                asm_mnemonic: Some("PUSHA_SAVE_ESP".to_string()),
            });
            for reg_idx in [0u32, 1, 2, 3] {
                emit_stack_push(
                    address,
                    x86_reg(reg_idx, slot_size),
                    slot_size,
                    &mut ops,
                    &mut temp,
                    &mut seq,
                    "PUSHA",
                );
            }
            // Push original ESP
            let esp_trunc = temp.alloc(slot_size);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::SubPiece,
                address,
                output: Some(esp_trunc.clone()),
                inputs: vec![orig_esp, const_u64(0, 4)],
                asm_mnemonic: Some("PUSHA_ESP_TRUNC".to_string()),
            });
            emit_stack_push(
                address,
                esp_trunc,
                slot_size,
                &mut ops,
                &mut temp,
                &mut seq,
                "PUSHA_ESP",
            );
            for reg_idx in [5u32, 6, 7] {
                emit_stack_push(
                    address,
                    x86_reg(reg_idx, slot_size),
                    slot_size,
                    &mut ops,
                    &mut temp,
                    &mut seq,
                    "PUSHA",
                );
            }
            ops
        }
        0x61 => {
            // POPA: pop EDI, ESI, EBP, (skip ESP), EBX, EDX, ECX, EAX
            let slot_size = 4u32;
            let mut ops = Vec::new();
            for reg_idx in [7u32, 6, 5] {
                let val = emit_stack_pop(address, slot_size, &mut ops, &mut temp, &mut seq, "POPA");
                let dst = x86_reg(reg_idx, slot_size);
                ops.push(PcodeOp {
                    seq_num: next_seq(&mut seq),
                    opcode: PcodeOpcode::Copy,
                    address,
                    output: Some(dst),
                    inputs: vec![val],
                    asm_mnemonic: Some("POPA_WRITE".to_string()),
                });
            }
            // Skip ESP: adjust SP by +4 instead of writing ESP
            let sp = stack_pointer_reg();
            let sp_skip = temp.alloc(8);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntAdd,
                address,
                output: Some(sp_skip.clone()),
                inputs: vec![sp.clone(), const_u64(u64::from(slot_size), 8)],
                asm_mnemonic: Some("POPA_SKIP_ESP".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(sp),
                inputs: vec![sp_skip],
                asm_mnemonic: Some("POPA_SKIP_ESP_WRITE".to_string()),
            });
            for reg_idx in [3u32, 2, 1, 0] {
                let val = emit_stack_pop(address, slot_size, &mut ops, &mut temp, &mut seq, "POPA");
                let dst = x86_reg(reg_idx, slot_size);
                ops.push(PcodeOp {
                    seq_num: next_seq(&mut seq),
                    opcode: PcodeOpcode::Copy,
                    address,
                    output: Some(dst),
                    inputs: vec![val],
                    asm_mnemonic: Some("POPA_WRITE".to_string()),
                });
            }
            ops
        }

        // FAR CALL/JMP → CallOther policy
        0x9A => vec![PcodeOp {
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: None,
            inputs: vec![const_u64(X86_FAR_CALL_POLICY_ID, 8)],
            asm_mnemonic: Some("FAR_CALL_POLICY".to_string()),
        }],
        0xEA => vec![PcodeOp {
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: None,
            inputs: vec![const_u64(X86_FAR_JMP_POLICY_ID, 8)],
            asm_mnemonic: Some("FAR_JMP_POLICY".to_string()),
        }],

        // --- Phase B: new standalone instructions ---

        // B1: 0xC9 LEAVE — RSP = RBP; pop RBP
        0xC9 => {
            let slot = stack_operand_size(&prefix);
            let rbp = x86_reg(5, slot);
            let rsp = stack_pointer_reg();
            let mut ops = Vec::new();
            // RSP = RBP
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(rsp),
                inputs: vec![rbp.clone()],
                asm_mnemonic: Some("LEAVE_RSP_SET".to_string()),
            });
            // pop: val = *RSP; RSP += slot
            let val = emit_stack_pop(address, slot, &mut ops, &mut temp, &mut seq, "LEAVE");
            // RBP = val
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(rbp),
                inputs: vec![val],
                asm_mnemonic: Some("LEAVE_RBP_WRITE".to_string()),
            });
            ops
        }

        // B2: 0x63 MOVSXD — sign-extend r/m32 into r64 (or copy r/m32→r32 in 32-bit mode)
        0x63 => {
            let mut pre_ops = Vec::new();
            let decoded = match decode_modrm_operand(
                insn,
                op_idx,
                &prefix,
                4,
                address,
                &mut temp,
                &mut pre_ops,
                &mut seq,
            ) {
                Some(v) => v,
                None => return Vec::new(),
            };
            let mut ops = pre_ops;
            let src = materialize_rm_value(&decoded.rm, 4, address, &mut ops, &mut temp, &mut seq);
            if size == 8 {
                // MOVSXD r64, r/m32
                let dst = x86_reg(decoded.reg_index, 8);
                ops.push(PcodeOp {
                    seq_num: next_seq(&mut seq),
                    opcode: PcodeOpcode::IntSExt,
                    address,
                    output: Some(dst),
                    inputs: vec![src],
                    asm_mnemonic: Some("MOVSXD".to_string()),
                });
            } else {
                // 32-bit mode: plain copy, upper 32 bits implicitly zero-extend
                let dst = x86_reg(decoded.reg_index, 4);
                ops.push(PcodeOp {
                    seq_num: next_seq(&mut seq),
                    opcode: PcodeOpcode::Copy,
                    address,
                    output: Some(dst),
                    inputs: vec![src],
                    asm_mnemonic: Some("MOVSXD_32".to_string()),
                });
            }
            ops
        }

        // B3: 0x91–0x97 XCHG rAX, r<n>  (NOP 0x90 already handled above)
        0x91..=0x97 => {
            let reg_idx = u32::from(op & 7) + rex_b(&prefix);
            let mut ops = Vec::new();
            let rax = x86_reg(0, size);
            let other = x86_reg(reg_idx, size);
            let saved = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(saved.clone()),
                inputs: vec![rax.clone()],
                asm_mnemonic: Some("XCHG_RAX_SAVE".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(rax),
                inputs: vec![other.clone()],
                asm_mnemonic: Some("XCHG_RAX_WRITE".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(other),
                inputs: vec![saved],
                asm_mnemonic: Some("XCHG_REG_WRITE".to_string()),
            });
            ops
        }

        // --- Phase C: MOV moffs + PUSHF/POPF ---

        // C1: 0xA0–0xA3 MOV moffs (absolute address, no ModRM)
        0xA0 | 0xA1 | 0xA2 | 0xA3 => {
            let addr_width = if prefix.address_size_override {
                4usize
            } else {
                8
            };
            let abs_addr = match decode_immediate(insn, op_idx + 1, addr_width, 8, false) {
                Some(v) => v,
                None => return Vec::new(),
            };
            let data_size = if matches!(op, 0xA0 | 0xA2) {
                1u32
            } else {
                size
            };
            let mut ops = Vec::new();
            match op {
                0xA0 | 0xA1 => {
                    // Load *moffs → AL/rAX
                    let val = temp.alloc(data_size);
                    ops.push(PcodeOp {
                        seq_num: next_seq(&mut seq),
                        opcode: PcodeOpcode::Load,
                        address,
                        output: Some(val.clone()),
                        inputs: vec![const_u64(RAM_SPACE_ID, 8), abs_addr],
                        asm_mnemonic: Some("MOV_MOFFS_LOAD".to_string()),
                    });
                    ops.push(PcodeOp {
                        seq_num: next_seq(&mut seq),
                        opcode: PcodeOpcode::Copy,
                        address,
                        output: Some(x86_reg(0, data_size)),
                        inputs: vec![val],
                        asm_mnemonic: Some("MOV_MOFFS_WRITE".to_string()),
                    });
                }
                _ => {
                    // 0xA2 | 0xA3: Store AL/rAX → *moffs
                    ops.push(PcodeOp {
                        seq_num: next_seq(&mut seq),
                        opcode: PcodeOpcode::Store,
                        address,
                        output: None,
                        inputs: vec![const_u64(RAM_SPACE_ID, 8), abs_addr, x86_reg(0, data_size)],
                        asm_mnemonic: Some("MOV_MOFFS_STORE".to_string()),
                    });
                }
            }
            ops
        }

        // C2: 0x9C PUSHFQ / 0x9D POPFQ
        0x9C => {
            // PUSHFQ: assemble RFLAGS and push
            // Bit layout: CF(0), PF(2), AF(4), ZF(6), SF(7), DF(10), OF(11); bit1=1 (reserved)
            let slot = stack_operand_size(&prefix);
            let mut ops = Vec::new();
            // Extend each flag to `slot` bytes then shift into position
            let cf_ext = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntZExt,
                address,
                output: Some(cf_ext.clone()),
                inputs: vec![x86_flag_cf()],
                asm_mnemonic: Some("PUSHF_CF_EXT".to_string()),
            });
            let pf_ext = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntZExt,
                address,
                output: Some(pf_ext.clone()),
                inputs: vec![x86_flag_pf()],
                asm_mnemonic: Some("PUSHF_PF_EXT".to_string()),
            });
            let pf_sh = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntLeft,
                address,
                output: Some(pf_sh.clone()),
                inputs: vec![pf_ext, const_u64(2, slot)],
                asm_mnemonic: Some("PUSHF_PF_SHIFT".to_string()),
            });
            let af_ext = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntZExt,
                address,
                output: Some(af_ext.clone()),
                inputs: vec![x86_flag_af()],
                asm_mnemonic: Some("PUSHF_AF_EXT".to_string()),
            });
            let af_sh = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntLeft,
                address,
                output: Some(af_sh.clone()),
                inputs: vec![af_ext, const_u64(4, slot)],
                asm_mnemonic: Some("PUSHF_AF_SHIFT".to_string()),
            });
            let zf_ext = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntZExt,
                address,
                output: Some(zf_ext.clone()),
                inputs: vec![x86_flag_zf()],
                asm_mnemonic: Some("PUSHF_ZF_EXT".to_string()),
            });
            let zf_sh = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntLeft,
                address,
                output: Some(zf_sh.clone()),
                inputs: vec![zf_ext, const_u64(6, slot)],
                asm_mnemonic: Some("PUSHF_ZF_SHIFT".to_string()),
            });
            let sf_ext = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntZExt,
                address,
                output: Some(sf_ext.clone()),
                inputs: vec![x86_flag_sf()],
                asm_mnemonic: Some("PUSHF_SF_EXT".to_string()),
            });
            let sf_sh = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntLeft,
                address,
                output: Some(sf_sh.clone()),
                inputs: vec![sf_ext, const_u64(7, slot)],
                asm_mnemonic: Some("PUSHF_SF_SHIFT".to_string()),
            });
            let df_ext = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntZExt,
                address,
                output: Some(df_ext.clone()),
                inputs: vec![x86_flag_df()],
                asm_mnemonic: Some("PUSHF_DF_EXT".to_string()),
            });
            let df_sh = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntLeft,
                address,
                output: Some(df_sh.clone()),
                inputs: vec![df_ext, const_u64(10, slot)],
                asm_mnemonic: Some("PUSHF_DF_SHIFT".to_string()),
            });
            let of_ext = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntZExt,
                address,
                output: Some(of_ext.clone()),
                inputs: vec![x86_flag_of()],
                asm_mnemonic: Some("PUSHF_OF_EXT".to_string()),
            });
            let of_sh = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntLeft,
                address,
                output: Some(of_sh.clone()),
                inputs: vec![of_ext, const_u64(11, slot)],
                asm_mnemonic: Some("PUSHF_OF_SHIFT".to_string()),
            });
            // Assemble: start with bit1=1 | CF | PF<<2
            let t0 = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(t0.clone()),
                inputs: vec![cf_ext, const_u64(0x02, slot)],
                asm_mnemonic: Some("PUSHF_OR0".to_string()),
            });
            let t1 = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(t1.clone()),
                inputs: vec![t0, pf_sh],
                asm_mnemonic: Some("PUSHF_OR1".to_string()),
            });
            let t2 = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(t2.clone()),
                inputs: vec![t1, af_sh],
                asm_mnemonic: Some("PUSHF_OR2".to_string()),
            });
            let t3 = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(t3.clone()),
                inputs: vec![t2, zf_sh],
                asm_mnemonic: Some("PUSHF_OR3".to_string()),
            });
            let t4 = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(t4.clone()),
                inputs: vec![t3, sf_sh],
                asm_mnemonic: Some("PUSHF_OR4".to_string()),
            });
            let t5 = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(t5.clone()),
                inputs: vec![t4, df_sh],
                asm_mnemonic: Some("PUSHF_OR5".to_string()),
            });
            let flags_val = temp.alloc(slot);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(flags_val.clone()),
                inputs: vec![t5, of_sh],
                asm_mnemonic: Some("PUSHF_OR6".to_string()),
            });
            emit_stack_push(
                address, flags_val, slot, &mut ops, &mut temp, &mut seq, "PUSHF",
            );
            ops
        }

        0x9D => {
            // POPFQ: pop RFLAGS and extract individual flag bits
            let slot = stack_operand_size(&prefix);
            let mut ops = Vec::new();
            let raw = emit_stack_pop(address, slot, &mut ops, &mut temp, &mut seq, "POPF");
            // Helper: extract bit N into a 1-byte flag varnode
            macro_rules! extract_flag {
                ($bit:expr, $flag_fn:expr, $mnem_sh:literal, $mnem_and:literal, $mnem_wr:literal) => {{
                    let shifted = temp.alloc(slot);
                    ops.push(PcodeOp {
                        seq_num: next_seq(&mut seq),
                        opcode: PcodeOpcode::IntRight,
                        address,
                        output: Some(shifted.clone()),
                        inputs: vec![raw.clone(), const_u64($bit, slot)],
                        asm_mnemonic: Some($mnem_sh.to_string()),
                    });
                    let bit_val = temp.alloc(1);
                    ops.push(PcodeOp {
                        seq_num: next_seq(&mut seq),
                        opcode: PcodeOpcode::SubPiece,
                        address,
                        output: Some(bit_val.clone()),
                        inputs: vec![shifted, const_u64(0, 4)],
                        asm_mnemonic: Some($mnem_and.to_string()),
                    });
                    let masked = temp.alloc(1);
                    ops.push(PcodeOp {
                        seq_num: next_seq(&mut seq),
                        opcode: PcodeOpcode::IntAnd,
                        address,
                        output: Some(masked.clone()),
                        inputs: vec![bit_val, const_u64(1, 1)],
                        asm_mnemonic: Some($mnem_and.to_string()),
                    });
                    ops.push(PcodeOp {
                        seq_num: next_seq(&mut seq),
                        opcode: PcodeOpcode::Copy,
                        address,
                        output: Some($flag_fn),
                        inputs: vec![masked],
                        asm_mnemonic: Some($mnem_wr.to_string()),
                    });
                }};
            }
            extract_flag!(
                0u64,
                x86_flag_cf(),
                "POPF_CF_SH",
                "POPF_CF_AND",
                "POPF_CF_WR"
            );
            extract_flag!(
                2u64,
                x86_flag_pf(),
                "POPF_PF_SH",
                "POPF_PF_AND",
                "POPF_PF_WR"
            );
            extract_flag!(
                4u64,
                x86_flag_af(),
                "POPF_AF_SH",
                "POPF_AF_AND",
                "POPF_AF_WR"
            );
            extract_flag!(
                6u64,
                x86_flag_zf(),
                "POPF_ZF_SH",
                "POPF_ZF_AND",
                "POPF_ZF_WR"
            );
            extract_flag!(
                7u64,
                x86_flag_sf(),
                "POPF_SF_SH",
                "POPF_SF_AND",
                "POPF_SF_WR"
            );
            extract_flag!(
                10u64,
                x86_flag_df(),
                "POPF_DF_SH",
                "POPF_DF_AND",
                "POPF_DF_WR"
            );
            extract_flag!(
                11u64,
                x86_flag_of(),
                "POPF_OF_SH",
                "POPF_OF_AND",
                "POPF_OF_WR"
            );
            ops
        }

        // --- Phase C: ENTER ---

        // 0xC8 ENTER imm16, imm8 — decomposed as: PUSH RBP; MOV RBP, RSP; SUB RSP, alloc_size
        // Encoding: C8 iw ib (imm16 = alloc size, imm8 = nesting level — level 0 handled)
        0xC8 => {
            let alloc_vn = match decode_immediate(insn, op_idx + 1, 2, size, false) {
                Some(v) => v,
                None => return Vec::new(),
            };
            let slot = stack_operand_size(&prefix);
            let rbp = x86_reg(5, slot);
            let rsp = stack_pointer_reg();
            let mut ops = Vec::new();
            // PUSH RBP
            emit_stack_push(
                address,
                rbp.clone(),
                slot,
                &mut ops,
                &mut temp,
                &mut seq,
                "ENTER_PUSH_RBP",
            );
            // MOV RBP, RSP
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(rbp),
                inputs: vec![rsp.clone()],
                asm_mnemonic: Some("ENTER_FRAME".to_string()),
            });
            // SUB RSP, alloc_size
            let new_rsp = temp.alloc(slot);
            let alloc_ext = if alloc_vn.size < slot {
                let e = temp.alloc(slot);
                ops.push(PcodeOp {
                    seq_num: next_seq(&mut seq),
                    opcode: PcodeOpcode::IntZExt,
                    address,
                    output: Some(e.clone()),
                    inputs: vec![alloc_vn],
                    asm_mnemonic: Some("ENTER_ALLOC_EXT".to_string()),
                });
                e
            } else {
                alloc_vn
            };
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntSub,
                address,
                output: Some(new_rsp.clone()),
                inputs: vec![rsp.clone(), alloc_ext],
                asm_mnemonic: Some("ENTER_ALLOC".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(rsp),
                inputs: vec![new_rsp],
                asm_mnemonic: Some("ENTER_RSP_WRITE".to_string()),
            });
            ops
        }

        // Phase A: 1-byte 잔여 opcode 보완

        // HLT → CallOther
        0xF4 => vec![PcodeOp {
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: None,
            inputs: vec![const_u64(X86_HLT_POLICY_ID, 8)],
            asm_mnemonic: Some("HLT_POLICY".to_string()),
        }],

        // MOV Sreg, r/m16 — segment register write
        0x8E => {
            let mut ops = Vec::new();
            let decoded = match decode_modrm_operand(
                insn, op_idx, &prefix, 2, address, &mut temp, &mut ops, &mut seq,
            ) {
                Some(d) => d,
                None => return Vec::new(),
            };
            let src = materialize_rm_value(&decoded.rm, 2, address, &mut ops, &mut temp, &mut seq);
            let dst = x86_seg(u32::from(decoded.reg_field));
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(dst),
                inputs: vec![src],
                asm_mnemonic: Some("MOV_SEG_WRITE".to_string()),
            });
            ops
        }

        // BCD / legacy arithmetic adjust → CallOther
        0x27 => vec![PcodeOp {
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: None,
            inputs: vec![const_u64(X86_DAA_POLICY_ID, 8)],
            asm_mnemonic: Some("DAA_POLICY".to_string()),
        }],
        0x2F => vec![PcodeOp {
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: None,
            inputs: vec![const_u64(X86_DAS_POLICY_ID, 8)],
            asm_mnemonic: Some("DAS_POLICY".to_string()),
        }],
        0x37 => vec![PcodeOp {
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: None,
            inputs: vec![const_u64(X86_AAA_POLICY_ID, 8)],
            asm_mnemonic: Some("AAA_POLICY".to_string()),
        }],
        0x3F => vec![PcodeOp {
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: None,
            inputs: vec![const_u64(X86_AAS_POLICY_ID, 8)],
            asm_mnemonic: Some("AAS_POLICY".to_string()),
        }],

        // INS / OUTS (string I/O) → CallOther
        0x6C | 0x6D => vec![PcodeOp {
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: None,
            inputs: vec![const_u64(X86_INS_POLICY_ID, 8)],
            asm_mnemonic: Some("INS_POLICY".to_string()),
        }],
        0x6E | 0x6F => vec![PcodeOp {
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: None,
            inputs: vec![const_u64(X86_OUTS_POLICY_ID, 8)],
            asm_mnemonic: Some("OUTS_POLICY".to_string()),
        }],

        // Phase A: remaining 1-byte stubs

        // WAIT / FWAIT: serialize pending x87 FP exceptions
        0x9B => vec![PcodeOp {
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: None,
            inputs: vec![const_u64(X86_WAIT_POLICY_ID, 8)],
            asm_mnemonic: Some("WAIT_POLICY".to_string()),
        }],

        // INTO: interrupt on overflow
        0xCE => vec![PcodeOp {
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: None,
            inputs: vec![const_u64(X86_INTO_POLICY_ID, 8)],
            asm_mnemonic: Some("INTO_POLICY".to_string()),
        }],

        // IRET / IRETD / IRETQ: return from interrupt
        0xCF => vec![PcodeOp {
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: None,
            inputs: vec![const_u64(X86_IRET_POLICY_ID, 8)],
            asm_mnemonic: Some("IRET_POLICY".to_string()),
        }],

        // INT1 / ICEBP: single-step ICE breakpoint
        0xF1 => vec![PcodeOp {
            seq_num: next_seq(&mut seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: None,
            inputs: vec![const_u64(X86_INT1_POLICY_ID, 8)],
            asm_mnemonic: Some("INT1_POLICY".to_string()),
        }],

        // MOV r/m16, Sreg — segment register read (reverse of 0x8E)
        0x8C => {
            let mut ops = Vec::new();
            let decoded = match decode_modrm_operand(
                insn, op_idx, &prefix, 2, address, &mut temp, &mut ops, &mut seq,
            ) {
                Some(d) => d,
                None => return Vec::new(),
            };
            let src = x86_seg(u32::from(decoded.reg_field));
            ops = write_rm_value(&decoded.rm, src, address, &mut ops, &mut seq, "MOV_RM_SEG");
            ops
        }

        _ => Vec::new(),
    }
}

mod addressing;
mod alu;
mod ext;
#[cfg(test)]
mod tests;

use self::addressing::{
    decode_immediate, decode_modrm_operand, immediate_bytes_for_operand, operand_size,
    parse_prefixes,
};
use self::alu::emit_alu_ops;
use self::ext::{
    decode_extended_semantic, decode_imul_r_rm_imm, decode_vex_semantic, emit_div_one_operand,
    emit_mul_one_operand,
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

/// Emit proper P-code for ROL, ROR, RCL, or RCR.
/// `kind`: 0=ROL, 1=ROR, 2=RCL, 3=RCR
fn emit_rotate_ops(
    address: u64,
    size: u32,
    lhs: Varnode,
    count: Varnode,
    dst: Destination,
    kind: u8,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let size_bits = u64::from(size.saturating_mul(8));
    let count_mask = if size == 8 { 0x3Fu64 } else { 0x1Fu64 };

    let mut ops: Vec<PcodeOp> = Vec::new();

    // Normalize count to operand size, masking to [0, size_bits-1].
    let count_norm: Varnode = if count.is_constant {
        let masked = (count.constant_val as u64) & count_mask;
        if masked == 0 {
            return Vec::new();
        }
        const_u64(masked, size)
    } else {
        // Zero-extend to operand size if needed.
        let extended = if count.size == size {
            count
        } else {
            let ext = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntZExt,
                address,
                output: Some(ext.clone()),
                inputs: vec![count],
                asm_mnemonic: Some("ROT_COUNT_ZEXT".to_string()),
            });
            ext
        };
        // Apply mask.
        let masked = temp.alloc(size);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntAnd,
            address,
            output: Some(masked.clone()),
            inputs: vec![extended, const_u64(count_mask, size)],
            asm_mnemonic: Some("ROT_COUNT_MASK".to_string()),
        });
        masked
    };

    emit_rotate_ops_inner(
        address, size, size_bits, lhs, count_norm, ops, dst, kind, temp, seq,
    )
}

fn emit_rotate_ops_inner(
    address: u64,
    size: u32,
    size_bits: u64,
    lhs: Varnode,
    count: Varnode,
    mut ops: Vec<PcodeOp>,
    dst: Destination,
    kind: u8,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let result = match &dst {
        Destination::Reg(v) => v.clone(),
        Destination::Mem(_) | Destination::None => temp.alloc(size),
    };

    match kind {
        0 => {
            // ROL: result = (lhs << n) | (lhs >> (size_bits - n))
            let shift_left = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntLeft,
                address,
                output: Some(shift_left.clone()),
                inputs: vec![lhs.clone(), count.clone()],
                asm_mnemonic: Some("ROL_SHL".to_string()),
            });
            let complement = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntSub,
                address,
                output: Some(complement.clone()),
                inputs: vec![const_u64(size_bits, size), count.clone()],
                asm_mnemonic: Some("ROL_COMPLEMENT".to_string()),
            });
            let shift_right = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntRight,
                address,
                output: Some(shift_right.clone()),
                inputs: vec![lhs, complement],
                asm_mnemonic: Some("ROL_SHR".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(result.clone()),
                inputs: vec![shift_left, shift_right],
                asm_mnemonic: Some("ROL".to_string()),
            });
            // CF = bit 0 of result (last bit rotated into position 0)
            let cf_val = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntAnd,
                address,
                output: Some(cf_val.clone()),
                inputs: vec![result.clone(), const_u64(1, size)],
                asm_mnemonic: Some("ROL_CF_RAW".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::SubPiece,
                address,
                output: Some(x86_flag_cf()),
                inputs: vec![cf_val, const_u64(0, 4)],
                asm_mnemonic: Some("ROL_CF".to_string()),
            });
            // OF = CF XOR MSB(result) — only defined for 1-bit rotate
            let msb = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntRight,
                address,
                output: Some(msb.clone()),
                inputs: vec![result.clone(), const_u64(size_bits - 1, size)],
                asm_mnemonic: Some("ROL_MSB".to_string()),
            });
            let msb_bit = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::SubPiece,
                address,
                output: Some(msb_bit.clone()),
                inputs: vec![msb, const_u64(0, 4)],
                asm_mnemonic: Some("ROL_MSB_TRUNC".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntXor,
                address,
                output: Some(x86_flag_of()),
                inputs: vec![x86_flag_cf(), msb_bit],
                asm_mnemonic: Some("ROL_OF".to_string()),
            });
        }
        1 => {
            // ROR: result = (lhs >> n) | (lhs << (size_bits - n))
            let shift_right = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntRight,
                address,
                output: Some(shift_right.clone()),
                inputs: vec![lhs.clone(), count.clone()],
                asm_mnemonic: Some("ROR_SHR".to_string()),
            });
            let complement = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntSub,
                address,
                output: Some(complement.clone()),
                inputs: vec![const_u64(size_bits, size), count],
                asm_mnemonic: Some("ROR_COMPLEMENT".to_string()),
            });
            let shift_left = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntLeft,
                address,
                output: Some(shift_left.clone()),
                inputs: vec![lhs, complement],
                asm_mnemonic: Some("ROR_SHL".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(result.clone()),
                inputs: vec![shift_right, shift_left],
                asm_mnemonic: Some("ROR".to_string()),
            });
            // CF = MSB of result (bit that was bit 0 before rotation is now MSB)
            let cf_raw = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntRight,
                address,
                output: Some(cf_raw.clone()),
                inputs: vec![result.clone(), const_u64(size_bits - 1, size)],
                asm_mnemonic: Some("ROR_CF_RAW".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::SubPiece,
                address,
                output: Some(x86_flag_cf()),
                inputs: vec![cf_raw, const_u64(0, 4)],
                asm_mnemonic: Some("ROR_CF".to_string()),
            });
            // OF = MSB XOR second-highest bit — only defined for 1-bit rotate
            let msb2 = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntRight,
                address,
                output: Some(msb2.clone()),
                inputs: vec![result.clone(), const_u64(size_bits - 2, size)],
                asm_mnemonic: Some("ROR_MSB2".to_string()),
            });
            let msb2_bit = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::SubPiece,
                address,
                output: Some(msb2_bit.clone()),
                inputs: vec![msb2, const_u64(0, 4)],
                asm_mnemonic: Some("ROR_MSB2_TRUNC".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntXor,
                address,
                output: Some(x86_flag_of()),
                inputs: vec![x86_flag_cf(), msb2_bit],
                asm_mnemonic: Some("ROR_OF".to_string()),
            });
        }
        2 => {
            // RCL: rotate left through carry (count=1 path; arbitrary count falls back)
            // For count=1: result = (lhs << 1) | CF; new_CF = old MSB
            let new_cf = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntRight,
                address,
                output: Some(new_cf.clone()),
                inputs: vec![lhs.clone(), const_u64(size_bits - 1, size)],
                asm_mnemonic: Some("RCL_NEW_CF".to_string()),
            });
            let new_cf_bit = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::SubPiece,
                address,
                output: Some(new_cf_bit.clone()),
                inputs: vec![new_cf, const_u64(0, 4)],
                asm_mnemonic: Some("RCL_NEW_CF_TRUNC".to_string()),
            });
            let shifted = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntLeft,
                address,
                output: Some(shifted.clone()),
                inputs: vec![lhs, count],
                asm_mnemonic: Some("RCL_SHL".to_string()),
            });
            let cf_ext = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntZExt,
                address,
                output: Some(cf_ext.clone()),
                inputs: vec![x86_flag_cf()],
                asm_mnemonic: Some("RCL_CF_ZEXT".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(result.clone()),
                inputs: vec![shifted, cf_ext],
                asm_mnemonic: Some("RCL".to_string()),
            });
            // Update CF with the old MSB
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(x86_flag_cf()),
                inputs: vec![new_cf_bit],
                asm_mnemonic: Some("RCL_CF".to_string()),
            });
            // OF = new CF XOR new MSB (count=1 only)
            let new_msb = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntRight,
                address,
                output: Some(new_msb.clone()),
                inputs: vec![result.clone(), const_u64(size_bits - 1, size)],
                asm_mnemonic: Some("RCL_NEW_MSB".to_string()),
            });
            let new_msb_bit = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::SubPiece,
                address,
                output: Some(new_msb_bit.clone()),
                inputs: vec![new_msb, const_u64(0, 4)],
                asm_mnemonic: Some("RCL_NEW_MSB_TRUNC".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntXor,
                address,
                output: Some(x86_flag_of()),
                inputs: vec![x86_flag_cf(), new_msb_bit],
                asm_mnemonic: Some("RCL_OF".to_string()),
            });
        }
        3 => {
            // RCR: rotate right through carry (count=1 path; arbitrary count falls back)
            // For count=1: result = (lhs >> 1) | (CF << (size_bits-1)); new_CF = old bit 0
            let new_cf = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::SubPiece,
                address,
                output: Some(new_cf.clone()),
                inputs: vec![lhs.clone(), const_u64(0, 4)],
                asm_mnemonic: Some("RCR_NEW_CF".to_string()),
            });
            let shifted = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntRight,
                address,
                output: Some(shifted.clone()),
                inputs: vec![lhs, count],
                asm_mnemonic: Some("RCR_SHR".to_string()),
            });
            let cf_ext = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntZExt,
                address,
                output: Some(cf_ext.clone()),
                inputs: vec![x86_flag_cf()],
                asm_mnemonic: Some("RCR_CF_ZEXT".to_string()),
            });
            let cf_shifted = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntLeft,
                address,
                output: Some(cf_shifted.clone()),
                inputs: vec![cf_ext, const_u64(size_bits - 1, size)],
                asm_mnemonic: Some("RCR_CF_SHIFT".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(result.clone()),
                inputs: vec![shifted, cf_shifted],
                asm_mnemonic: Some("RCR".to_string()),
            });
            // Update CF with old bit 0
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(x86_flag_cf()),
                inputs: vec![new_cf],
                asm_mnemonic: Some("RCR_CF".to_string()),
            });
            // OF = new MSB XOR second-highest bit (count=1 only)
            let new_msb = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntRight,
                address,
                output: Some(new_msb.clone()),
                inputs: vec![result.clone(), const_u64(size_bits - 1, size)],
                asm_mnemonic: Some("RCR_NEW_MSB".to_string()),
            });
            let new_msb_bit = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::SubPiece,
                address,
                output: Some(new_msb_bit.clone()),
                inputs: vec![new_msb, const_u64(0, 4)],
                asm_mnemonic: Some("RCR_NEW_MSB_TRUNC".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntXor,
                address,
                output: Some(x86_flag_of()),
                inputs: vec![x86_flag_cf(), new_msb_bit],
                asm_mnemonic: Some("RCR_OF".to_string()),
            });
        }
        _ => {}
    }

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
            let src_addr =
                widen_index_to_address(src_idx, address, &mut ops, temp, seq, "MOVS_SRC");
            let dst_addr =
                widen_index_to_address(dst_idx, address, &mut ops, temp, seq, "MOVS_DST");
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
            emit_df_index_update(
                6, index_size, step, address, &mut ops, temp, seq, "MOVS_SRC",
            );
            emit_df_index_update(
                7, index_size, step, address, &mut ops, temp, seq, "MOVS_DST",
            );
        }
        0xA6 | 0xA7 => {
            let src_idx = x86_reg(6, index_size);
            let dst_idx = x86_reg(7, index_size);
            let src_addr =
                widen_index_to_address(src_idx, address, &mut ops, temp, seq, "CMPS_SRC");
            let dst_addr =
                widen_index_to_address(dst_idx, address, &mut ops, temp, seq, "CMPS_DST");
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
            emit_df_index_update(
                6, index_size, step, address, &mut ops, temp, seq, "CMPS_SRC",
            );
            emit_df_index_update(
                7, index_size, step, address, &mut ops, temp, seq, "CMPS_DST",
            );
        }
        0xAA | 0xAB => {
            let dst_idx = x86_reg(7, index_size);
            let dst_addr =
                widen_index_to_address(dst_idx, address, &mut ops, temp, seq, "STOS_DST");
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Store,
                address,
                output: None,
                inputs: vec![const_u64(RAM_SPACE_ID, 8), dst_addr, x86_reg(0, data_size)],
                asm_mnemonic: Some("STOS_STORE".to_string()),
            });
            emit_df_index_update(
                7, index_size, step, address, &mut ops, temp, seq, "STOS_DST",
            );
        }
        0xAC | 0xAD => {
            let src_idx = x86_reg(6, index_size);
            let src_addr =
                widen_index_to_address(src_idx, address, &mut ops, temp, seq, "LODS_SRC");
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
            emit_df_index_update(
                6, index_size, step, address, &mut ops, temp, seq, "LODS_SRC",
            );
        }
        0xAE | 0xAF => {
            let dst_idx = x86_reg(7, index_size);
            let dst_addr =
                widen_index_to_address(dst_idx, address, &mut ops, temp, seq, "SCAS_DST");
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
            emit_df_index_update(
                7, index_size, step, address, &mut ops, temp, seq, "SCAS_DST",
            );
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
