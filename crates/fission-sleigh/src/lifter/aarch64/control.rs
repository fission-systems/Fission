use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use super::super::common::{
	a64_flag_c, a64_flag_n, a64_flag_v, a64_flag_z, a64_reg, const_u64, UNIQUE_SPACE_ID,
};

pub(crate) fn decode_control(insn: &[u8], address: u64) -> Option<Vec<PcodeOp>> {
	if insn.len() < 4 {
		return None;
	}

	let word = u32::from_le_bytes([insn[0], insn[1], insn[2], insn[3]]);

	if (word & 0xFFFF_FC1F) == 0xD65F_0000 {
		let reg = ((word >> 5) & 0x1F) as i64;
		return Some(vec![PcodeOp {
			seq_num: 1,
			opcode: PcodeOpcode::Return,
			address,
			output: None,
			inputs: vec![Varnode::constant(reg, 4)],
			asm_mnemonic: Some("RET".to_string()),
		}]);
	}

	if (word & 0xFC00_0000) == 0x1400_0000 {
		let imm26 = (word & 0x03FF_FFFF) as i32;
		let signed = (imm26 << 6) >> 6;
		let disp = (signed as i64) << 2;
		let target = address.wrapping_add_signed(disp);
		return Some(vec![PcodeOp {
			seq_num: 1,
			opcode: PcodeOpcode::Branch,
			address,
			output: None,
			inputs: vec![Varnode::constant(target as i64, 8)],
			asm_mnemonic: Some("B".to_string()),
		}]);
	}

	if (word & 0xFC00_0000) == 0x9400_0000 {
		let imm26 = (word & 0x03FF_FFFF) as i32;
		let signed = (imm26 << 6) >> 6;
		let disp = (signed as i64) << 2;
		let target = address.wrapping_add_signed(disp);
		return Some(vec![PcodeOp {
			seq_num: 1,
			opcode: PcodeOpcode::Call,
			address,
			output: None,
			inputs: vec![Varnode::constant(target as i64, 8)],
			asm_mnemonic: Some("BL".to_string()),
		}]);
	}

	if (word & 0x7E00_0000) == 0x3400_0000 || (word & 0x7E00_0000) == 0x3500_0000 {
		let is_nonzero = ((word >> 24) & 1) != 0;
		let sf = ((word >> 31) & 1) != 0;
		let size = if sf { 8 } else { 4 };
		let imm19 = ((word >> 5) & 0x7F_FFFF) as i32;
		let signed = (imm19 << 13) >> 13;
		let disp = (signed as i64) << 2;
		let target = address.wrapping_add_signed(disp);
		let rt = word & 0x1F;
		let mut seq = 1u32;
		let mut tmp = ctrl_tmp_base(address);
		let mut ops = Vec::new();
		let cond = alloc_ctrl_tmp(&mut tmp, 1);
		ops.push(PcodeOp {
			seq_num: next_seq(&mut seq),
			opcode: if is_nonzero {
				PcodeOpcode::IntNotEqual
			} else {
				PcodeOpcode::IntEqual
			},
			address,
			output: Some(cond.clone()),
			inputs: vec![a64_reg(rt, size), const_u64(0, size)],
			asm_mnemonic: Some(if is_nonzero {
				"CMP_NE_ZERO".to_string()
			} else {
				"CMP_EQ_ZERO".to_string()
			}),
		});
		ops.push(PcodeOp {
			seq_num: next_seq(&mut seq),
			opcode: PcodeOpcode::CBranch,
			address,
			output: None,
			inputs: vec![Varnode::constant(target as i64, 8), cond],
			asm_mnemonic: Some(if is_nonzero {
				"CBNZ".to_string()
			} else {
				"CBZ".to_string()
			}),
		});
		return Some(ops);
	}

	if (word & 0x7F00_0000) == 0x3600_0000 || (word & 0x7F00_0000) == 0x3700_0000 {
		let is_nonzero = ((word >> 24) & 1) != 0;
		let b5 = ((word >> 31) & 1) != 0;
		let size = if b5 { 8 } else { 4 };
		let bit_pos = ((u32::from(b5)) << 5) | ((word >> 19) & 0x1F);
		let imm14 = ((word >> 5) & 0x3FFF) as i32;
		let signed = (imm14 << 18) >> 18;
		let disp = (signed as i64) << 2;
		let target = address.wrapping_add_signed(disp);
		let rt = word & 0x1F;

		let mut seq = 1u32;
		let mut tmp = ctrl_tmp_base(address);
		let mut ops = Vec::new();
		let shifted = alloc_ctrl_tmp(&mut tmp, size);
		ops.push(PcodeOp {
			seq_num: next_seq(&mut seq),
			opcode: PcodeOpcode::IntRight,
			address,
			output: Some(shifted.clone()),
			inputs: vec![a64_reg(rt, size), const_u64(u64::from(bit_pos), size)],
			asm_mnemonic: Some("TBIT_SHIFT".to_string()),
		});
		let bit = alloc_ctrl_tmp(&mut tmp, size);
		ops.push(PcodeOp {
			seq_num: next_seq(&mut seq),
			opcode: PcodeOpcode::IntAnd,
			address,
			output: Some(bit.clone()),
			inputs: vec![shifted, const_u64(1, size)],
			asm_mnemonic: Some("TBIT_MASK".to_string()),
		});
		let cond = alloc_ctrl_tmp(&mut tmp, 1);
		ops.push(PcodeOp {
			seq_num: next_seq(&mut seq),
			opcode: if is_nonzero {
				PcodeOpcode::IntNotEqual
			} else {
				PcodeOpcode::IntEqual
			},
			address,
			output: Some(cond.clone()),
			inputs: vec![bit, const_u64(0, size)],
			asm_mnemonic: Some(if is_nonzero {
				"TBNZ_PRED".to_string()
			} else {
				"TBZ_PRED".to_string()
			}),
		});
		ops.push(PcodeOp {
			seq_num: next_seq(&mut seq),
			opcode: PcodeOpcode::CBranch,
			address,
			output: None,
			inputs: vec![Varnode::constant(target as i64, 8), cond],
			asm_mnemonic: Some(if is_nonzero {
				"TBNZ".to_string()
			} else {
				"TBZ".to_string()
			}),
		});
		return Some(ops);
	}

	if (word & 0xFF00_0010) == 0x5400_0000 {
		let imm19 = ((word >> 5) & 0x7F_FFFF) as i32;
		let signed = (imm19 << 13) >> 13;
		let disp = (signed as i64) << 2;
		let target = address.wrapping_add_signed(disp);
		let cond = (word & 0xF) as u32;
		if cond == 0xF {
			return None;
		}
		if cond == 0xE {
			return Some(vec![PcodeOp {
				seq_num: 1,
				opcode: PcodeOpcode::Branch,
				address,
				output: None,
				inputs: vec![Varnode::constant(target as i64, 8)],
				asm_mnemonic: Some("B.AL".to_string()),
			}]);
		}

		let mut seq = 1u32;
		let mut tmp = ctrl_tmp_base(address);
		let mut ops = Vec::new();
		let pred = emit_bcond_predicate(&mut ops, address, cond, &mut seq, &mut tmp)?;
		ops.push(PcodeOp {
			seq_num: next_seq(&mut seq),
			opcode: PcodeOpcode::CBranch,
			address,
			output: None,
			inputs: vec![Varnode::constant(target as i64, 8), pred],
			asm_mnemonic: Some("B.cond".to_string()),
		});
		return Some(ops);
	}

	None
}

