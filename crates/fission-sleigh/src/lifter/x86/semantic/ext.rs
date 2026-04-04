use super::*;
use super::super::predicate::emit_jcc_predicate_with_allocator;

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
        0xB6 | 0xB7 | 0xBE | 0xBF => {
            let src_size = if matches!(ext, 0xB6 | 0xBE) { 1 } else { 2 };
            let is_sign_extend = matches!(ext, 0xBE | 0xBF);
            decode_movx(insn, op_idx, prefix, size, src_size, is_sign_extend, address, temp, seq)
        }
        0xAF => decode_imul_r_rm(insn, op_idx, prefix, size, address, temp, seq),
        0xBC => decode_bsf_bsr(insn, op_idx, prefix, size, address, temp, seq, false),
        0xBD => decode_bsf_bsr(insn, op_idx, prefix, size, address, temp, seq, true),
        0x40..=0x4F => decode_cmovcc(insn, op_idx, prefix, size, address, temp, seq, ext - 0x40),
        0x90..=0x9F => decode_setcc(insn, op_idx, prefix, address, temp, seq, ext - 0x90),
        _ => Vec::new(),
    }
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
        output: Some(x86_reg(2, size)),
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
        inputs: vec![const_u64(policy_id, 8), divisor.clone()],
        asm_mnemonic: Some(policy_tag.to_string()),
    });

    let full_size = size.saturating_mul(2);
    let dividend = temp.alloc(full_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Piece,
        address,
        output: Some(dividend.clone()),
        inputs: vec![x86_reg(2, size), x86_reg(0, size)],
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
        output: Some(x86_reg(2, size)),
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
