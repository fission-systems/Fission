use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use super::predicate::emit_jcc_predicate_with_allocator;
use super::super::common::{const_u64, x86_flag_zf, x86_reg, RAM_SPACE_ID, UNIQUE_SPACE_ID};
#[cfg(test)]
use super::super::common::{x86_flag_cf, x86_flag_of, x86_flag_sf};

#[derive(Debug, Clone, Copy)]
struct PrefixState {
    address_size_override: bool,
    rex: u8,
    segment_override: Option<u8>,
}

#[derive(Debug, Clone)]
struct X86CtrlTempFactory {
    next: u64,
}

impl X86CtrlTempFactory {
    fn new(address: u64) -> Self {
        Self {
            next: ctrl_tmp_base(address),
        }
    }

    fn alloc(&mut self, size: u32) -> Varnode {
        let vn = Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset: self.next,
            size,
            is_constant: false,
            constant_val: 0,
        };
        self.next = self.next.wrapping_add(8);
        vn
    }
}

pub(crate) fn decode_control(insn: &[u8], address: u64, decoded_len: u64) -> Option<Vec<PcodeOp>> {
    let (op_idx, op, ext) = opcode_window(insn)?;
    let prefix = prefix_state(insn, op_idx);
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
        (0xE0..=0xE3, _) => decode_loop_jcxz_control(insn, op_idx, op, &prefix, address, next),
        (0xFF, _) => decode_ff_indirect_control(insn, op_idx, &prefix, address),
        _ => None,
    }
}

fn decode_loop_jcxz_control(
    insn: &[u8],
    op_idx: usize,
    op: u8,
    prefix: &PrefixState,
    address: u64,
    next: u64,
) -> Option<Vec<PcodeOp>> {
    let rel = *insn.get(op_idx + 1)? as i8;
    let target = next.wrapping_add_signed(rel as i64);

    let mut seq = 1u32;
    let mut temp = X86CtrlTempFactory::new(address);
    let mut ops = Vec::new();
    let counter_size = if prefix.address_size_override { 4 } else { 8 };
    let counter_reg = x86_reg(1, counter_size);

    let cond = match op {
        0xE3 => {
            let is_zero = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntEqual,
                address,
                output: Some(is_zero.clone()),
                inputs: vec![counter_reg.clone(), const_u64(0, counter_size)],
                asm_mnemonic: Some("JCXZ_COUNT_ZERO".to_string()),
            });
            is_zero
        }
        0xE0..=0xE2 => {
            let dec = temp.alloc(counter_size);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntSub,
                address,
                output: Some(dec.clone()),
                inputs: vec![counter_reg.clone(), const_u64(1, counter_size)],
                asm_mnemonic: Some("LOOP_COUNT_DEC".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(counter_reg),
                inputs: vec![dec.clone()],
                asm_mnemonic: Some("LOOP_COUNT_WRITE".to_string()),
            });

            let nz = temp.alloc(1);
            ops.push(PcodeOp {
                seq_num: next_seq(&mut seq),
                opcode: PcodeOpcode::IntNotEqual,
                address,
                output: Some(nz.clone()),
                inputs: vec![dec, const_u64(0, counter_size)],
                asm_mnemonic: Some("LOOP_COUNT_NONZERO".to_string()),
            });

            match op {
                0xE2 => nz,
                0xE1 => {
                    let cond = temp.alloc(1);
                    ops.push(PcodeOp {
                        seq_num: next_seq(&mut seq),
                        opcode: PcodeOpcode::BoolAnd,
                        address,
                        output: Some(cond.clone()),
                        inputs: vec![nz, x86_flag_zf()],
                        asm_mnemonic: Some("LOOPE_COND".to_string()),
                    });
                    cond
                }
                0xE0 => {
                    let not_zf = temp.alloc(1);
                    ops.push(PcodeOp {
                        seq_num: next_seq(&mut seq),
                        opcode: PcodeOpcode::BoolNegate,
                        address,
                        output: Some(not_zf.clone()),
                        inputs: vec![x86_flag_zf()],
                        asm_mnemonic: Some("LOOPNE_NOT_ZF".to_string()),
                    });
                    let cond = temp.alloc(1);
                    ops.push(PcodeOp {
                        seq_num: next_seq(&mut seq),
                        opcode: PcodeOpcode::BoolAnd,
                        address,
                        output: Some(cond.clone()),
                        inputs: vec![nz, not_zf],
                        asm_mnemonic: Some("LOOPNE_COND".to_string()),
                    });
                    cond
                }
                _ => return None,
            }
        }
        _ => return None,
    };

    ops.push(PcodeOp {
        seq_num: next_seq(&mut seq),
        opcode: PcodeOpcode::CBranch,
        address,
        output: None,
        inputs: vec![Varnode::constant(target as i64, 8), cond],
        asm_mnemonic: Some(match op {
            0xE0 => "LOOPNE",
            0xE1 => "LOOPE",
            0xE2 => "LOOP",
            0xE3 => "JCXZ",
            _ => "LOOP_JCXZ",
        }
        .to_string()),
    });

    Some(ops)
}