fn next_seq(seq: &mut u32) -> u32 {
	let cur = *seq;
	*seq = seq.saturating_add(1);
	cur
}

fn ctrl_tmp_base(address: u64) -> u64 {
	0xD000_0000_0000_0000u64.wrapping_add(address.wrapping_shl(6))
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

fn emit_bcond_predicate(
	ops: &mut Vec<PcodeOp>,
	address: u64,
	cond: u32,
	seq: &mut u32,
	tmp: &mut u64,
) -> Option<Varnode> {
	let n = a64_flag_n();
	let z = a64_flag_z();
	let c = a64_flag_c();
	let v = a64_flag_v();

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
		0x0 => z,
		0x1 => bool_not(ops, z, "COND_NE", seq, tmp),
		0x2 => c,
		0x3 => bool_not(ops, c, "COND_CC", seq, tmp),
		0x4 => n,
		0x5 => bool_not(ops, n, "COND_PL", seq, tmp),
		0x6 => v,
		0x7 => bool_not(ops, v, "COND_VC", seq, tmp),
		0x8 => {
			let nz = bool_not(ops, z, "COND_NZ", seq, tmp);
			bool_and(ops, c, nz, "COND_HI", seq, tmp)
		}
		0x9 => {
			let nc = bool_not(ops, c, "COND_NC", seq, tmp);
			bool_or(ops, nc, z, "COND_LS", seq, tmp)
		}
		0xA => bool_eq(ops, n, v, "COND_GE", seq, tmp),
		0xB => bool_ne(ops, n, v, "COND_LT", seq, tmp),
		0xC => {
			let ge = bool_eq(ops, n, v, "COND_GE_CORE", seq, tmp);
			let nz = bool_not(ops, z, "COND_NZ", seq, tmp);
			bool_and(ops, ge, nz, "COND_GT", seq, tmp)
		}
		0xD => {
			let lt = bool_ne(ops, n, v, "COND_LT_CORE", seq, tmp);
			bool_or(ops, z, lt, "COND_LE", seq, tmp)
		}
		_ => return None,
	})
}

