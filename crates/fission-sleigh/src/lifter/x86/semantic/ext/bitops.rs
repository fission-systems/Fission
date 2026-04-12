use super::cond::emit_conditional_value_merge;
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BitTestKind {
    Bt,
    Bts,
    Btr,
    Btc,
}

#[derive(Debug, Clone)]
enum BitTestTarget {
    Reg(Varnode),
    Mem(Varnode),
}

fn bt_tag(kind: BitTestKind) -> &'static str {
    match kind {
        BitTestKind::Bt => "BT",
        BitTestKind::Bts => "BTS",
        BitTestKind::Btr => "BTR",
        BitTestKind::Btc => "BTC",
    }
}

fn bt_word_shift(size: u32) -> u64 {
    let bits = u64::from(size.saturating_mul(8).max(1));
    u64::from(bits.trailing_zeros())
}

pub(super) fn decode_bt_family(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    kind: BitTestKind,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, op_idx + 1, prefix, size, address, temp, &mut ops, seq) {
            Some(v) => v,
            None => return Vec::new(),
        };

    let tag = bt_tag(kind);
    let bit_index = x86_reg(decoded.reg_index, size);
    let bits_per_word = u64::from(size.saturating_mul(8));
    let local_index = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(local_index.clone()),
        inputs: vec![
            bit_index.clone(),
            const_u64(bits_per_word.saturating_sub(1), size),
        ],
        asm_mnemonic: Some(format!("{tag}_BIT_INDEX")),
    });

    let (base_value, target) = match decoded.rm {
        RmOperand::Reg(dst) => (dst.clone(), BitTestTarget::Reg(dst)),
        RmOperand::Mem(base_addr) => {
            let word_index = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntSRight,
                address,
                output: Some(word_index.clone()),
                inputs: vec![bit_index, const_u64(bt_word_shift(size), size)],
                asm_mnemonic: Some(format!("{tag}_MEM_WORD_INDEX")),
            });

            let byte_delta = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntMult,
                address,
                output: Some(byte_delta.clone()),
                inputs: vec![word_index, const_u64(u64::from(size), size)],
                asm_mnemonic: Some(format!("{tag}_MEM_BYTE_DELTA")),
            });

            let addr_delta = if byte_delta.size < base_addr.size {
                let extended = temp.alloc(base_addr.size);
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::IntSExt,
                    address,
                    output: Some(extended.clone()),
                    inputs: vec![byte_delta],
                    asm_mnemonic: Some(format!("{tag}_MEM_ADDR_DELTA_EXT")),
                });
                extended
            } else if byte_delta.size > base_addr.size {
                let truncated = temp.alloc(base_addr.size);
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::SubPiece,
                    address,
                    output: Some(truncated.clone()),
                    inputs: vec![byte_delta, const_u64(0, 4)],
                    asm_mnemonic: Some(format!("{tag}_MEM_ADDR_DELTA_TRUNC")),
                });
                truncated
            } else {
                byte_delta
            };

            let effective_addr = temp.alloc(base_addr.size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntAdd,
                address,
                output: Some(effective_addr.clone()),
                inputs: vec![base_addr, addr_delta],
                asm_mnemonic: Some(format!("{tag}_MEM_ADDR")),
            });

            let loaded = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Load,
                address,
                output: Some(loaded.clone()),
                inputs: vec![const_u64(RAM_SPACE_ID, 8), effective_addr.clone()],
                asm_mnemonic: Some(format!("{tag}_MEM_LOAD")),
            });
            (loaded, BitTestTarget::Mem(effective_addr))
        }
    };

    let bit_mask = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntLeft,
        address,
        output: Some(bit_mask.clone()),
        inputs: vec![const_u64(1, size), local_index],
        asm_mnemonic: Some(format!("{tag}_MASK")),
    });

    let bit_value = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(bit_value.clone()),
        inputs: vec![base_value.clone(), bit_mask.clone()],
        asm_mnemonic: Some(format!("{tag}_BIT")),
    });

    let cf = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntNotEqual,
        address,
        output: Some(cf.clone()),
        inputs: vec![bit_value, const_u64(0, size)],
        asm_mnemonic: Some(format!("{tag}_CF")),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_flag_cf()),
        inputs: vec![cf],
        asm_mnemonic: Some(format!("{tag}_CF_WRITE")),
    });

    let updated = match kind {
        BitTestKind::Bt => None,
        BitTestKind::Bts => {
            let out = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(out.clone()),
                inputs: vec![base_value, bit_mask],
                asm_mnemonic: Some(format!("{tag}_SET")),
            });
            Some(out)
        }
        BitTestKind::Btr => {
            let inv_mask = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntNegate,
                address,
                output: Some(inv_mask.clone()),
                inputs: vec![bit_mask],
                asm_mnemonic: Some(format!("{tag}_MASK_INV")),
            });
            let out = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntAnd,
                address,
                output: Some(out.clone()),
                inputs: vec![base_value, inv_mask],
                asm_mnemonic: Some(format!("{tag}_RESET")),
            });
            Some(out)
        }
        BitTestKind::Btc => {
            let out = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntXor,
                address,
                output: Some(out.clone()),
                inputs: vec![base_value, bit_mask],
                asm_mnemonic: Some(format!("{tag}_TOGGLE")),
            });
            Some(out)
        }
    };

    if let Some(value) = updated {
        match target {
            BitTestTarget::Reg(dst) => {
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::Copy,
                    address,
                    output: Some(dst),
                    inputs: vec![value],
                    asm_mnemonic: Some(format!("{tag}_WRITE")),
                });
            }
            BitTestTarget::Mem(addr_vn) => {
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::Store,
                    address,
                    output: None,
                    inputs: vec![const_u64(RAM_SPACE_ID, 8), addr_vn, value],
                    asm_mnemonic: Some(format!("{tag}_STORE")),
                });
            }
        }
    }

    ops
}

