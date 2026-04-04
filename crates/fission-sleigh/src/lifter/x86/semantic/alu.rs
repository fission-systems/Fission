use super::*;

pub(super) fn emit_alu_ops(
    address: u64,
    size: u32,
    lhs: Varnode,
    rhs: Varnode,
    dst: Destination,
    kind: AluKind,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    if kind == AluKind::Adc {
        return emit_adc_ops(address, size, lhs, rhs, dst, temp, seq);
    }
    if kind == AluKind::Sbb {
        return emit_sbb_ops(address, size, lhs, rhs, dst, temp, seq);
    }
    if kind == AluKind::Neg {
        return emit_neg_ops(address, size, lhs, dst, temp, seq);
    }
    if matches!(kind, AluKind::Shl | AluKind::Shr | AluKind::Sar) {
        return emit_shift_ops(address, size, lhs, rhs, dst, kind, temp, seq);
    }

    let mut ops = Vec::new();
    let result = match &dst {
        Destination::Reg(v) => v.clone(),
        Destination::Mem(_) | Destination::None => temp.alloc(size),
    };

    let (opcode, mnemonic) = match kind {
        AluKind::Add => (PcodeOpcode::IntAdd, "ADD"),
        AluKind::Adc => unreachable!("handled above"),
        AluKind::Sub => (PcodeOpcode::IntSub, "SUB"),
        AluKind::Sbb => unreachable!("handled above"),
        AluKind::And => (PcodeOpcode::IntAnd, "AND"),
        AluKind::Or => (PcodeOpcode::IntOr, "OR"),
        AluKind::Xor => (PcodeOpcode::IntXor, "XOR"),
        AluKind::Cmp => (PcodeOpcode::IntSub, "CMP"),
        AluKind::Test => (PcodeOpcode::IntAnd, "TEST"),
        AluKind::Inc => (PcodeOpcode::IntAdd, "INC"),
        AluKind::Dec => (PcodeOpcode::IntSub, "DEC"),
        AluKind::Neg => unreachable!("handled above"),
        AluKind::Shl | AluKind::Shr | AluKind::Sar => unreachable!("handled above"),
    };

    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode,
        address,
        output: Some(result.clone()),
        inputs: vec![lhs.clone(), rhs.clone()],
        asm_mnemonic: Some(mnemonic.to_string()),
    });

    if let Destination::Mem(addr) = &dst {
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::Store,
            address,
            output: None,
            inputs: vec![const_u64(RAM_SPACE_ID, 8), addr.clone(), result.clone()],
            asm_mnemonic: Some("RM_STORE".to_string()),
        });
    }

    match kind {
        AluKind::Add => emit_arith_flags(&mut ops, address, size, lhs, rhs, result, false, temp, seq),
        AluKind::Sub | AluKind::Cmp => {
            emit_arith_flags(&mut ops, address, size, lhs, rhs, result, true, temp, seq)
        }
        AluKind::Inc => {
            emit_overflow_only_from_arith(&mut ops, address, lhs, rhs, false, temp, seq);
            emit_zsp_flags(&mut ops, address, size, result, temp, seq);
        }
        AluKind::Dec => {
            emit_overflow_only_from_arith(&mut ops, address, lhs, rhs, true, temp, seq);
            emit_zsp_flags(&mut ops, address, size, result, temp, seq);
        }
        AluKind::And | AluKind::Or | AluKind::Xor | AluKind::Test => {
            emit_logic_flags(&mut ops, address, size, result, temp, seq)
        }
        AluKind::Adc
        | AluKind::Sbb
        | AluKind::Neg
        | AluKind::Shl
        | AluKind::Shr
        | AluKind::Sar => unreachable!("handled above"),
    }

    ops
}

