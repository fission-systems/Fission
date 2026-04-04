use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use super::super::common::{
    x86_flag_cf, x86_flag_of, x86_flag_pf, x86_flag_sf, x86_flag_zf, UNIQUE_SPACE_ID,
};

pub(crate) fn decode_control(insn: &[u8], address: u64, decoded_len: u64) -> Option<Vec<PcodeOp>> {
    let (op_idx, op, ext) = opcode_window(insn)?;
    let next = address.wrapping_add(decoded_len);

    match (op, ext) {
        (0xC3, _) | (0xCB, _) | (0xC2, _) | (0xCA, _) => Some(vec![PcodeOp {
            seq_num: 1,
            opcode: PcodeOpcode::Return,
            address,
            output: None,
            inputs: Vec::new(),
            asm_mnemonic: Some("RET".to_string()),
        }]),
        (0xE8, _) => {
            if op_idx + 5 > insn.len() {
                return None;
            }
            let rel = i32::from_le_bytes([
                insn[op_idx + 1],
                insn[op_idx + 2],
                insn[op_idx + 3],
                insn[op_idx + 4],
            ]);
            let target = next.wrapping_add_signed(rel as i64);
            Some(vec![PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::Call,
                address,
                output: None,
                inputs: vec![Varnode::constant(target as i64, 8)],
                asm_mnemonic: Some("CALL".to_string()),
            }])
        }
        (0xE9, _) => {
            if op_idx + 5 > insn.len() {
                return None;
            }
            let rel = i32::from_le_bytes([
                insn[op_idx + 1],
                insn[op_idx + 2],
                insn[op_idx + 3],
                insn[op_idx + 4],
            ]);
            let target = next.wrapping_add_signed(rel as i64);
            Some(vec![PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::Branch,
                address,
                output: None,
                inputs: vec![Varnode::constant(target as i64, 8)],
                asm_mnemonic: Some("JMP".to_string()),
            }])
        }
        (0xEB, _) => {
            if op_idx + 2 > insn.len() {
                return None;
            }
            let rel = insn[op_idx + 1] as i8;
            let target = next.wrapping_add_signed(rel as i64);
            Some(vec![PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::Branch,
                address,
                output: None,
                inputs: vec![Varnode::constant(target as i64, 8)],
                asm_mnemonic: Some("JMP".to_string()),
            }])
        }
        (0x70..=0x7F, _) => {
            if op_idx + 2 > insn.len() {
                return None;
            }
            let rel = insn[op_idx + 1] as i8;
            let target = next.wrapping_add_signed(rel as i64);
            build_jcc_ops(address, target, op & 0x0F)
        }
        (0x0F, Some(0x80..=0x8F)) => {
            if op_idx + 6 > insn.len() {
                return None;
            }
            let rel = i32::from_le_bytes([
                insn[op_idx + 2],
                insn[op_idx + 3],
                insn[op_idx + 4],
                insn[op_idx + 5],
            ]);
            let target = next.wrapping_add_signed(rel as i64);
            build_jcc_ops(address, target, ext? & 0x0F)
        }
        _ => None,
    }
}

fn build_jcc_ops(address: u64, target: u64, cond: u8) -> Option<Vec<PcodeOp>> {
    let mut seq = 1u32;
    let mut tmp = ctrl_tmp_base(address);
    let mut ops = Vec::new();
    let pred = emit_jcc_predicate(&mut ops, address, cond, &mut seq, &mut tmp)?;
    ops.push(PcodeOp {
        seq_num: next_seq(&mut seq),
        opcode: PcodeOpcode::CBranch,
        address,
        output: None,
        inputs: vec![Varnode::constant(target as i64, 8), pred],
        asm_mnemonic: Some("Jcc".to_string()),
    });
    Some(ops)
}

fn next_seq(seq: &mut u32) -> u32 {
    let cur = *seq;
    *seq = seq.saturating_add(1);
    cur
}

fn ctrl_tmp_base(address: u64) -> u64 {
    0xE000_0000_0000_0000u64.wrapping_add(address.wrapping_shl(6))
}

fn alloc_ctrl_tmp(next: &mut u64, size: u32) -> Varnode {
    let vn = Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: *next,
        size,
        is_constant: false,
        constant_val: 0,
    };
    *next = next.wrapping_add(8);
    vn
}