/// 0F BA /4-/7: BT/BTS/BTR/BTC r/m, imm8
/// Bit-test family with immediate bit index — reg field selects the operation.
pub(super) fn decode_bt_imm8(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, op_idx + 1, prefix, size, address, temp, &mut ops, seq) {
            Some(v) => v,
            None => return Vec::new(),
        };

    let kind = match decoded.reg_field {
        4 => BitTestKind::Bt,
        5 => BitTestKind::Bts,
        6 => BitTestKind::Btr,
        7 => BitTestKind::Btc,
        _ => return Vec::new(),
    };
    let tag = bt_tag(kind);

    // imm8 comes immediately after the ModRM (and SIB/disp)
    let imm8_byte = match insn.get(decoded.next_idx) {
        Some(v) => *v,
        None => return Vec::new(),
    };
    // Mask to valid range for the operand size
    let bits_per_word = size.saturating_mul(8);
    let raw_index = u64::from(imm8_byte) & u64::from(bits_per_word.saturating_sub(1));
    let local_index = const_u64(raw_index, size);

    let base_value = match &decoded.rm {
        RmOperand::Reg(dst) => dst.clone(),
        RmOperand::Mem(base_addr) => {
            let loaded = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Load,
                address,
                output: Some(loaded.clone()),
                inputs: vec![const_u64(RAM_SPACE_ID, 8), base_addr.clone()],
                asm_mnemonic: Some(format!("{tag}_IMM8_MEM_LOAD")),
            });
            loaded
        }
    };

    let bit_mask = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntLeft,
        address,
        output: Some(bit_mask.clone()),
        inputs: vec![const_u64(1, size), local_index],
        asm_mnemonic: Some(format!("{tag}_IMM8_MASK")),
    });

    let bit_value = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntAnd,
        address,
        output: Some(bit_value.clone()),
        inputs: vec![base_value.clone(), bit_mask.clone()],
        asm_mnemonic: Some(format!("{tag}_IMM8_BIT")),
    });

    let cf = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntNotEqual,
        address,
        output: Some(cf.clone()),
        inputs: vec![bit_value, const_u64(0, size)],
        asm_mnemonic: Some(format!("{tag}_IMM8_CF")),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_flag_cf()),
        inputs: vec![cf],
        asm_mnemonic: Some(format!("{tag}_IMM8_CF_WRITE")),
    });

    let updated = match kind {
        BitTestKind::Bt => None,
        BitTestKind::Bts => {
            let out = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntOr,
                address,
                output: Some(out.clone()),
                inputs: vec![base_value, bit_mask],
                asm_mnemonic: Some(format!("{tag}_IMM8_SET")),
            });
            Some(out)
        }
        BitTestKind::Btr => {
            let inv_mask = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntNegate,
                address,
                output: Some(inv_mask.clone()),
                inputs: vec![bit_mask],
                asm_mnemonic: Some(format!("{tag}_IMM8_MASK_INV")),
            });
            let out = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntAnd,
                address,
                output: Some(out.clone()),
                inputs: vec![base_value, inv_mask],
                asm_mnemonic: Some(format!("{tag}_IMM8_RESET")),
            });
            Some(out)
        }
        BitTestKind::Btc => {
            let out = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntXor,
                address,
                output: Some(out.clone()),
                inputs: vec![base_value, bit_mask],
                asm_mnemonic: Some(format!("{tag}_IMM8_TOGGLE")),
            });
            Some(out)
        }
    };

    if let Some(value) = updated {
        match &decoded.rm {
            RmOperand::Reg(dst) => {
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::Copy,
                    address,
                    output: Some(dst.clone()),
                    inputs: vec![value],
                    asm_mnemonic: Some(format!("{tag}_IMM8_WRITE")),
                });
            }
            RmOperand::Mem(addr_vn) => {
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::Store,
                    address,
                    output: None,
                    inputs: vec![const_u64(RAM_SPACE_ID, 8), addr_vn.clone(), value],
                    asm_mnemonic: Some(format!("{tag}_IMM8_STORE")),
                });
            }
        }
    }

    ops
}