fn emit_adc_ops(
    address: u64,
    size: u32,
    lhs: Varnode,
    rhs: Varnode,
    dst: Destination,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let result = match &dst {
        Destination::Reg(v) => v.clone(),
        Destination::Mem(_) | Destination::None => temp.alloc(size),
    };
    let sum1 = temp.alloc(size);
    let cf_ext = temp.alloc(size);

    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAdd,
        address,
        output: Some(sum1.clone()),
        inputs: vec![lhs.clone(), rhs.clone()],
        asm_mnemonic: Some("ADC_SUM1".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntZExt,
        address,
        output: Some(cf_ext.clone()),
        inputs: vec![x86_flag_cf()],
        asm_mnemonic: Some("ADC_CF_EXT".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAdd,
        address,
        output: Some(result.clone()),
        inputs: vec![sum1.clone(), cf_ext.clone()],
        asm_mnemonic: Some("ADC".to_string()),
    });

    store_if_memory(&mut ops, address, &dst, result.clone(), seq);

    let cf1 = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntCarry,
        address,
        output: Some(cf1.clone()),
        inputs: vec![lhs.clone(), rhs.clone()],
        asm_mnemonic: Some("ADC_CF1".to_string()),
    });
    let cf2 = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntCarry,
        address,
        output: Some(cf2.clone()),
        inputs: vec![sum1.clone(), cf_ext.clone()],
        asm_mnemonic: Some("ADC_CF2".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::BoolOr,
        address,
        output: Some(x86_flag_cf()),
        inputs: vec![cf1, cf2],
        asm_mnemonic: Some("ADC_CF".to_string()),
    });

    let of1 = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSCarry,
        address,
        output: Some(of1.clone()),
        inputs: vec![lhs, rhs],
        asm_mnemonic: Some("ADC_OF1".to_string()),
    });
    let of2 = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSCarry,
        address,
        output: Some(of2.clone()),
        inputs: vec![sum1, cf_ext],
        asm_mnemonic: Some("ADC_OF2".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::BoolOr,
        address,
        output: Some(x86_flag_of()),
        inputs: vec![of1, of2],
        asm_mnemonic: Some("ADC_OF".to_string()),
    });

    emit_zsp_flags(&mut ops, address, size, result, temp, seq);
    ops
}

fn emit_sbb_ops(
    address: u64,
    size: u32,
    lhs: Varnode,
    rhs: Varnode,
    dst: Destination,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let result = match &dst {
        Destination::Reg(v) => v.clone(),
        Destination::Mem(_) | Destination::None => temp.alloc(size),
    };
    let cf_ext = temp.alloc(size);
    let rhs_cf = temp.alloc(size);

    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntZExt,
        address,
        output: Some(cf_ext.clone()),
        inputs: vec![x86_flag_cf()],
        asm_mnemonic: Some("SBB_CF_EXT".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAdd,
        address,
        output: Some(rhs_cf.clone()),
        inputs: vec![rhs, cf_ext],
        asm_mnemonic: Some("SBB_RHS_CF".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSub,
        address,
        output: Some(result.clone()),
        inputs: vec![lhs.clone(), rhs_cf.clone()],
        asm_mnemonic: Some("SBB".to_string()),
    });

    store_if_memory(&mut ops, address, &dst, result.clone(), seq);

    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntLess,
        address,
        output: Some(x86_flag_cf()),
        inputs: vec![lhs.clone(), rhs_cf.clone()],
        asm_mnemonic: Some("SBB_CF".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSBorrow,
        address,
        output: Some(x86_flag_of()),
        inputs: vec![lhs, rhs_cf],
        asm_mnemonic: Some("SBB_OF".to_string()),
    });

    emit_zsp_flags(&mut ops, address, size, result, temp, seq);
    ops
}

