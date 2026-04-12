use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use super::super::common::{
    a64_flag_c, a64_flag_n, a64_flag_v, a64_flag_z, a64_reg, const_u64, A64TempFactory,
};

pub(super) fn decode_add_sub_imm(
    word: u32,
    address: u64,
    temp: &mut A64TempFactory,
    seq: &mut u32,
) -> Option<Vec<PcodeOp>> {
    if (word & 0x1F00_0000) != 0x1100_0000 {
        return None;
    }

    let sf = ((word >> 31) & 1) != 0;
    let size = if sf { 8 } else { 4 };
    let is_sub = ((word >> 30) & 1) != 0;
    let set_flags = ((word >> 29) & 1) != 0;
    let shift = (word >> 22) & 0x3;
    if shift > 1 {
        return None;
    }

    let imm12 = ((word >> 10) & 0x0FFF) as u64;
    let imm = if shift == 1 { imm12 << 12 } else { imm12 };
    let rn = (word >> 5) & 0x1F;
    let rd = word & 0x1F;
    let is_cmp_alias = set_flags && rd == 31;

    let lhs = a64_reg(rn, size);
    let rhs = const_u64(imm, size);
    let result = if is_cmp_alias {
        temp.alloc(size)
    } else {
        a64_reg(rd, size)
    };
    let mut ops = Vec::new();
    ops.push(PcodeOp {
        seq_num: {
            let s = *seq;
            *seq = seq.saturating_add(1);
            s
        },
        opcode: if is_sub {
            PcodeOpcode::IntSub
        } else {
            PcodeOpcode::IntAdd
        },
        address,
        output: Some(result.clone()),
        inputs: vec![lhs.clone(), rhs.clone()],
        asm_mnemonic: Some(if is_sub {
            if is_cmp_alias {
                "CMPI".to_string()
            } else if set_flags {
                "SUBSI".to_string()
            } else {
                "SUBI".to_string()
            }
        } else if is_cmp_alias {
            "CMNI".to_string()
        } else if set_flags {
            "ADDSI".to_string()
        } else {
            "ADDI".to_string()
        }),
    });

    if set_flags {
        emit_nzcv_from_arith(&mut ops, address, lhs, rhs, result, is_sub, temp, seq);
    }

    Some(ops)
}

pub(super) fn decode_add_sub_reg(
    word: u32,
    address: u64,
    temp: &mut A64TempFactory,
    seq: &mut u32,
) -> Option<Vec<PcodeOp>> {
    if (word & 0x1F20_0000) != 0x0B00_0000 {
        return None;
    }

    let sf = ((word >> 31) & 1) != 0;
    let size = if sf { 8 } else { 4 };
    let is_sub = ((word >> 30) & 1) != 0;
    let set_flags = ((word >> 29) & 1) != 0;
    let shift = (word >> 22) & 0x3;
    let imm6 = (word >> 10) & 0x3F;
    let rm = (word >> 16) & 0x1F;
    let rn = (word >> 5) & 0x1F;
    let rd = word & 0x1F;
    let is_cmp_alias = set_flags && rd == 31;

    let mut ops = Vec::new();
    let lhs = a64_reg(rn, size);
    let mut rhs = a64_reg(rm, size);
    if imm6 != 0 {
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
            asm_mnemonic: Some("SHIFT".to_string()),
        });
        rhs = shifted;
    } else if shift != 0 {
        return None;
    }
    let result = if is_cmp_alias {
        temp.alloc(size)
    } else {
        a64_reg(rd, size)
    };

    ops.push(PcodeOp {
        seq_num: {
            let s = *seq;
            *seq = seq.saturating_add(1);
            s
        },
        opcode: if is_sub {
            PcodeOpcode::IntSub
        } else {
            PcodeOpcode::IntAdd
        },
        address,
        output: Some(result.clone()),
        inputs: vec![lhs.clone(), rhs.clone()],
        asm_mnemonic: Some(if is_sub {
            if is_cmp_alias {
                "CMP".to_string()
            } else if set_flags {
                "SUBS".to_string()
            } else {
                "SUB".to_string()
            }
        } else {
            if is_cmp_alias {
                "CMN".to_string()
            } else if set_flags {
                "ADDS".to_string()
            } else {
                "ADD".to_string()
            }
        }),
    });

    if set_flags {
        emit_nzcv_from_arith(&mut ops, address, lhs, rhs, result, is_sub, temp, seq);
    }

    Some(ops)
}

