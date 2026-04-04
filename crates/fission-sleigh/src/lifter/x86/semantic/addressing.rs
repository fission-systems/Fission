use super::*;

pub(super) fn parse_prefixes(insn: &[u8]) -> (usize, PrefixState) {
    let mut idx = 0usize;
    let mut state = PrefixState {
        operand_size_override: false,
        address_size_override: false,
        rex: 0,
    };

    while idx < insn.len() && is_prefix(insn[idx]) {
        let byte = insn[idx];
        if byte == 0x66 {
            state.operand_size_override = true;
        }
        if byte == 0x67 {
            state.address_size_override = true;
        }
        if (0x40..=0x4F).contains(&byte) {
            state.rex = byte;
        }
        idx += 1;
    }

    (idx, state)
}

pub(super) fn operand_size(prefix: &PrefixState) -> u32 {
    if rex_w(prefix) {
        8
    } else if prefix.operand_size_override {
        2
    } else {
        4
    }
}

pub(super) fn immediate_bytes_for_operand(size: u32) -> usize {
    if size == 2 {
        2
    } else {
        4
    }
}

fn rex_w(prefix: &PrefixState) -> bool {
    (prefix.rex & 0x08) != 0
}

fn rex_r(prefix: &PrefixState) -> u32 {
    if (prefix.rex & 0x04) != 0 {
        8
    } else {
        0
    }
}

fn rex_x(prefix: &PrefixState) -> u32 {
    if (prefix.rex & 0x02) != 0 {
        8
    } else {
        0
    }
}

fn rex_b(prefix: &PrefixState) -> u32 {
    if (prefix.rex & 0x01) != 0 {
        8
    } else {
        0
    }
}

pub(super) fn decode_modrm_operand(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    ops: &mut Vec<PcodeOp>,
    seq: &mut u32,
) -> Option<DecodedModrm> {
    let mut idx = op_idx + 1;
    let modrm = *insn.get(idx)?;
    idx += 1;

    let mode = (modrm >> 6) & 0x3;
    let reg_field = (modrm >> 3) & 0x7;
    let rm_low = modrm & 0x7;

    let reg_index = u32::from(reg_field) + rex_r(prefix);

    if mode == 0x3 {
        let rm_idx = u32::from(rm_low) + rex_b(prefix);
        return Some(DecodedModrm {
            reg_index,
            reg_field,
            rm: RmOperand::Reg(x86_reg(rm_idx, size)),
            next_idx: idx,
        });
    }

    if prefix.address_size_override {
        return decode_modrm_operand_addr32(insn, op_idx, prefix, size, address, temp, ops, seq);
    }

    let mut base: Option<Varnode> = None;
    let mut index: Option<(Varnode, u8)> = None;
    let disp: i64;

    if rm_low == 0x4 {
        let sib = *insn.get(idx)?;
        idx += 1;

        let scale = (sib >> 6) & 0x3;
        let index_low = (sib >> 3) & 0x7;
        let base_low = sib & 0x7;

        if !(index_low == 0x4 && rex_x(prefix) == 0) {
            let idx_reg = u32::from(index_low) + rex_x(prefix);
            index = Some((x86_reg(idx_reg, 8), scale));
        }

        if mode == 0 && base_low == 0x5 {
            let d = read_i32(insn, idx)?;
            idx += 4;
            disp = i64::from(d);
        } else {
            let base_idx = u32::from(base_low) + rex_b(prefix);
            base = Some(x86_reg(base_idx, 8));
            let (d, consumed) = read_disp(insn, idx, mode)?;
            idx += consumed;
            disp = d;
        }
    } else if mode == 0 && rm_low == 0x5 {
        let d = read_i32(insn, idx)?;
        idx += 4;
        base = Some(const_u64(address.wrapping_add(insn.len() as u64), 8));
        disp = i64::from(d);
    } else {
        let base_idx = u32::from(rm_low) + rex_b(prefix);
        base = Some(x86_reg(base_idx, 8));
        let (d, consumed) = read_disp(insn, idx, mode)?;
        idx += consumed;
        disp = d;
    }

    let addr = compose_effective_address(base, index, disp, address, temp, ops, seq)?;

    Some(DecodedModrm {
        reg_index,
        reg_field,
        rm: RmOperand::Mem(addr),
        next_idx: idx,
    })
}