/// TZCNT r, r/m (F3 0F BC): count trailing zeros — ZF=1 if src==0, CF=1 if src==0.
fn decode_tzcnt(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, op_idx + 1, prefix, size, address, temp, &mut ops, seq) {
            Some(v) => v,
            None => return Vec::new(),
        };
    let src = materialize_rm_value(&decoded.rm, size, address, &mut ops, temp, seq);
    let dst = x86_reg(decoded.reg_index, size);

    // ZF = (src == 0); CF = (src == 0)
    let is_zero = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntEqual,
        address,
        output: Some(is_zero.clone()),
        inputs: vec![src.clone(), const_u64(0, size)],
        asm_mnemonic: Some("TZCNT_IS_ZERO".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_flag_zf()),
        inputs: vec![is_zero.clone()],
        asm_mnemonic: Some("TZCNT_ZF".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_flag_cf()),
        inputs: vec![is_zero],
        asm_mnemonic: Some("TZCNT_CF".to_string()),
    });

    // TZCNT = PopCount(src & -src) - 1, but use BSF index helper
    let idx = emit_bsf_index(&mut ops, address, size, src, temp, seq);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(dst),
        inputs: vec![idx],
        asm_mnemonic: Some("TZCNT_WRITE".to_string()),
    });
    ops
}

/// LZCNT r, r/m (F3 0F BD): count leading zeros — ZF=1 if result==0, CF=1 if src==0.
fn decode_lzcnt(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, op_idx + 1, prefix, size, address, temp, &mut ops, seq) {
            Some(v) => v,
            None => return Vec::new(),
        };
    let src = materialize_rm_value(&decoded.rm, size, address, &mut ops, temp, seq);
    let dst = x86_reg(decoded.reg_index, size);
    let width_bits = u64::from(size.saturating_mul(8));

    // CF = (src == 0) [src was zero before leading-zero count]
    let src_zero = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntEqual,
        address,
        output: Some(src_zero.clone()),
        inputs: vec![src.clone(), const_u64(0, size)],
        asm_mnemonic: Some("LZCNT_SRC_ZERO".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(x86_flag_cf()),
        inputs: vec![src_zero],
        asm_mnemonic: Some("LZCNT_CF".to_string()),
    });

    // LZCNT = (width - 1) - BSR(src) when src != 0; = width when src == 0.
    // Emit BSR-based index then compute: lzcnt = (width-1) - bsr_idx
    let bsr_idx = emit_bsr_index(&mut ops, address, size, src.clone(), temp, seq);
    let lzcnt_nonzero = temp.alloc(size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntSub,
        address,
        output: Some(lzcnt_nonzero.clone()),
        inputs: vec![const_u64(width_bits - 1, size), bsr_idx],
        asm_mnemonic: Some("LZCNT_NONZERO".to_string()),
    });
    let width_const = const_u64(width_bits, size);
    let is_nonzero = temp.alloc(1);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntNotEqual,
        address,
        output: Some(is_nonzero.clone()),
        inputs: vec![src, const_u64(0, size)],
        asm_mnemonic: Some("LZCNT_SRC_NONZERO".to_string()),
    });
    let result = emit_conditional_value_merge(
        &mut ops,
        address,
        size,
        lzcnt_nonzero,
        width_const,
        &is_nonzero,
        temp,
        seq,
        "LZCNT",
    );
    // ZF = (result == 0) → i.e. result == 0 only when src had MSB set (BSR = width-1)
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntEqual,
        address,
        output: Some(x86_flag_zf()),
        inputs: vec![result.clone(), const_u64(0, size)],
        asm_mnemonic: Some("LZCNT_ZF".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(dst),
        inputs: vec![result],
        asm_mnemonic: Some("LZCNT_WRITE".to_string()),
    });
    ops
}

pub(super) fn decode_bsf_bsr(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    is_reverse: bool,
) -> Vec<PcodeOp> {
    // F3 prefix changes BSF→TZCNT and BSR→LZCNT
    if prefix.rep_prefix == Some(RepPrefix::Rep) {
        return if is_reverse {
            decode_lzcnt(insn, op_idx, prefix, size, address, temp, seq)
        } else {
            decode_tzcnt(insn, op_idx, prefix, size, address, temp, seq)
        };
    }
    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, op_idx + 1, prefix, size, address, temp, &mut ops, seq) {
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