fn emit_jcc_predicate(
    ops: &mut Vec<PcodeOp>,
    address: u64,
    cond: u8,
    seq: &mut u32,
    tmp: &mut u64,
) -> Option<Varnode> {
    let cf = x86_flag_cf();
    let pf = x86_flag_pf();
    let zf = x86_flag_zf();
    let sf = x86_flag_sf();
    let of = x86_flag_of();

    let bool_not = |ops: &mut Vec<PcodeOp>, input: Varnode, tag: &str, seq: &mut u32, tmp: &mut u64| {
        let out = alloc_ctrl_tmp(tmp, 1);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::BoolNegate,
            address,
            output: Some(out.clone()),
            inputs: vec![input],
            asm_mnemonic: Some(tag.to_string()),
        });
        out
    };
    let bool_and = |ops: &mut Vec<PcodeOp>, lhs: Varnode, rhs: Varnode, tag: &str, seq: &mut u32, tmp: &mut u64| {
        let out = alloc_ctrl_tmp(tmp, 1);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::BoolAnd,
            address,
            output: Some(out.clone()),
            inputs: vec![lhs, rhs],
            asm_mnemonic: Some(tag.to_string()),
        });
        out
    };
    let bool_or = |ops: &mut Vec<PcodeOp>, lhs: Varnode, rhs: Varnode, tag: &str, seq: &mut u32, tmp: &mut u64| {
        let out = alloc_ctrl_tmp(tmp, 1);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::BoolOr,
            address,
            output: Some(out.clone()),
            inputs: vec![lhs, rhs],
            asm_mnemonic: Some(tag.to_string()),
        });
        out
    };
    let bool_eq = |ops: &mut Vec<PcodeOp>, lhs: Varnode, rhs: Varnode, tag: &str, seq: &mut u32, tmp: &mut u64| {
        let out = alloc_ctrl_tmp(tmp, 1);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntEqual,
            address,
            output: Some(out.clone()),
            inputs: vec![lhs, rhs],
            asm_mnemonic: Some(tag.to_string()),
        });
        out
    };
    let bool_ne = |ops: &mut Vec<PcodeOp>, lhs: Varnode, rhs: Varnode, tag: &str, seq: &mut u32, tmp: &mut u64| {
        let out = alloc_ctrl_tmp(tmp, 1);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntNotEqual,
            address,
            output: Some(out.clone()),
            inputs: vec![lhs, rhs],
            asm_mnemonic: Some(tag.to_string()),
        });
        out
    };

    Some(match cond {
        0x0 => of,
        0x1 => bool_not(ops, of, "JNO_PRED", seq, tmp),
        0x2 => cf,
        0x3 => bool_not(ops, cf, "JAE_PRED", seq, tmp),
        0x4 => zf,
        0x5 => bool_not(ops, zf, "JNE_PRED", seq, tmp),
        0x6 => bool_or(ops, cf, zf, "JBE_PRED", seq, tmp),
        0x7 => {
            let ncf = bool_not(ops, cf, "JA_NCF", seq, tmp);
            let nzf = bool_not(ops, zf, "JA_NZF", seq, tmp);
            bool_and(ops, ncf, nzf, "JA_PRED", seq, tmp)
        }
        0x8 => sf,
        0x9 => bool_not(ops, sf, "JNS_PRED", seq, tmp),
        0xA => pf,
        0xB => bool_not(ops, pf, "JNP_PRED", seq, tmp),
        0xC => bool_ne(ops, sf, of, "JL_PRED", seq, tmp),
        0xD => bool_eq(ops, sf, of, "JGE_PRED", seq, tmp),
        0xE => {
            let lt = bool_ne(ops, sf, of, "JLE_LT_CORE", seq, tmp);
            bool_or(ops, zf, lt, "JLE_PRED", seq, tmp)
        }
        0xF => {
            let ge = bool_eq(ops, sf, of, "JG_GE_CORE", seq, tmp);
            let nz = bool_not(ops, zf, "JG_NZ", seq, tmp);
            bool_and(ops, ge, nz, "JG_PRED", seq, tmp)
        }
        _ => return None,
    })
}