fn decode_ff_indirect_control(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
) -> Option<Vec<PcodeOp>> {
    let mut idx = op_idx + 1;
    let modrm = *insn.get(idx)?;
    idx += 1;

    let mode = (modrm >> 6) & 0x3;
    let reg_field = (modrm >> 3) & 0x7;
    let rm_low = modrm & 0x7;

    let (opcode, asm) = match reg_field {
        2 | 3 => (PcodeOpcode::CallInd, "CALL_IND"),
        4 | 5 => (PcodeOpcode::BranchInd, "JMP_IND"),
        _ => return None,
    };

    if mode == 0x3 {
        let rm_idx = u32::from(rm_low) + rex_b(prefix);
        return Some(vec![PcodeOp {
            seq_num: 1,
            opcode,
            address,
            output: None,
            inputs: vec![x86_reg(rm_idx, 8)],
            asm_mnemonic: Some(asm.to_string()),
        }]);
    }

    let mut seq = 1u32;
    let mut temp = X86CtrlTempFactory::new(address);
    let mut ops = Vec::new();
    let addr_vn = if prefix.address_size_override {
        decode_effective_address_addr32(
            insn,
            mode,
            rm_low,
            prefix,
            address,
            &mut idx,
            &mut ops,
            &mut seq,
            &mut temp,
        )?
    } else {
        decode_effective_address_addr64(
            insn,
            mode,
            rm_low,
            prefix,
            address,
            &mut idx,
            &mut ops,
            &mut seq,
            &mut temp,
        )?
    };

    let target = temp.alloc(8);
    ops.push(PcodeOp {
        seq_num: next_seq(&mut seq),
        opcode: PcodeOpcode::Load,
        address,
        output: Some(target.clone()),
        inputs: vec![const_u64(RAM_SPACE_ID, 8), addr_vn.clone()],
        asm_mnemonic: Some("INDIRECT_TARGET_LOAD".to_string()),
    });
    ops.push(PcodeOp {
        seq_num: next_seq(&mut seq),
        opcode,
        address,
        output: None,
        inputs: vec![target],
        asm_mnemonic: Some(asm.to_string()),
    });
    Some(ops)
}

fn decode_effective_address_addr64(
    insn: &[u8],
    mode: u8,
    rm_low: u8,
    prefix: &PrefixState,
    address: u64,
    idx: &mut usize,
    ops: &mut Vec<PcodeOp>,
    seq: &mut u32,
    temp: &mut X86CtrlTempFactory,
) -> Option<Varnode> {
    let base: Option<Varnode>;
    let mut index: Option<(Varnode, u8)> = None;
    let disp: i64;

    if rm_low == 0x4 {
        let sib = *insn.get(*idx)?;
        *idx += 1;
        let scale = (sib >> 6) & 0x3;
        let index_low = (sib >> 3) & 0x7;
        let base_low = sib & 0x7;

        if !(index_low == 0x4 && rex_x(prefix) == 0) {
            index = Some((x86_reg(u32::from(index_low) + rex_x(prefix), 8), scale));
        }

        if mode == 0 && base_low == 0x5 {
            let d = read_i32(insn, *idx)?;
            *idx += 4;
            disp = i64::from(d);
            base = Some(const_u64(address.wrapping_add(insn.len() as u64), 8));
        } else {
            base = Some(x86_reg(u32::from(base_low) + rex_b(prefix), 8));
            let (d, consumed) = read_disp(insn, *idx, mode)?;
            *idx += consumed;
            disp = d;
        }
    } else if mode == 0 && rm_low == 0x5 {
        let d = read_i32(insn, *idx)?;
        *idx += 4;
        base = Some(const_u64(address.wrapping_add(insn.len() as u64), 8));
        disp = i64::from(d);
    } else {
        base = Some(x86_reg(u32::from(rm_low) + rex_b(prefix), 8));
        let (d, consumed) = read_disp(insn, *idx, mode)?;
        *idx += consumed;
        disp = d;
    }

    compose_effective_address(base, index, disp, address, ops, seq, temp, 8, "EA", false)
}

