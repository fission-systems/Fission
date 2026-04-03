use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

pub(crate) fn decode_control(insn: &[u8], address: u64, decoded_len: u64) -> Option<PcodeOp> {
    let (op_idx, op, ext) = opcode_window(insn)?;
    let next = address.wrapping_add(decoded_len);

    match (op, ext) {
        (0xC3, _) | (0xCB, _) | (0xC2, _) | (0xCA, _) => Some(PcodeOp {
            seq_num: 1,
            opcode: PcodeOpcode::Return,
            address,
            output: None,
            inputs: Vec::new(),
            asm_mnemonic: Some("RET".to_string()),
        }),
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
            Some(PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::Call,
                address,
                output: None,
                inputs: vec![Varnode::constant(target as i64, 8)],
                asm_mnemonic: Some("CALL".to_string()),
            })
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
            Some(PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::Branch,
                address,
                output: None,
                inputs: vec![Varnode::constant(target as i64, 8)],
                asm_mnemonic: Some("JMP".to_string()),
            })
        }
        (0xEB, _) => {
            if op_idx + 2 > insn.len() {
                return None;
            }
            let rel = insn[op_idx + 1] as i8;
            let target = next.wrapping_add_signed(rel as i64);
            Some(PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::Branch,
                address,
                output: None,
                inputs: vec![Varnode::constant(target as i64, 8)],
                asm_mnemonic: Some("JMP".to_string()),
            })
        }
        (0x70..=0x7F, _) => {
            if op_idx + 2 > insn.len() {
                return None;
            }
            let rel = insn[op_idx + 1] as i8;
            let target = next.wrapping_add_signed(rel as i64);
            Some(PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::CBranch,
                address,
                output: None,
                inputs: vec![Varnode::constant(target as i64, 8), Varnode::constant(1, 1)],
                asm_mnemonic: Some("Jcc".to_string()),
            })
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
            Some(PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::CBranch,
                address,
                output: None,
                inputs: vec![Varnode::constant(target as i64, 8), Varnode::constant(1, 1)],
                asm_mnemonic: Some("Jcc".to_string()),
            })
        }
        _ => None,
    }
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
