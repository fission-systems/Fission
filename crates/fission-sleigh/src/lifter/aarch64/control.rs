use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

pub(crate) fn decode_control(insn: &[u8], address: u64) -> Option<PcodeOp> {
	if insn.len() < 4 {
		return None;
	}

	let word = u32::from_le_bytes([insn[0], insn[1], insn[2], insn[3]]);

	if (word & 0xFFFF_FC1F) == 0xD65F_0000 {
		let reg = ((word >> 5) & 0x1F) as i64;
		return Some(PcodeOp {
			seq_num: 1,
			opcode: PcodeOpcode::Return,
			address,
			output: None,
			inputs: vec![Varnode::constant(reg, 4)],
			asm_mnemonic: Some("RET".to_string()),
		});
	}

	if (word & 0xFC00_0000) == 0x1400_0000 {
		let imm26 = (word & 0x03FF_FFFF) as i32;
		let signed = (imm26 << 6) >> 6;
		let disp = (signed as i64) << 2;
		let target = address.wrapping_add_signed(disp);
		return Some(PcodeOp {
			seq_num: 1,
			opcode: PcodeOpcode::Branch,
			address,
			output: None,
			inputs: vec![Varnode::constant(target as i64, 8)],
			asm_mnemonic: Some("B".to_string()),
		});
	}

	if (word & 0xFC00_0000) == 0x9400_0000 {
		let imm26 = (word & 0x03FF_FFFF) as i32;
		let signed = (imm26 << 6) >> 6;
		let disp = (signed as i64) << 2;
		let target = address.wrapping_add_signed(disp);
		return Some(PcodeOp {
			seq_num: 1,
			opcode: PcodeOpcode::Call,
			address,
			output: None,
			inputs: vec![Varnode::constant(target as i64, 8)],
			asm_mnemonic: Some("BL".to_string()),
		});
	}

	if (word & 0xFF00_0010) == 0x5400_0000 {
		let imm19 = ((word >> 5) & 0x7F_FFFF) as i32;
		let signed = (imm19 << 13) >> 13;
		let disp = (signed as i64) << 2;
		let target = address.wrapping_add_signed(disp);
		let cond = (word & 0xF) as i64;
		return Some(PcodeOp {
			seq_num: 1,
			opcode: PcodeOpcode::CBranch,
			address,
			output: None,
			inputs: vec![Varnode::constant(target as i64, 8), Varnode::constant(cond, 1)],
			asm_mnemonic: Some("B.cond".to_string()),
		});
	}

	None
}