fn decode_effective_address_addr32(
    insn: &[u8],
    mode: u8,
    rm_low: u8,
    prefix: &PrefixState,
    address: u64,
    idx: &mut usize,
    ops: &mut Vec<PcodeOp>,
    seq: &mut u32,
    temp: &mut X86CtrlTempFactory,
) -> Option<Varnode> {
    let mut base: Option<Varnode> = None;
    let mut index: Option<(Varnode, u8)> = None;
    let disp: i64;

    if rm_low == 0x4 {
        let sib = *insn.get(*idx)?;
        *idx += 1;
        let scale = (sib >> 6) & 0x3;
        let index_low = (sib >> 3) & 0x7;
        let base_low = sib & 0x7;

        if !(index_low == 0x4 && rex_x(prefix) == 0) {
            index = Some((x86_reg(u32::from(index_low) + rex_x(prefix), 4), scale));
        }

        if mode == 0 && base_low == 0x5 {
            let d = read_i32(insn, *idx)?;
            *idx += 4;
            disp = i64::from(d);
        } else {
            base = Some(x86_reg(u32::from(base_low) + rex_b(prefix), 4));
            let (d, consumed) = read_disp(insn, *idx, mode)?;
            *idx += consumed;
            disp = d;
        }
    } else if mode == 0 && rm_low == 0x5 {
        let d = read_i32(insn, *idx)?;
        *idx += 4;
        disp = i64::from(d);
    } else {
        base = Some(x86_reg(u32::from(rm_low) + rex_b(prefix), 4));
        let (d, consumed) = read_disp(insn, *idx, mode)?;
        *idx += consumed;
        disp = d;
    }

    let ea32 = compose_effective_address(base, index, disp, address, ops, seq, temp, 4, "EA32", true)?;
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

fn compose_effective_address(
    base: Option<Varnode>,
    index: Option<(Varnode, u8)>,
    disp: i64,
    address: u64,
    ops: &mut Vec<PcodeOp>,
    seq: &mut u32,
    temp: &mut X86CtrlTempFactory,
    width: u32,
    prefix: &str,
    disp_const_only: bool,
) -> Option<Varnode> {
    let mut cur = base;

    if let Some((idx_vn, scale)) = index {
        let scaled = if scale == 0 {
            idx_vn
        } else {
            let out = temp.alloc(width);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::IntLeft,
                address,
                output: Some(out.clone()),
                inputs: vec![idx_vn, const_u64(u64::from(scale), width)],
                asm_mnemonic: Some(format!("{prefix}_SCALE")),
            });
            out
        };

        cur = match cur {
            Some(base_v) => {
                let out = temp.alloc(width);
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::IntAdd,
                    address,
                    output: Some(out.clone()),
                    inputs: vec![base_v, scaled],
                    asm_mnemonic: Some(format!("{prefix}_ADD_INDEX")),
                });
                Some(out)
            }
            None => Some(scaled),
        };
    }

    if disp != 0 {
        if cur.is_none() && disp_const_only {
            cur = Some(Varnode::constant(disp, width));
        } else {
            let delta = const_u64(disp.unsigned_abs(), width);
            cur = match cur {
                Some(base_v) => {
                    let out = temp.alloc(width);
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
                        asm_mnemonic: Some(format!("{prefix}_DISP")),
                    });
                    Some(out)
                }
                None => Some(Varnode::constant(disp, width)),
            };
        }
    }

    cur.or(Some(const_u64(0, width)))
}

fn build_jcc_ops(address: u64, target: u64, cond: u8) -> Option<Vec<PcodeOp>> {
    let mut seq = 1u32;
    let mut tmp = ctrl_tmp_base(address);
    let mut alloc_tmp = |size: u32| {
        let vn = Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset: tmp,
            size,
            is_constant: false,
            constant_val: 0,
        };
        tmp = tmp.wrapping_add(8);
        vn
    };
    let mut ops = Vec::new();
    let pred = emit_jcc_predicate_with_allocator(&mut ops, address, cond, &mut seq, &mut alloc_tmp)?;
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

fn prefix_state(insn: &[u8], op_idx: usize) -> PrefixState {
    let mut state = PrefixState {
        address_size_override: false,
        rex: 0,
        segment_override: None,
    };
    for &b in &insn[..op_idx] {
        match b {
            0x67 => state.address_size_override = true,
            0x2E => state.segment_override = Some(1), // CS
            0x3E => state.segment_override = Some(3), // DS
            0x26 => state.segment_override = Some(0), // ES
            0x36 => state.segment_override = Some(2), // SS
            0x64 => state.segment_override = Some(4), // FS
            0x65 => state.segment_override = Some(5), // GS
            0x40..=0x4F => state.rex = b,
            _ => {}
        }
    }
    state
}

