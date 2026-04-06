use super::*;
use super::super::super::predicate::emit_jcc_predicate_with_allocator;

pub(super) fn decode_setcc(
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

pub(super) fn decode_cmovcc(
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

    let merged = emit_conditional_value_merge(
        &mut ops,
        address,
        size,
        src,
        dst.clone(),
        &pred,
        temp,
        seq,
        "CMOVcc",
    );
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

pub(super) fn emit_conditional_value_merge(
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
