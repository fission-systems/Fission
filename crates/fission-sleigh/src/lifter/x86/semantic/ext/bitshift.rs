use super::*;

pub(super) fn decode_shld_shrd(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    ext: u8,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded = match decode_modrm_operand(insn, op_idx + 1, prefix, size, address, temp, &mut ops, seq) {
        Some(v) => v,
        None => return Vec::new(),
    };

    let use_imm8 = matches!(ext, 0xA4 | 0xAC);
    let is_shld = matches!(ext, 0xA4 | 0xA5);
    let mut count = if use_imm8 {
        match decode_immediate(insn, decoded.next_idx, 1, size, false) {
            Some(v) => v,
            None => return Vec::new(),
        }
    } else {
        x86_reg(1, 1)
    };

    let count_mask = if size == 8 { 0x3F } else { 0x1F };
    if count.is_constant {
        let masked = (count.constant_val as u64) & count_mask;
        if masked == 0 {
            return ops;
        }
        count = const_u64(masked, size);
    } else {
        if count.size != size {
            let ext_count = temp.alloc(size);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntZExt,
                address,
                output: Some(ext_count.clone()),
                inputs: vec![count],
                asm_mnemonic: Some("SHXD_COUNT_ZEXT".to_string()),
            });
            count = ext_count;
        }
        let masked = temp.alloc(size);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntAnd,
            address,
            output: Some(masked.clone()),
            inputs: vec![count, const_u64(count_mask, size)],
            asm_mnemonic: Some("SHXD_COUNT_MASK".to_string()),
        });
        count = masked;
    }

    let dst_old = materialize_rm_value(&decoded.rm, size, address, &mut ops, temp, seq);
    let src = x86_reg(decoded.reg_index, size);
    let bits = u64::from(size).saturating_mul(8);
    let inv_shift = if count.is_constant {
        const_u64(bits.saturating_sub(count.constant_val as u64), size)
    } else {
        let out = temp.alloc(size);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntSub,
            address,
            output: Some(out.clone()),
            inputs: vec![const_u64(bits, size), count.clone()],
            asm_mnemonic: Some("SHXD_INV_SHIFT".to_string()),
        });
        out
    };

    let a = temp.alloc(size);
    let b = temp.alloc(size);
    if is_shld {
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntLeft,
            address,
            output: Some(a.clone()),
            inputs: vec![dst_old, count.clone()],
            asm_mnemonic: Some("SHLD_PART_A".to_string()),
        });
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntRight,
            address,
            output: Some(b.clone()),
            inputs: vec![src, inv_shift],
            asm_mnemonic: Some("SHLD_PART_B".to_string()),
        });
    } else {
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntRight,
            address,
            output: Some(a.clone()),
            inputs: vec![dst_old, count.clone()],
            asm_mnemonic: Some("SHRD_PART_A".to_string()),
        });
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::IntLeft,
            address,
            output: Some(b.clone()),
            inputs: vec![src, inv_shift],
            asm_mnemonic: Some("SHRD_PART_B".to_string()),
        });
    }

    let merged = temp.alloc(size);
    let tag = if is_shld { "SHLD" } else { "SHRD" };
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntOr,
        address,
        output: Some(merged.clone()),
        inputs: vec![a, b],
        asm_mnemonic: Some(format!("{tag}_MERGE")),
    });

    write_rm_value(&decoded.rm, merged, address, &mut ops, seq, tag)
}

pub(super) fn decode_bswap(
    prefix: &PrefixState,
    size: u32,
    ext: u8,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let reg_size = if size == 8 { 8 } else { 4 };
    let reg_index = u32::from(ext.saturating_sub(0xC8)) + rex_b(prefix);
    let reg = x86_reg(reg_index, reg_size);

    let mut ops = Vec::new();
    let mut reversed: Option<Varnode> = None;
    for byte_idx in (0..reg_size).rev() {
        let byte = temp.alloc(1);
        ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::SubPiece,
            address,
            output: Some(byte.clone()),
            inputs: vec![reg.clone(), const_u64(u64::from(byte_idx), 4)],
            asm_mnemonic: Some("BSWAP_EXTRACT".to_string()),
        });
        reversed = Some(match reversed {
            Some(low) => {
                let combined = temp.alloc(low.size.saturating_add(1));
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::Piece,
                    address,
                    output: Some(combined.clone()),
                    inputs: vec![byte, low],
                    asm_mnemonic: Some("BSWAP_PIECE".to_string()),
                });
                combined
            }
            None => byte,
        });
    }

    let result = match reversed {
        Some(v) => v,
        None => return Vec::new(),
    };
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(reg),
        inputs: vec![result],
        asm_mnemonic: Some("BSWAP_WRITE".to_string()),
    });
    ops
}
