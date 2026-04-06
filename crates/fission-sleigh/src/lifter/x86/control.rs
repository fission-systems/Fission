use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use super::predicate::emit_jcc_predicate_with_allocator;
use super::super::common::{const_u64, x86_reg, RAM_SPACE_ID, UNIQUE_SPACE_ID};
#[cfg(test)]
use super::super::common::{x86_flag_cf, x86_flag_of, x86_flag_sf, x86_flag_zf};

#[derive(Debug, Clone, Copy)]
struct PrefixState {
    address_size_override: bool,
    rex: u8,
}

#[derive(Debug, Clone)]
struct X86CtrlTempFactory {
    next: u64,
}

const X86_FAR_CALL_UNSUPPORTED_ID: u64 = 0xFF03;
const X86_FAR_JMP_UNSUPPORTED_ID: u64 = 0xFF05;

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
        (0xFF, _) => decode_ff_indirect_control(insn, op_idx, &prefix, address),
        _ => None,
    }
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

    let (opcode, asm, unsupported_id) = match reg_field {
        2 => (PcodeOpcode::CallInd, "CALL_IND", None),
        4 => (PcodeOpcode::BranchInd, "JMP_IND", None),
        3 => (PcodeOpcode::CallOther, "FAR_CALL_UNSUPPORTED", Some(X86_FAR_CALL_UNSUPPORTED_ID)),
        5 => (PcodeOpcode::CallOther, "FAR_JMP_UNSUPPORTED", Some(X86_FAR_JMP_UNSUPPORTED_ID)),
        _ => return None,
    };

    if mode == 0x3 {
        let rm_idx = u32::from(rm_low) + rex_b(prefix);
        let mut inputs = Vec::new();
        if let Some(id) = unsupported_id {
            inputs.push(const_u64(id, 8));
            // Keep FAR unsupported CallOther schema stable across reg/mem forms:
            // [policy_id, target_addr_or_zero, target_value]
            inputs.push(const_u64(0, 8));
        }
        inputs.push(x86_reg(rm_idx, 8));
        return Some(vec![PcodeOp {
            seq_num: 1,
            opcode,
            address,
            output: None,
            inputs,
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
    let mut inputs = Vec::new();
    if let Some(id) = unsupported_id {
        inputs.push(const_u64(id, 8));
        inputs.push(addr_vn);
        inputs.push(target);
    } else {
        inputs.push(target);
    }
    ops.push(PcodeOp {
        seq_num: next_seq(&mut seq),
        opcode,
        address,
        output: None,
        inputs,
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
    };
    for &b in &insn[..op_idx] {
        if b == 0x67 {
            state.address_size_override = true;
        }
        if (0x40..=0x4F).contains(&b) {
            state.rex = b;
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
    fn decode_ff_far_call_is_explicitly_unsupported() {
        let ops = decode(&[0xFF, 0x18], 0x7040); // far call m16:64
        let load = ops
            .iter()
            .find(|op| op.asm_mnemonic.as_deref() == Some("INDIRECT_TARGET_LOAD"))
            .expect("expected indirect target load");
        let unsupported = ops
            .iter()
            .find(|op| op.asm_mnemonic.as_deref() == Some("FAR_CALL_UNSUPPORTED"))
            .expect("expected explicit far call unsupported op");
        assert_eq!(unsupported.opcode, PcodeOpcode::CallOther);
        assert_eq!(unsupported.inputs.len(), 3);
        assert!(unsupported.inputs[0].is_constant);
        assert_eq!(unsupported.inputs[0].constant_val as u64, X86_FAR_CALL_UNSUPPORTED_ID);
        assert_eq!(unsupported.inputs[2], load.output.clone().expect("load output"));
    }

    #[test]
    fn decode_ff_far_jmp_is_explicitly_unsupported() {
        let ops = decode(&[0xFF, 0x28], 0x7050); // far jmp m16:64
        let load = ops
            .iter()
            .find(|op| op.asm_mnemonic.as_deref() == Some("INDIRECT_TARGET_LOAD"))
            .expect("expected indirect target load");
        let unsupported = ops
            .iter()
            .find(|op| op.asm_mnemonic.as_deref() == Some("FAR_JMP_UNSUPPORTED"))
            .expect("expected explicit far jmp unsupported op");
        assert_eq!(unsupported.opcode, PcodeOpcode::CallOther);
        assert_eq!(unsupported.inputs.len(), 3);
        assert!(unsupported.inputs[0].is_constant);
        assert_eq!(unsupported.inputs[0].constant_val as u64, X86_FAR_JMP_UNSUPPORTED_ID);
        assert_eq!(unsupported.inputs[2], load.output.clone().expect("load output"));
    }

    #[test]
    fn decode_ff_far_call_reg_uses_explicit_unsupported_marker() {
        let ops = decode(&[0xFF, 0xD8], 0x7060); // far call rax
        assert_eq!(ops.len(), 1);
        let unsupported = &ops[0];
        assert_eq!(unsupported.opcode, PcodeOpcode::CallOther);
        assert_eq!(unsupported.asm_mnemonic.as_deref(), Some("FAR_CALL_UNSUPPORTED"));
        assert_eq!(unsupported.inputs.len(), 3);
        assert_eq!(unsupported.inputs[0].constant_val as u64, X86_FAR_CALL_UNSUPPORTED_ID);
        assert_eq!(unsupported.inputs[1].constant_val as u64, 0);
        assert_eq!(unsupported.inputs[2], x86_reg(0, 8));
    }

    #[test]
    fn decode_ff_far_jmp_reg_uses_explicit_unsupported_marker() {
        let ops = decode(&[0xFF, 0xE8], 0x7070); // far jmp rax
        assert_eq!(ops.len(), 1);
        let unsupported = &ops[0];
        assert_eq!(unsupported.opcode, PcodeOpcode::CallOther);
        assert_eq!(unsupported.asm_mnemonic.as_deref(), Some("FAR_JMP_UNSUPPORTED"));
        assert_eq!(unsupported.inputs.len(), 3);
        assert_eq!(unsupported.inputs[0].constant_val as u64, X86_FAR_JMP_UNSUPPORTED_ID);
        assert_eq!(unsupported.inputs[1].constant_val as u64, 0);
        assert_eq!(unsupported.inputs[2], x86_reg(0, 8));
    }
}