fn opcode_window(insn: &[u8]) -> Option<(usize, u8, Option<u8>)> {
    if insn.is_empty() {
        return None;
    }

    let mut idx = 0usize;
    while idx < insn.len() && is_prefix(insn[idx]) {
        idx += 1;
    }
    if idx >= insn.len() {
        return None;
    }

    let op = insn[idx];
    if op == 0x0F {
        let ext = insn.get(idx + 1).copied();
        Some((idx, op, ext))
    } else {
        Some((idx, op, None))
    }
}

fn is_prefix(byte: u8) -> bool {
    matches!(
        byte,
        0xF0
            | 0xF2
            | 0xF3
            | 0x2E
            | 0x36
            | 0x3E
            | 0x26
            | 0x64
            | 0x65
            | 0x66
            | 0x67
            | 0x40..=0x4F
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(insn: &[u8], address: u64) -> Vec<PcodeOp> {
        decode_control(insn, address, insn.len() as u64).expect("expected x86 control decode")
    }

    fn is_flag(vn: &Varnode, expected: Varnode) -> bool {
        vn.space_id == expected.space_id && vn.offset == expected.offset && vn.size == expected.size
    }

    #[test]
    fn decode_ret_opcode() {
        let ops = decode(&[0xC3], 0x1000);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].opcode, PcodeOpcode::Return);
        assert_eq!(ops[0].asm_mnemonic.as_deref(), Some("RET"));
    }

    #[test]
    fn decode_short_jo_uses_of_flag_directly() {
        let address = 0x2000u64;
        let ops = decode(&[0x70, 0x05], address);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].opcode, PcodeOpcode::CBranch);
        assert_eq!(ops[0].inputs[0].constant_val as u64, address + 2 + 5);
        assert!(is_flag(&ops[0].inputs[1], x86_flag_of()));
    }

    #[test]
    fn decode_short_jne_with_prefix_builds_negated_zf_predicate() {
        let address = 0x3000u64;
        let ops = decode(&[0x66, 0x75, 0x02], address);
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].opcode, PcodeOpcode::BoolNegate);
        assert_eq!(ops[1].opcode, PcodeOpcode::CBranch);
        assert!(is_flag(&ops[0].inputs[0], x86_flag_zf()));
        assert_eq!(ops[1].inputs[0].constant_val as u64, address + 3 + 2);
        assert_eq!(
            ops[1].inputs[1].offset,
            ops[0].output.as_ref().expect("negated zf output").offset
        );
    }

    #[test]
    fn decode_near_jbe_is_cf_or_zf() {
        let address = 0x4000u64;
        let ops = decode(&[0x0F, 0x86, 0x10, 0x00, 0x00, 0x00], address);
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].opcode, PcodeOpcode::BoolOr);
        assert_eq!(ops[1].opcode, PcodeOpcode::CBranch);
        assert!(ops[0].inputs.iter().any(|vn| is_flag(vn, x86_flag_cf())));
        assert!(ops[0].inputs.iter().any(|vn| is_flag(vn, x86_flag_zf())));
        assert_eq!(ops[1].inputs[0].constant_val as u64, address + 6 + 0x10);
    }

    #[test]
    fn decode_near_jg_matches_nz_and_sf_eq_of_shape() {
        let ops = decode(&[0x0F, 0x8F, 0x01, 0x00, 0x00, 0x00], 0x5000);
        assert_eq!(ops.len(), 4);
        assert_eq!(ops[0].opcode, PcodeOpcode::IntEqual);
        assert_eq!(ops[1].opcode, PcodeOpcode::BoolNegate);
        assert_eq!(ops[2].opcode, PcodeOpcode::BoolAnd);
        assert_eq!(ops[3].opcode, PcodeOpcode::CBranch);
        assert!(is_flag(&ops[0].inputs[0], x86_flag_sf()));
        assert!(is_flag(&ops[0].inputs[1], x86_flag_of()));
        assert!(is_flag(&ops[1].inputs[0], x86_flag_zf()));
    }

    #[test]
    fn decode_prefix_only_is_rejected() {
        assert!(decode_control(&[0x66, 0x67], 0x6000, 2).is_none());
    }
}