fn decode_modrm_operand_addr32(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    ops: &mut Vec<PcodeOp>,
    seq: &mut u32,
) -> Option<DecodedModrm> {
    let mut idx = op_idx + 1;
    let modrm = *insn.get(idx)?;
    idx += 1;

    let mode = (modrm >> 6) & 0x3;
    let reg_field = (modrm >> 3) & 0x7;
    let rm_low = modrm & 0x7;

    let reg_index = u32::from(reg_field) + rex_r(prefix);

    if mode == 0x3 {
        let rm_idx = u32::from(rm_low) + rex_b(prefix);
        return Some(DecodedModrm {
            reg_index,
            reg_field,
            rm: RmOperand::Reg(x86_reg(rm_idx, size)),
            next_idx: idx,
        });
    }

    let mut base: Option<Varnode> = None;
    let mut index: Option<(Varnode, u8)> = None;
    let disp: i64;

    if rm_low == 0x4 {
        let sib = *insn.get(idx)?;
        idx += 1;

        let scale = (sib >> 6) & 0x3;
        let index_low = (sib >> 3) & 0x7;
        let base_low = sib & 0x7;

        if !(index_low == 0x4 && rex_x(prefix) == 0) {
            let idx_reg = u32::from(index_low) + rex_x(prefix);
            index = Some((x86_reg(idx_reg, 4), scale));
        }

        if mode == 0 && base_low == 0x5 {
            let d = read_i32(insn, idx)?;
            idx += 4;
            disp = i64::from(d);
        } else {
            let base_idx = u32::from(base_low) + rex_b(prefix);
            base = Some(x86_reg(base_idx, 4));
            let (d, consumed) = read_disp(insn, idx, mode)?;
            idx += consumed;
            disp = d;
        }
    } else if mode == 0 && rm_low == 0x5 {
        let d = read_i32(insn, idx)?;
        idx += 4;
        disp = i64::from(d);
    } else {
        let base_idx = u32::from(rm_low) + rex_b(prefix);
        base = Some(x86_reg(base_idx, 4));
        let (d, consumed) = read_disp(insn, idx, mode)?;
        idx += consumed;
        disp = d;
    }

    let addr = compose_effective_address_addr32(base, index, disp, address, temp, ops, seq)?;

    Some(DecodedModrm {
        reg_index,
        reg_field,
        rm: RmOperand::Mem(addr),
        next_idx: idx,
    })
}

fn read_disp(insn: &[u8], idx: usize, mode: u8) -> Option<(i64, usize)> {
    match mode {
        0 => Some((0, 0)),
        1 => Some((i64::from(*insn.get(idx)? as i8), 1)),
        2 => Some((i64::from(read_i32(insn, idx)?), 4)),
        _ => Some((0, 0)),
    }
}

fn read_i32(insn: &[u8], idx: usize) -> Option<i32> {
    let b0 = *insn.get(idx)?;
    let b1 = *insn.get(idx + 1)?;
    let b2 = *insn.get(idx + 2)?;
    let b3 = *insn.get(idx + 3)?;
    Some(i32::from_le_bytes([b0, b1, b2, b3]))
}

fn compose_effective_address(
    base: Option<Varnode>,
    index: Option<(Varnode, u8)>,
    disp: i64,
    address: u64,
    temp: &mut X86TempFactory,
    ops: &mut Vec<PcodeOp>,
    seq: &mut u32,
) -> Option<Varnode> {
    let mut cur = base;

    if let Some((idx, scale)) = index {
        let scaled = if scale == 0 {
            idx
        } else {
            let out = temp.alloc(8);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntLeft,
                address,
                output: Some(out.clone()),
                inputs: vec![idx, const_u64(u64::from(scale), 8)],
                asm_mnemonic: Some("EA_SCALE".to_string()),
            });
            out
        };

        cur = match cur {
            Some(base_v) => {
                let out = temp.alloc(8);
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::IntAdd,
                    address,
                    output: Some(out.clone()),
                    inputs: vec![base_v, scaled],
                    asm_mnemonic: Some("EA_ADD_INDEX".to_string()),
                });
                Some(out)
            }
            None => Some(scaled),
        };
    }

    if disp != 0 {
        let delta = const_u64(disp.unsigned_abs(), 8);
        cur = match cur {
            Some(base_v) => {
                let out = temp.alloc(8);
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: if disp >= 0 {
                        PcodeOpcode::IntAdd
                    } else {
                        PcodeOpcode::IntSub
                    },
                    address,
                    output: Some(out.clone()),
                    inputs: vec![base_v, delta],
                    asm_mnemonic: Some("EA_DISP".to_string()),
                });
                Some(out)
            }
            None => Some(Varnode::constant(disp, 8)),
        };
    }

    cur.or(Some(const_u64(0, 8)))
}