fn emit_neg_ops(
    address: u64,
    size: u32,
    lhs: Varnode,
    dst: Destination,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let result = match &dst {
        Destination::Reg(v) => v.clone(),
        Destination::Mem(_) | Destination::None => temp.alloc(size),
    };
    let zero = const_u64(0, size);

    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSub,
        address,
        output: Some(result.clone()),
        inputs: vec![zero.clone(), lhs.clone()],
        asm_mnemonic: Some("NEG".to_string()),
    });

    store_if_memory(&mut ops, address, &dst, result.clone(), seq);

    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntNotEqual,
        address,
        output: Some(x86_flag_cf()),
        inputs: vec![lhs.clone(), zero.clone()],
        asm_mnemonic: Some("NEG_CF".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSBorrow,
        address,
        output: Some(x86_flag_of()),
        inputs: vec![zero, lhs],
        asm_mnemonic: Some("NEG_OF".to_string()),
    });

    emit_zsp_flags(&mut ops, address, size, result, temp, seq);
    ops
}

fn emit_shift_ops(
    address: u64,
    size: u32,
    lhs: Varnode,
    count: Varnode,
    dst: Destination,
    kind: AluKind,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let count_mask = if size == 8 { 0x3F } else { 0x1F };
    let count_const = if count.is_constant {
        Some((count.constant_val as u64) & count_mask)
    } else {
        None
    };
    if count_const == Some(0) {
        return Vec::new();
    }

    let mut ops = Vec::new();
    let mut count_val = if count.is_constant {
        const_u64(count_const.unwrap_or(0), size)
    } else if count.size == size {
        count
    } else {
        let count_ext = temp.alloc(size);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntZExt,
            address,
            output: Some(count_ext.clone()),
            inputs: vec![count],
            asm_mnemonic: Some("SHIFT_COUNT_ZEXT".to_string()),
        });
        count_ext
    };
    if !count_val.is_constant {
        let masked = temp.alloc(size);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntAnd,
            address,
            output: Some(masked.clone()),
            inputs: vec![count_val, const_u64(count_mask, size)],
            asm_mnemonic: Some("SHIFT_COUNT_MASK".to_string()),
        });
        count_val = masked;
    }

    let count_nonzero = if count_const.is_none() {
        let nz = temp.alloc(1);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntNotEqual,
            address,
            output: Some(nz.clone()),
            inputs: vec![count_val.clone(), const_u64(0, size)],
            asm_mnemonic: Some("SHIFT_COUNT_NONZERO".to_string()),
        });
        Some(nz)
    } else {
        None
    };

    let shift_result = match (&dst, count_nonzero.as_ref()) {
        (Destination::Reg(_), Some(_)) => temp.alloc(size),
        (Destination::Reg(v), None) => v.clone(),
        (Destination::Mem(_) | Destination::None, _) => temp.alloc(size),
    };

    let shift_opcode = match kind {
        AluKind::Shl => PcodeOpcode::IntLeft,
        AluKind::Shr => PcodeOpcode::IntRight,
        AluKind::Sar => PcodeOpcode::IntSRight,
        _ => unreachable!("shift kind expected"),
    };

    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: shift_opcode,
        address,
        output: Some(shift_result.clone()),
        inputs: vec![lhs.clone(), count_val.clone()],
        asm_mnemonic: Some(match kind {
            AluKind::Shl => "SHL".to_string(),
            AluKind::Shr => "SHR".to_string(),
            AluKind::Sar => "SAR".to_string(),
            _ => unreachable!("shift kind expected"),
        }),
    });

    let result = if let Some(cond) = count_nonzero.as_ref() {
        let merged = emit_conditional_value_merge(
            &mut ops,
            address,
            size,
            shift_result.clone(),
            lhs.clone(),
            cond,
            temp,
            seq,
            "SHIFT_RESULT",
        );
        match &dst {
            Destination::Reg(v) => {
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::Copy,
                    address,
                    output: Some(v.clone()),
                    inputs: vec![merged],
                    asm_mnemonic: Some("SHIFT_RESULT_WRITE".to_string()),
                });
                v.clone()
            }
            Destination::Mem(_) | Destination::None => merged,
        }
    } else {
        shift_result.clone()
    };

    store_if_memory(&mut ops, address, &dst, result.clone(), seq);

    let cf_source = temp.alloc(size);
    match kind {
        AluKind::Shl => {
            let bits = u64::from(size).saturating_mul(8);
            let shift = if let Some(c) = count_const {
                const_u64(bits.saturating_sub(c), size)
            } else {
                let out = temp.alloc(size);
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::IntSub,
                    address,
                    output: Some(out.clone()),
                    inputs: vec![const_u64(bits, size), count_val.clone()],
                    asm_mnemonic: Some("SHL_CF_SHIFT".to_string()),
                });
                out
            };
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntRight,
                address,
                output: Some(cf_source.clone()),
                inputs: vec![lhs.clone(), shift],
                asm_mnemonic: Some("SHL_CF_SRC".to_string()),
            });
        }
        AluKind::Shr | AluKind::Sar => {
            let shift = if let Some(c) = count_const {
                const_u64(c.saturating_sub(1), size)
            } else {
                let out = temp.alloc(size);
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::IntSub,
                    address,
                    output: Some(out.clone()),
                    inputs: vec![count_val.clone(), const_u64(1, size)],
                    asm_mnemonic: Some("SHR_CF_SHIFT".to_string()),
                });
                out
            };
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntRight,
                address,
                output: Some(cf_source.clone()),
                inputs: vec![lhs.clone(), shift],
                asm_mnemonic: Some("SHR_CF_SRC".to_string()),
            });
        }
        _ => unreachable!("shift kind expected"),
    }

    let cf_target = if count_nonzero.is_some() {
        temp.alloc(1)
    } else {
        x86_flag_cf()
    };

    let cf_bit = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(cf_bit.clone()),
        inputs: vec![cf_source, const_u64(1, size)],
        asm_mnemonic: Some("SHIFT_CF_BIT".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntNotEqual,
        address,
        output: Some(cf_target.clone()),
        inputs: vec![cf_bit, const_u64(0, size)],
        asm_mnemonic: Some("SHIFT_CF".to_string()),
    });

    if let Some(cond) = count_nonzero.as_ref() {
        emit_conditional_flag_write(
            &mut ops,
            address,
            x86_flag_cf(),
            cf_target,
            cond,
            temp,
            seq,
            "SHIFT_CF",
        );
    }

    if count_const == Some(1) {
        match kind {
            AluKind::Shl => {
                let before = temp.alloc(1);
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::IntSLess,
                    address,
                    output: Some(before.clone()),
                    inputs: vec![lhs, const_u64(0, size)],
                    asm_mnemonic: Some("SHL_OF_BEFORE".to_string()),
                });
                let after = temp.alloc(1);
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::IntSLess,
                    address,
                    output: Some(after.clone()),
                    inputs: vec![result.clone(), const_u64(0, size)],
                    asm_mnemonic: Some("SHL_OF_AFTER".to_string()),
                });
                let of_target = if count_nonzero.is_some() {
                    temp.alloc(1)
                } else {
                    x86_flag_of()
                };
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::IntNotEqual,
                    address,
                    output: Some(of_target.clone()),
                    inputs: vec![before, after],
                    asm_mnemonic: Some("SHL_OF".to_string()),
                });
                if let Some(cond) = count_nonzero.as_ref() {
                    emit_conditional_flag_write(
                        &mut ops,
                        address,
                        x86_flag_of(),
                        of_target,
                        cond,
                        temp,
                        seq,
                        "SHIFT_OF",
                    );
                }
            }
            AluKind::Shr => {
                let bits = u64::from(size).saturating_mul(8);
                let msb_src = temp.alloc(size);
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::IntRight,
                    address,
                    output: Some(msb_src.clone()),
                    inputs: vec![lhs, const_u64(bits.saturating_sub(1), size)],
                    asm_mnemonic: Some("SHR_OF_SRC".to_string()),
                });
                let msb_bit = temp.alloc(size);
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::IntAnd,
                    address,
                    output: Some(msb_bit.clone()),
                    inputs: vec![msb_src, const_u64(1, size)],
                    asm_mnemonic: Some("SHR_OF_BIT".to_string()),
                });
                let of_target = if count_nonzero.is_some() {
                    temp.alloc(1)
                } else {
                    x86_flag_of()
                };
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::IntNotEqual,
                    address,
                    output: Some(of_target.clone()),
                    inputs: vec![msb_bit, const_u64(0, size)],
                    asm_mnemonic: Some("SHR_OF".to_string()),
                });
                if let Some(cond) = count_nonzero.as_ref() {
                    emit_conditional_flag_write(
                        &mut ops,
                        address,
                        x86_flag_of(),
                        of_target,
                        cond,
                        temp,
                        seq,
                        "SHIFT_OF",
                    );
                }
            }
            AluKind::Sar => {
                let of_target = if count_nonzero.is_some() {
                    temp.alloc(1)
                } else {
                    x86_flag_of()
                };
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::Copy,
                    address,
                    output: Some(of_target.clone()),
                    inputs: vec![const_u64(0, 1)],
                    asm_mnemonic: Some("SAR_OF_ZERO".to_string()),
                });
                if let Some(cond) = count_nonzero.as_ref() {
                    emit_conditional_flag_write(
                        &mut ops,
                        address,
                        x86_flag_of(),
                        of_target,
                        cond,
                        temp,
                        seq,
                        "SHIFT_OF",
                    );
                }
            }
            _ => unreachable!("shift kind expected"),
        }
    }

    if let Some(cond) = count_nonzero.as_ref() {
        let (zf, sf, pf) = emit_zsp_flags_to_temps(&mut ops, address, size, shift_result, temp, seq);
        emit_conditional_flag_write(&mut ops, address, x86_flag_zf(), zf, cond, temp, seq, "SHIFT_ZF");
        emit_conditional_flag_write(&mut ops, address, x86_flag_sf(), sf, cond, temp, seq, "SHIFT_SF");
        emit_conditional_flag_write(&mut ops, address, x86_flag_pf(), pf, cond, temp, seq, "SHIFT_PF");
    } else {
        emit_zsp_flags(&mut ops, address, size, result, temp, seq);
    }
    ops
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