fn rex_b(prefix: &PrefixState) -> u32 {
    if (prefix.rex & 0x01) != 0 {
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

    #[test]
    fn decode_ff_call_indirect_reg() {
        let ops = decode(&[0xFF, 0xD0], 0x7000); // call rax
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].opcode, PcodeOpcode::CallInd);
        assert_eq!(ops[0].inputs, vec![x86_reg(0, 8)]);
    }

    #[test]
    fn decode_ff_jmp_indirect_reg_with_rex_b() {
        let ops = decode(&[0x41, 0xFF, 0xE0], 0x7010); // jmp r8
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].opcode, PcodeOpcode::BranchInd);
        assert_eq!(ops[0].inputs, vec![x86_reg(8, 8)]);
    }

    #[test]
    fn decode_ff_call_indirect_memory_emits_target_load() {
        let ops = decode(&[0xFF, 0x10], 0x7020); // call qword ptr [rax]
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].opcode, PcodeOpcode::Load);
        assert_eq!(ops[0].asm_mnemonic.as_deref(), Some("INDIRECT_TARGET_LOAD"));
        assert_eq!(ops[1].opcode, PcodeOpcode::CallInd);
        assert_eq!(ops[1].inputs.len(), 1);
        assert_eq!(ops[0].output.as_ref(), Some(&ops[1].inputs[0]));
    }

    #[test]
    fn decode_ff_jmp_indirect_rip_relative_memory() {
        let ops = decode(&[0xFF, 0x25, 0x10, 0x00, 0x00, 0x00], 0x7030); // jmp qword ptr [rip+0x10]
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Load));
        assert_eq!(ops.last().map(|op| op.opcode), Some(PcodeOpcode::BranchInd));
    }

    #[test]
    fn decode_ff_far_call_memory_decodes_as_call_ind() {
        let ops = decode(&[0xFF, 0x18], 0x7040); // far call m16:64
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].opcode, PcodeOpcode::Load);
        assert_eq!(ops[1].opcode, PcodeOpcode::CallInd);
        assert_eq!(ops[1].asm_mnemonic.as_deref(), Some("CALL_IND"));
        assert_eq!(ops[0].output.as_ref(), Some(&ops[1].inputs[0]));
    }

    #[test]
    fn decode_ff_far_jmp_memory_decodes_as_branch_ind() {
        let ops = decode(&[0xFF, 0x28], 0x7050); // far jmp m16:64
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].opcode, PcodeOpcode::Load);
        assert_eq!(ops[1].opcode, PcodeOpcode::BranchInd);
        assert_eq!(ops[1].asm_mnemonic.as_deref(), Some("JMP_IND"));
        assert_eq!(ops[0].output.as_ref(), Some(&ops[1].inputs[0]));
    }

    #[test]
    fn decode_ff_far_call_reg_decodes_as_call_ind() {
        let ops = decode(&[0xFF, 0xD8], 0x7060); // far call rax
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].opcode, PcodeOpcode::CallInd);
        assert_eq!(ops[0].asm_mnemonic.as_deref(), Some("CALL_IND"));
        assert_eq!(ops[0].inputs, vec![x86_reg(0, 8)]);
    }

    #[test]
    fn decode_ff_far_jmp_reg_decodes_as_branch_ind() {
        let ops = decode(&[0xFF, 0xE8], 0x7070); // far jmp rax
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].opcode, PcodeOpcode::BranchInd);
        assert_eq!(ops[0].asm_mnemonic.as_deref(), Some("JMP_IND"));
        assert_eq!(ops[0].inputs, vec![x86_reg(0, 8)]);
    }

    #[test]
    fn decode_loop_ne_updates_counter_and_branches() {
        let address = 0x8000u64;
        let ops = decode(&[0xE0, 0xFE], address);
        assert_eq!(ops.last().map(|op| op.opcode), Some(PcodeOpcode::CBranch));
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntSub));
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntNotEqual));
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::BoolNegate));
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::BoolAnd));
        assert_eq!(ops.last().expect("cbranch").inputs[0].constant_val as u64, address);
    }

    #[test]
    fn decode_loop_uses_ecx_when_address_override_present() {
        let ops = decode(&[0x67, 0xE2, 0x02], 0x8100); // addr-size override => ecx
        let sub = ops
            .iter()
            .find(|op| op.opcode == PcodeOpcode::IntSub)
            .expect("loop decrement");
        assert_eq!(sub.inputs[0], x86_reg(1, 4));
        assert_eq!(sub.inputs[1], const_u64(1, 4));
    }

    #[test]
    fn decode_jcxz_family_does_not_decrement_counter() {
        let ops = decode(&[0xE3, 0x05], 0x8200);
        assert_eq!(ops.last().map(|op| op.opcode), Some(PcodeOpcode::CBranch));
        assert!(!ops.iter().any(|op| op.opcode == PcodeOpcode::IntSub));
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntEqual));
    }
}