fn compose_effective_address_addr32(
    base: Option<Varnode>,
    index: Option<(Varnode, u8)>,
    disp: i64,
    address: u64,
    temp: &mut X86TempFactory,
    ops: &mut Vec<PcodeOp>,
    seq: &mut u32,
) -> Option<Varnode> {
    let mut cur = base;

    if let Some((idx, scale)) = index {
        let scaled = if scale == 0 {
            idx
        } else {
            let out = temp.alloc(4);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntLeft,
                address,
                output: Some(out.clone()),
                inputs: vec![idx, const_u64(u64::from(scale), 4)],
                asm_mnemonic: Some("EA32_SCALE".to_string()),
            });
            out
        };

        cur = match cur {
            Some(base_v) => {
                let out = temp.alloc(4);
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::IntAdd,
                    address,
                    output: Some(out.clone()),
                    inputs: vec![base_v, scaled],
                    asm_mnemonic: Some("EA32_ADD_INDEX".to_string()),
                });
                Some(out)
            }
            None => Some(scaled),
        };
    }

    if disp != 0 {
        let delta = const_u64(disp.unsigned_abs(), 4);
        cur = match cur {
            Some(base_v) => {
                let out = temp.alloc(4);
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: if disp >= 0 {
                        PcodeOpcode::IntAdd
                    } else {
                        PcodeOpcode::IntSub
                    },
                    address,
                    output: Some(out.clone()),
                    inputs: vec![base_v, delta],
                    asm_mnemonic: Some("EA32_DISP".to_string()),
                });
                Some(out)
            }
            None => Some(Varnode::constant(disp, 4)),
        };
    }

    let ea32 = cur.or(Some(const_u64(0, 4)))?;
    let ea64 = temp.alloc(8);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::IntZExt,
        address,
        output: Some(ea64.clone()),
        inputs: vec![ea32],
        asm_mnemonic: Some("EA32_FINAL_ZEXT".to_string()),
    });
    Some(ea64)
}

pub(super) fn decode_immediate(insn: &[u8], idx: usize, width: usize, out_size: u32, sign_extend: bool) -> Option<Varnode> {
    let (val, _consumed) = match width {
        1 => {
            let raw = *insn.get(idx)?;
            let v = if sign_extend {
                i64::from(raw as i8)
            } else {
                i64::from(raw)
            };
            (v, 1usize)
        }
        2 => {
            let b0 = *insn.get(idx)?;
            let b1 = *insn.get(idx + 1)?;
            let raw = u16::from_le_bytes([b0, b1]);
            let v = if sign_extend {
                i64::from(raw as i16)
            } else {
                i64::from(raw)
            };
            (v, 2usize)
        }
        4 => {
            let raw = read_i32(insn, idx)?;
            let v = if sign_extend {
                i64::from(raw)
            } else {
                i64::from(raw as u32)
            };
            (v, 4usize)
        }
        8 => {
            let b0 = *insn.get(idx)?;
            let b1 = *insn.get(idx + 1)?;
            let b2 = *insn.get(idx + 2)?;
            let b3 = *insn.get(idx + 3)?;
            let b4 = *insn.get(idx + 4)?;
            let b5 = *insn.get(idx + 5)?;
            let b6 = *insn.get(idx + 6)?;
            let b7 = *insn.get(idx + 7)?;
            let raw = i64::from_le_bytes([b0, b1, b2, b3, b4, b5, b6, b7]);
            (raw, 8usize)
        }
        _ => return None,
    };
    Some(Varnode::constant(val, out_size))
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