fn emit_conditional_flag_write(
    ops: &mut Vec<PcodeOp>,
    address: u64,
    flag: Varnode,
    new_flag: Varnode,
    cond: &Varnode,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    tag: &str,
) {
    let merged = emit_conditional_value_merge(
        ops,
        address,
        1,
        new_flag,
        flag.clone(),
        cond,
        temp,
        seq,
        tag,
    );
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(flag),
        inputs: vec![merged],
        asm_mnemonic: Some(format!("{tag}_WRITE")),
    });
}

fn store_if_memory(ops: &mut Vec<PcodeOp>, address: u64, dst: &Destination, result: Varnode, seq: &mut u32) {
    if let Destination::Mem(addr) = dst {
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::Store,
            address,
            output: None,
            inputs: vec![const_u64(RAM_SPACE_ID, 8), addr.clone(), result],
            asm_mnemonic: Some("RM_STORE".to_string()),
        });
    }
}

fn emit_overflow_only_from_arith(
    ops: &mut Vec<PcodeOp>,
    address: u64,
    lhs: Varnode,
    rhs: Varnode,
    is_sub: bool,
    _temp: &mut X86TempFactory,
    seq: &mut u32,
) {
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: if is_sub {
            PcodeOpcode::IntSBorrow
        } else {
            PcodeOpcode::IntSCarry
        },
        address,
        output: Some(x86_flag_of()),
        inputs: vec![lhs, rhs],
        asm_mnemonic: Some(if is_sub {
            "DEC_OF".to_string()
        } else {
            "INC_OF".to_string()
        }),
    });
}

