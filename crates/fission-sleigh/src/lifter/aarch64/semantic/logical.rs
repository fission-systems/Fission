use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use super::super::common::{
    a64_flag_c, a64_flag_n, a64_flag_v, a64_flag_z, a64_reg, const_u64, A64TempFactory,
};

pub(super) fn decode_logical_shifted_reg(
    word: u32,
    address: u64,
    temp: &mut A64TempFactory,
    seq: &mut u32,
) -> Option<Vec<PcodeOp>> {
    if (word & 0x1F00_0000) != 0x0A00_0000 {
        return None;
    }

    let sf = ((word >> 31) & 1) != 0;
    let size = if sf { 8 } else { 4 };
    let opc = (word >> 29) & 0x3;
    let shift = (word >> 22) & 0x3;
    if shift == 0x3 {
        return None;
    }
    let negate_rhs = ((word >> 21) & 1) != 0;
    let rm = (word >> 16) & 0x1F;
    let imm6 = (word >> 10) & 0x3F;
    if !sf && (imm6 & 0x20) != 0 {
        return None;
    }
    let rn = (word >> 5) & 0x1F;
    let rd = word & 0x1F;

    let mut ops = Vec::new();
    let lhs = if rn == 31 {
        const_u64(0, size)
    } else {
        a64_reg(rn, size)
    };
    let mut rhs = if rm == 31 {
        const_u64(0, size)
    } else {
        a64_reg(rm, size)
    };

    if negate_rhs {
        let not_rhs = temp.alloc(size);
        ops.push(PcodeOp {
            seq_num: {
                let s = *seq;
                *seq = seq.saturating_add(1);
                s
            },
            opcode: PcodeOpcode::IntNegate,
            address,
            output: Some(not_rhs.clone()),
            inputs: vec![rhs],
            asm_mnemonic: Some("LOG_NOT_RHS".to_string()),
        });
        rhs = not_rhs;
    }

    if shift != 0 || imm6 != 0 {
        let shifted = temp.alloc(size);
        let shift_op = match shift {
            0 => PcodeOpcode::IntLeft,
            1 => PcodeOpcode::IntRight,
            2 => PcodeOpcode::IntSRight,
            _ => return None,
        };
        ops.push(PcodeOp {
            seq_num: {
                let s = *seq;
                *seq = seq.saturating_add(1);
                s
            },
            opcode: shift_op,
            address,
            output: Some(shifted.clone()),
            inputs: vec![rhs, const_u64(u64::from(imm6), 4)],
            asm_mnemonic: Some("LOG_SHIFT".to_string()),
        });
        rhs = shifted;
    }

    let set_flags = opc == 0x3;
    let is_tst_alias = set_flags && !negate_rhs && rd == 31;
    let result = if rd == 31 {
        temp.alloc(size)
    } else {
        a64_reg(rd, size)
    };
    let op = match opc {
        0x0 => PcodeOpcode::IntAnd,
        0x1 => PcodeOpcode::IntOr,
        0x2 => PcodeOpcode::IntXor,
        0x3 => PcodeOpcode::IntAnd,
        _ => return None,
    };
    let mnemonic = match (opc, negate_rhs, is_tst_alias) {
        (0x0, false, _) => "AND",
        (0x0, true, _) => "BIC",
        (0x1, false, _) => "ORR",
        (0x1, true, _) => "ORN",
        (0x2, false, _) => "EOR",
        (0x2, true, _) => "EON",
        (0x3, false, true) => "TST",
        (0x3, false, false) => "ANDS",
        (0x3, true, _) => "BICS",
        _ => return None,
    };

    ops.push(PcodeOp {
        seq_num: {
            let s = *seq;
            *seq = seq.saturating_add(1);
            s
        },
        opcode: op,
        address,
        output: Some(result.clone()),
        inputs: vec![lhs, rhs],
        asm_mnemonic: Some(mnemonic.to_string()),
    });

    if set_flags {
        emit_nzcv_from_logic(&mut ops, address, result, temp, seq);
    }

    Some(ops)
}