fn emit_nzcv_from_arith(
    ops: &mut Vec<PcodeOp>,
    address: u64,
    lhs: Varnode,
    rhs: Varnode,
    result: Varnode,
    is_sub: bool,
    temp: &mut A64TempFactory,
    seq: &mut u32,
) {
    let n_val = temp.alloc(1);
    let z_val = temp.alloc(1);
    let c_val = temp.alloc(1);
    let v_val = temp.alloc(1);

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

    if is_sub {
        let borrow = temp.alloc(1);
        ops.push(PcodeOp {
            seq_num: {
                let s = *seq;
                *seq = seq.saturating_add(1);
                s
            },
            opcode: PcodeOpcode::IntLess,
            address,
            output: Some(borrow.clone()),
            inputs: vec![lhs.clone(), rhs.clone()],
            asm_mnemonic: Some("SET_BORROW".to_string()),
        });
        ops.push(PcodeOp {
            seq_num: {
                let s = *seq;
                *seq = seq.saturating_add(1);
                s
            },
            opcode: PcodeOpcode::BoolNegate,
            address,
            output: Some(c_val.clone()),
            inputs: vec![borrow],
            asm_mnemonic: Some("SET_C".to_string()),
        });
        ops.push(PcodeOp {
            seq_num: {
                let s = *seq;
                *seq = seq.saturating_add(1);
                s
            },
            opcode: PcodeOpcode::IntSBorrow,
            address,
            output: Some(v_val.clone()),
            inputs: vec![lhs.clone(), rhs.clone()],
            asm_mnemonic: Some("SET_V".to_string()),
        });
    } else {
        ops.push(PcodeOp {
            seq_num: {
                let s = *seq;
                *seq = seq.saturating_add(1);
                s
            },
            opcode: PcodeOpcode::IntCarry,
            address,
            output: Some(c_val.clone()),
            inputs: vec![lhs.clone(), rhs.clone()],
            asm_mnemonic: Some("SET_C".to_string()),
        });
        ops.push(PcodeOp {
            seq_num: {
                let s = *seq;
                *seq = seq.saturating_add(1);
                s
            },
            opcode: PcodeOpcode::IntSCarry,
            address,
            output: Some(v_val.clone()),
            inputs: vec![lhs.clone(), rhs.clone()],
            asm_mnemonic: Some("SET_V".to_string()),
        });
    }

    for (flag, val, tag) in [
        (a64_flag_n(), n_val, "WRITE_N"),
        (a64_flag_z(), z_val, "WRITE_Z"),
        (a64_flag_c(), c_val, "WRITE_C"),
        (a64_flag_v(), v_val, "WRITE_V"),
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

    #[test]
    fn decode_addi_without_flag_writes() {
        // ADDI W0, W1, #1
        let word = 0x1100_0420;
        let mut temp = A64TempFactory::new(0x1000);
        let mut seq = 1u32;
        let ops =
            decode_add_sub_imm(word, 0x1000, &mut temp, &mut seq).expect("expected ADDI decode");

        assert!(ops.iter().any(
            |op| op.opcode == PcodeOpcode::IntAdd && op.asm_mnemonic.as_deref() == Some("ADDI")
        ));
        assert!(ops.iter().all(|op| {
            op.output
                .as_ref()
                .map(|o| !(A64_NZCV_BASE..A64_NZCV_BASE + 4).contains(&o.offset))
                .unwrap_or(true)
        }));
    }

    #[test]
    fn decode_cmp_register_alias_writes_nzcv() {
        // CMP W0, W1 == SUBS WZR, W0, W1
        let word = 0x6B01_001F;
        let mut temp = A64TempFactory::new(0x1000);
        let mut seq = 1u32;
        let ops =
            decode_add_sub_reg(word, 0x1000, &mut temp, &mut seq).expect("expected CMP decode");

        assert!(ops.iter().any(
            |op| op.opcode == PcodeOpcode::IntSub && op.asm_mnemonic.as_deref() == Some("CMP")
        ));
        for flag in A64_NZCV_BASE..A64_NZCV_BASE + 4 {
            assert!(ops.iter().any(|op| {
                op.opcode == PcodeOpcode::Copy && op.output.as_ref().map(|o| o.offset) == Some(flag)
            }));
        }
    }
}