fn emit_arith_flags(
    ops: &mut Vec<PcodeOp>,
    address: u64,
    size: u32,
    lhs: Varnode,
    rhs: Varnode,
    result: Varnode,
    is_sub: bool,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) {
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: if is_sub {
            PcodeOpcode::IntLess
        } else {
            PcodeOpcode::IntCarry
        },
        address,
        output: Some(x86_flag_cf()),
        inputs: vec![lhs.clone(), rhs.clone()],
        asm_mnemonic: Some(if is_sub {
            "SUB_CF".to_string()
        } else {
            "ADD_CF".to_string()
        }),
    });

    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: if is_sub {
            PcodeOpcode::IntSBorrow
        } else {
            PcodeOpcode::IntSCarry
        },
        address,
        output: Some(x86_flag_of()),
        inputs: vec![lhs, rhs],
        asm_mnemonic: Some(if is_sub {
            "SUB_OF".to_string()
        } else {
            "ADD_OF".to_string()
        }),
    });

    emit_zsp_flags(ops, address, size, result, temp, seq);
}

fn emit_logic_flags(
    ops: &mut Vec<PcodeOp>,
    address: u64,
    size: u32,
    result: Varnode,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) {
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_flag_cf()),
        inputs: vec![const_u64(0, 1)],
        asm_mnemonic: Some("LOGIC_CF_ZERO".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_flag_of()),
        inputs: vec![const_u64(0, 1)],
        asm_mnemonic: Some("LOGIC_OF_ZERO".to_string()),
    });

    emit_zsp_flags(ops, address, size, result, temp, seq);
}