pub(super) fn decode_logical_imm(
    word: u32,
    address: u64,
    temp: &mut A64TempFactory,
    seq: &mut u32,
) -> Option<Vec<PcodeOp>> {
    if (word & 0x1F80_0000) != 0x1200_0000 {
        return None;
    }

    let sf = ((word >> 31) & 1) != 0;
    let size = if sf { 8 } else { 4 };
    let opc = (word >> 29) & 0x3;
    let n = (word >> 22) & 1;
    let immr = (word >> 16) & 0x3F;
    let imms = (word >> 10) & 0x3F;
    let rn = (word >> 5) & 0x1F;
    let rd = word & 0x1F;

    let imm = decode_logical_imm_mask(sf, n, immr, imms)?;
    let lhs = if rn == 31 {
        const_u64(0, size)
    } else {
        a64_reg(rn, size)
    };
    let rhs = const_u64(imm, size);

    let set_flags = opc == 0x3;
    let is_tst_alias = set_flags && rd == 31;
    let is_mov_alias = opc == 0x1 && rn == 31 && immr == 0;
    let result = if is_tst_alias {
        temp.alloc(size)
    } else {
        a64_reg(rd, size)
    };
    let op = match opc {
        0x0 => PcodeOpcode::IntAnd,
        0x1 => PcodeOpcode::IntOr,
        0x2 => PcodeOpcode::IntXor,
        0x3 => PcodeOpcode::IntAnd,
        _ => return None,
    };
    let mnemonic = match (opc, is_tst_alias) {
        (0x0, _) => "ANDI",
        (0x1, false) if is_mov_alias => "MOV",
        (0x1, _) => "ORRI",
        (0x2, _) => "EORI",
        (0x3, true) => "TSTI",
        (0x3, false) => "ANDSI",
        _ => return None,
    };

    let mut ops = Vec::new();
    if is_mov_alias {
        ops.push(PcodeOp {
            seq_num: {
                let s = *seq;
                *seq = seq.saturating_add(1);
                s
            },
            opcode: PcodeOpcode::Copy,
            address,
            output: Some(result.clone()),
            inputs: vec![rhs],
            asm_mnemonic: Some(mnemonic.to_string()),
        });
    } else {
        ops.push(PcodeOp {
            seq_num: {
                let s = *seq;
                *seq = seq.saturating_add(1);
                s
            },
            opcode: op,
            address,
            output: Some(result.clone()),
            inputs: vec![lhs, rhs],
            asm_mnemonic: Some(mnemonic.to_string()),
        });
    }

    if set_flags {
        emit_nzcv_from_logic(&mut ops, address, result, temp, seq);
    }

    Some(ops)
}

fn decode_logical_imm_mask(sf: bool, n: u32, immr: u32, imms: u32) -> Option<u64> {
    if !sf && n != 0 {
        return None;
    }

    let immn_imms = (n << 6) | ((!imms) & 0x3F);
    let len = highest_set_bit(immn_imms)?;
    if len == 0 {
        return None;
    }

    let levels = (1u32 << len) - 1;
    if (imms & levels) == levels {
        return None;
    }

    let s = imms & levels;
    let r = immr & levels;
    let esize = 1u32 << len;

    let ones_len = s + 1;
    let base = ones_mask(ones_len);
    let rotated = ror_by_size(base, r, esize);
    let m = if sf { 64 } else { 32 };
    Some(replicate_by_size(rotated, esize, m))
}

fn highest_set_bit(v: u32) -> Option<u32> {
    if v == 0 {
        None
    } else {
        Some(31 - v.leading_zeros())
    }
}

fn ones_mask(width: u32) -> u64 {
    if width >= 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    }
}

fn ror_by_size(value: u64, rot: u32, size: u32) -> u64 {
    let mask = ones_mask(size);
    let v = value & mask;
    if size == 64 {
        v.rotate_right(rot & 63)
    } else {
        let r = rot % size;
        if r == 0 {
            v
        } else {
            ((v >> r) | (v << (size - r))) & mask
        }
    }
}

fn replicate_by_size(pattern: u64, size: u32, total: u32) -> u64 {
    if size == 0 || total % size != 0 || total > 64 {
        return 0;
    }
    let mut out = 0u64;
    let count = total / size;
    let p = pattern & ones_mask(size);
    for i in 0..count {
        out |= p << (i * size);
    }
    out
}

