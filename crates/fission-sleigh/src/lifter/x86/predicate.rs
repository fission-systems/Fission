use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use super::super::common::{x86_flag_cf, x86_flag_of, x86_flag_pf, x86_flag_sf, x86_flag_zf};

pub(super) fn emit_jcc_predicate_with_allocator<F>(
    ops: &mut Vec<PcodeOp>,
    address: u64,
    cond: u8,
    seq: &mut u32,
    alloc_tmp: &mut F,
) -> Option<Varnode>
where
    F: FnMut(u32) -> Varnode,
{
    fn next_seq(seq: &mut u32) -> u32 {
        let cur = *seq;
        *seq = seq.saturating_add(1);
        cur
    }

    fn bool_not<F>(
        ops: &mut Vec<PcodeOp>,
        address: u64,
        input: Varnode,
        tag: &str,
        seq: &mut u32,
        alloc_tmp: &mut F,
    ) -> Varnode
    where
        F: FnMut(u32) -> Varnode,
    {
        let out = alloc_tmp(1);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::BoolNegate,
            address,
            output: Some(out.clone()),
            inputs: vec![input],
            asm_mnemonic: Some(tag.to_string()),
        });
        out
    }

    fn bool_and<F>(
        ops: &mut Vec<PcodeOp>,
        address: u64,
        lhs: Varnode,
        rhs: Varnode,
        tag: &str,
        seq: &mut u32,
        alloc_tmp: &mut F,
    ) -> Varnode
    where
        F: FnMut(u32) -> Varnode,
    {
        let out = alloc_tmp(1);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::BoolAnd,
            address,
            output: Some(out.clone()),
            inputs: vec![lhs, rhs],
            asm_mnemonic: Some(tag.to_string()),
        });
        out
    }

    fn bool_or<F>(
        ops: &mut Vec<PcodeOp>,
        address: u64,
        lhs: Varnode,
        rhs: Varnode,
        tag: &str,
        seq: &mut u32,
        alloc_tmp: &mut F,
    ) -> Varnode
    where
        F: FnMut(u32) -> Varnode,
    {
        let out = alloc_tmp(1);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::BoolOr,
            address,
            output: Some(out.clone()),
            inputs: vec![lhs, rhs],
            asm_mnemonic: Some(tag.to_string()),
        });
        out
    }

    fn bool_eq<F>(
        ops: &mut Vec<PcodeOp>,
        address: u64,
        lhs: Varnode,
        rhs: Varnode,
        tag: &str,
        seq: &mut u32,
        alloc_tmp: &mut F,
    ) -> Varnode
    where
        F: FnMut(u32) -> Varnode,
    {
        let out = alloc_tmp(1);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntEqual,
            address,
            output: Some(out.clone()),
            inputs: vec![lhs, rhs],
            asm_mnemonic: Some(tag.to_string()),
        });
        out
    }

    fn bool_ne<F>(
        ops: &mut Vec<PcodeOp>,
        address: u64,
        lhs: Varnode,
        rhs: Varnode,
        tag: &str,
        seq: &mut u32,
        alloc_tmp: &mut F,
    ) -> Varnode
    where
        F: FnMut(u32) -> Varnode,
    {
        let out = alloc_tmp(1);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntNotEqual,
            address,
            output: Some(out.clone()),
            inputs: vec![lhs, rhs],
            asm_mnemonic: Some(tag.to_string()),
        });
        out
    }

    let cf = x86_flag_cf();
    let pf = x86_flag_pf();
    let zf = x86_flag_zf();
    let sf = x86_flag_sf();
    let of = x86_flag_of();

    Some(match cond {
        0x0 => of,
        0x1 => bool_not(ops, address, of, "JNO_PRED", seq, alloc_tmp),
        0x2 => cf,
        0x3 => bool_not(ops, address, cf, "JAE_PRED", seq, alloc_tmp),
        0x4 => zf,
        0x5 => bool_not(ops, address, zf, "JNE_PRED", seq, alloc_tmp),
        0x6 => bool_or(ops, address, cf, zf, "JBE_PRED", seq, alloc_tmp),
        0x7 => {
            let ncf = bool_not(ops, address, cf, "JA_NCF", seq, alloc_tmp);
            let nzf = bool_not(ops, address, zf, "JA_NZF", seq, alloc_tmp);
            bool_and(ops, address, ncf, nzf, "JA_PRED", seq, alloc_tmp)
        }
        0x8 => sf,
        0x9 => bool_not(ops, address, sf, "JNS_PRED", seq, alloc_tmp),
        0xA => pf,
        0xB => bool_not(ops, address, pf, "JNP_PRED", seq, alloc_tmp),
        0xC => bool_ne(ops, address, sf, of, "JL_PRED", seq, alloc_tmp),
        0xD => bool_eq(ops, address, sf, of, "JGE_PRED", seq, alloc_tmp),
        0xE => {
            let lt = bool_ne(ops, address, sf, of, "JLE_LT_CORE", seq, alloc_tmp);
            bool_or(ops, address, zf, lt, "JLE_PRED", seq, alloc_tmp)
        }
        0xF => {
            let ge = bool_eq(ops, address, sf, of, "JG_GE_CORE", seq, alloc_tmp);
            let nz = bool_not(ops, address, zf, "JG_NZ", seq, alloc_tmp);
            bool_and(ops, address, ge, nz, "JG_PRED", seq, alloc_tmp)
        }
        _ => return None,
    })
}