fn emit_zsp_flags(
    ops: &mut Vec<PcodeOp>,
    address: u64,
    size: u32,
    result: Varnode,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) {
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntEqual,
        address,
        output: Some(x86_flag_zf()),
        inputs: vec![result.clone(), const_u64(0, size)],
        asm_mnemonic: Some("SET_ZF".to_string()),
    });

    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSLess,
        address,
        output: Some(x86_flag_sf()),
        inputs: vec![result.clone(), const_u64(0, size)],
        asm_mnemonic: Some("SET_SF".to_string()),
    });

    let low8 = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(low8.clone()),
        inputs: vec![result, const_u64(0xFF, size)],
        asm_mnemonic: Some("PF_LOW8".to_string()),
    });

    let pop = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::PopCount,
        address,
        output: Some(pop.clone()),
        inputs: vec![low8],
        asm_mnemonic: Some("PF_POPCNT".to_string()),
    });

    let parity_bit = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(parity_bit.clone()),
        inputs: vec![pop, const_u64(1, size)],
        asm_mnemonic: Some("PF_LSB".to_string()),
    });

    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntEqual,
        address,
        output: Some(x86_flag_pf()),
        inputs: vec![parity_bit, const_u64(0, size)],
        asm_mnemonic: Some("SET_PF".to_string()),
    });
}

fn emit_zsp_flags_to_temps(
    ops: &mut Vec<PcodeOp>,
    address: u64,
    size: u32,
    result: Varnode,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> (Varnode, Varnode, Varnode) {
    let zf = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntEqual,
        address,
        output: Some(zf.clone()),
        inputs: vec![result.clone(), const_u64(0, size)],
        asm_mnemonic: Some("SET_ZF".to_string()),
    });

    let sf = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSLess,
        address,
        output: Some(sf.clone()),
        inputs: vec![result.clone(), const_u64(0, size)],
        asm_mnemonic: Some("SET_SF".to_string()),
    });

    let low8 = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(low8.clone()),
        inputs: vec![result, const_u64(0xFF, size)],
        asm_mnemonic: Some("PF_LOW8".to_string()),
    });

    let pop = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::PopCount,
        address,
        output: Some(pop.clone()),
        inputs: vec![low8],
        asm_mnemonic: Some("PF_POPCNT".to_string()),
    });

    let parity_bit = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(parity_bit.clone()),
        inputs: vec![pop, const_u64(1, size)],
        asm_mnemonic: Some("PF_LSB".to_string()),
    });

    let pf = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntEqual,
        address,
        output: Some(pf.clone()),
        inputs: vec![parity_bit, const_u64(0, size)],
        asm_mnemonic: Some("SET_PF".to_string()),
    });

    (zf, sf, pf)
}