fn emit_nzcv_from_logic(
    ops: &mut Vec<PcodeOp>,
    address: u64,
    result: Varnode,
    temp: &mut A64TempFactory,
    seq: &mut u32,
) {
    let n_val = temp.alloc(1);
    let z_val = temp.alloc(1);

    ops.push(PcodeOp {
        seq_num: {
            let s = *seq;
            *seq = seq.saturating_add(1);
            s
        },
        opcode: PcodeOpcode::IntSLess,
        address,
        output: Some(n_val.clone()),
        inputs: vec![result.clone(), const_u64(0, result.size)],
        asm_mnemonic: Some("SET_N".to_string()),
    });

    ops.push(PcodeOp {
        seq_num: {
            let s = *seq;
            *seq = seq.saturating_add(1);
            s
        },
        opcode: PcodeOpcode::IntEqual,
        address,
        output: Some(z_val.clone()),
        inputs: vec![result.clone(), const_u64(0, result.size)],
        asm_mnemonic: Some("SET_Z".to_string()),
    });

    for (flag, val, tag) in [
        (a64_flag_n(), n_val, "WRITE_N"),
        (a64_flag_z(), z_val, "WRITE_Z"),
        (a64_flag_c(), const_u64(0, 1), "WRITE_C0"),
        (a64_flag_v(), const_u64(0, 1), "WRITE_V0"),
    ] {
        ops.push(PcodeOp {
            seq_num: {
                let s = *seq;
                *seq = seq.saturating_add(1);
                s
            },
            opcode: PcodeOpcode::Copy,
            address,
            output: Some(flag),
            inputs: vec![val],
            asm_mnemonic: Some(tag.to_string()),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::common::A64_NZCV_BASE;
    use super::*;

    fn decode_shifted(word: u32) -> Vec<PcodeOp> {
        let mut temp = A64TempFactory::new(0x1000);
        let mut seq = 1u32;
        decode_logical_shifted_reg(word, 0x1000, &mut temp, &mut seq)
            .expect("expected logical shifted-register decode")
    }

    fn decode_imm(word: u32) -> Vec<PcodeOp> {
        let mut temp = A64TempFactory::new(0x1000);
        let mut seq = 1u32;
        decode_logical_imm(word, 0x1000, &mut temp, &mut seq)
            .expect("expected logical immediate decode")
    }

    #[test]
    fn decode_tst_shifted_alias_updates_nzcv_with_zero_cv() {
        // TST W0, W1  == ANDS WZR, W0, W1
        let ops = decode_shifted(0x6A01_001F);
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntAnd));

        let write_c0 = ops.iter().find(|op| {
            op.opcode == PcodeOpcode::Copy
                && op.output.as_ref().map(|o| o.offset) == Some(A64_NZCV_BASE + 2)
                && op.inputs.len() == 1
                && op.inputs[0].is_constant
                && op.inputs[0].constant_val == 0
        });
        assert!(write_c0.is_some());

        let write_v0 = ops.iter().find(|op| {
            op.opcode == PcodeOpcode::Copy
                && op.output.as_ref().map(|o| o.offset) == Some(A64_NZCV_BASE + 3)
                && op.inputs.len() == 1
                && op.inputs[0].is_constant
                && op.inputs[0].constant_val == 0
        });
        assert!(write_v0.is_some());
    }

    #[test]
    fn decode_and_shifted_register_without_flag_writes() {
        // AND W2, W0, W1
        let ops = decode_shifted(0x0A01_0002);
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntAnd));
        assert!(ops.iter().all(|op| {
            op.output
                .as_ref()
                .map(|o| !(A64_NZCV_BASE..A64_NZCV_BASE + 4).contains(&o.offset))
                .unwrap_or(true)
        }));
    }

    #[test]
    fn decode_orri_immediate_constant_mask() {
        // ORR W0, W1, #1
        let ops = decode_imm(0x3200_0020);
        let op = ops
            .iter()
            .find(|op| op.opcode == PcodeOpcode::IntOr)
            .expect("missing ORR-immediate op");
        assert_eq!(op.inputs.len(), 2);
        assert!(op.inputs[1].is_constant);
        assert_eq!(op.inputs[1].constant_val, 1);
    }

    #[test]
    fn decode_tsti_alias_sets_zero_cv() {
        // TST W0, #1 == ANDS WZR, W0, #1
        let ops = decode_imm(0x7200_001f);
        assert!(ops
            .iter()
            .any(|op| op.asm_mnemonic.as_deref() == Some("TSTI")));

        let write_c0 = ops.iter().find(|op| {
            op.opcode == PcodeOpcode::Copy
                && op.output.as_ref().map(|o| o.offset) == Some(A64_NZCV_BASE + 2)
                && op.inputs.len() == 1
                && op.inputs[0].is_constant
                && op.inputs[0].constant_val == 0
        });
        assert!(write_c0.is_some());

        let write_v0 = ops.iter().find(|op| {
            op.opcode == PcodeOpcode::Copy
                && op.output.as_ref().map(|o| o.offset) == Some(A64_NZCV_BASE + 3)
                && op.inputs.len() == 1
                && op.inputs[0].is_constant
                && op.inputs[0].constant_val == 0
        });
        assert!(write_v0.is_some());
    }

    #[test]
    fn decode_mov_bitmask_alias_copy_form() {
        // MOV W0, #0x00010001 (ORR immediate alias form)
        let ops = decode_imm(0x3200_03e0);
        let mov = ops
            .iter()
            .find(|op| op.asm_mnemonic.as_deref() == Some("MOV"))
            .expect("missing MOV alias op");
        assert_eq!(mov.opcode, PcodeOpcode::Copy);
        assert_eq!(mov.inputs.len(), 1);
        assert!(mov.inputs[0].is_constant);

        assert!(ops.iter().all(|op| {
            op.asm_mnemonic.as_deref() != Some("ORRI") && op.opcode != PcodeOpcode::IntOr
        }));
    }
}
