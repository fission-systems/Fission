use super::*;

pub(super) fn decode_movx(
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
    let decoded = match decode_modrm_operand(
        insn,
        op_idx + 1,
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

    let src = materialize_rm_value(&decoded.rm, src_size, address, &mut ops, temp, seq);
    let dst = x86_reg(decoded.reg_index, dst_size);
    let (opcode, mnemonic) = if dst_size == src_size {
        (
            PcodeOpcode::Copy,
            if is_sign_extend {
                "MOVSX_WRITE"
            } else {
                "MOVZX_WRITE"
            },
        )
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
        asm_mnemonic: Some(
            if is_signed {
                "IDIV_DIVIDEND"
            } else {
                "DIV_DIVIDEND"
            }
            .to_string(),
        ),
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
        asm_mnemonic: Some(
            if is_signed {
                "IDIV_DIVISOR_EXT"
            } else {
                "DIV_DIVISOR_EXT"
            }
            .to_string(),
        ),
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
        asm_mnemonic: Some(
            if is_signed {
                "IDIV_QUOT_LO"
            } else {
                "DIV_QUOT_LO"
            }
            .to_string(),
        ),
    });
    let remainder = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::SubPiece,
        address,
        output: Some(remainder.clone()),
        inputs: vec![remainder_full, const_u64(0, 4)],
        asm_mnemonic: Some(
            if is_signed {
                "IDIV_REM_LO"
            } else {
                "DIV_REM_LO"
            }
            .to_string(),
        ),
    });

    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_reg(0, size)),
        inputs: vec![quotient],
        asm_mnemonic: Some(
            if is_signed {
                "IDIV_QUOT_WRITE"
            } else {
                "DIV_QUOT_WRITE"
            }
            .to_string(),
        ),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(if size == 1 {
            x86_reg(4, 1)
        } else {
            x86_reg(2, size)
        }),
        inputs: vec![remainder],
        asm_mnemonic: Some(
            if is_signed {
                "IDIV_REM_WRITE"
            } else {
                "DIV_REM_WRITE"
            }
            .to_string(),
        ),
    });
}
