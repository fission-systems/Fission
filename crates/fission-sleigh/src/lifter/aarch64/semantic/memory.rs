use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use super::super::super::common::{
	a64_reg, const_u64, sign_extend, A64TempFactory, RAM_SPACE_ID,
};

pub(super) fn decode_ldst_unscaled_or_indexed(
	word: u32,
	address: u64,
	temp: &mut A64TempFactory,
	seq: &mut u32,
) -> Option<Vec<PcodeOp>> {
	if (word & 0x3B00_0000) != 0x3800_0000 {
		return None;
	}

	let size_code = (word >> 30) & 0x3;
	let elem_size = 1u32 << size_code;
	if !matches!(elem_size, 1 | 2 | 4 | 8) {
		return None;
	}

	let opc = (word >> 22) & 0x3;
	if opc > 1 {
		return None;
	}

	let addr_mode = (word >> 10) & 0x3;
	if addr_mode == 0b10 {
		return None;
	}

	let imm9 = ((word >> 12) & 0x1FF) as i64;
	let signed_off = sign_extend(imm9, 9);
	let rn = (word >> 5) & 0x1F;
	let rt = word & 0x1F;
	let is_load = opc == 1;

	let mut ops = Vec::new();
	let base = a64_reg(rn, 8);

	let mut access_addr = base.clone();
	let mut wb_addr: Option<Varnode> = None;

	if addr_mode == 0b00 {
		if signed_off != 0 {
			let tmp = temp.alloc(8);
			ops.push(PcodeOp {
				seq_num: {
					let s = *seq;
					*seq = seq.saturating_add(1);
					s
				},
				opcode: if signed_off < 0 {
					PcodeOpcode::IntSub
				} else {
					PcodeOpcode::IntAdd
				},
				address,
				output: Some(tmp.clone()),
				inputs: vec![
					base,
					const_u64(
						if signed_off < 0 {
							signed_off.unsigned_abs()
						} else {
							signed_off as u64
						},
						8,
					),
				],
				asm_mnemonic: Some("LDSTUR_ADDR".to_string()),
			});
			access_addr = tmp;
		}
	} else if addr_mode == 0b01 {
		access_addr = base.clone();
		let tmp = if signed_off == 0 {
			base
		} else {
			let t = temp.alloc(8);
			ops.push(PcodeOp {
				seq_num: {
					let s = *seq;
					*seq = seq.saturating_add(1);
					s
				},
				opcode: if signed_off < 0 {
					PcodeOpcode::IntSub
				} else {
					PcodeOpcode::IntAdd
				},
				address,
				output: Some(t.clone()),
				inputs: vec![
					base,
					const_u64(
						if signed_off < 0 {
							signed_off.unsigned_abs()
						} else {
							signed_off as u64
						},
						8,
					),
				],
				asm_mnemonic: Some("LDST_POST_WB_ADDR".to_string()),
			});
			t
		};
		wb_addr = Some(tmp);
	} else if addr_mode == 0b11 {
		let tmp = if signed_off == 0 {
			base
		} else {
			let t = temp.alloc(8);
			ops.push(PcodeOp {
				seq_num: {
					let s = *seq;
					*seq = seq.saturating_add(1);
					s
				},
				opcode: if signed_off < 0 {
					PcodeOpcode::IntSub
				} else {
					PcodeOpcode::IntAdd
				},
				address,
				output: Some(t.clone()),
				inputs: vec![
					base,
					const_u64(
						if signed_off < 0 {
							signed_off.unsigned_abs()
						} else {
							signed_off as u64
						},
						8,
					),
				],
				asm_mnemonic: Some("LDST_PRE_WB_ADDR".to_string()),
			});
			t
		};
		access_addr = tmp.clone();
		wb_addr = Some(tmp);
	}

	if is_load {
		ops.push(PcodeOp {
			seq_num: {
				let s = *seq;
				*seq = seq.saturating_add(1);
				s
			},
			opcode: PcodeOpcode::Load,
			address,
			output: Some(a64_reg(rt, elem_size)),
			inputs: vec![const_u64(RAM_SPACE_ID, 8), access_addr],
			asm_mnemonic: Some(match addr_mode {
				0b00 => "LDUR".to_string(),
				0b01 => "LDR_POST".to_string(),
				_ => "LDR_PRE".to_string(),
			}),
		});
	} else {
		ops.push(PcodeOp {
			seq_num: {
				let s = *seq;
				*seq = seq.saturating_add(1);
				s
			},
			opcode: PcodeOpcode::Store,
			address,
			output: None,
			inputs: vec![const_u64(RAM_SPACE_ID, 8), access_addr, a64_reg(rt, elem_size)],
			asm_mnemonic: Some(match addr_mode {
				0b00 => "STUR".to_string(),
				0b01 => "STR_POST".to_string(),
				_ => "STR_PRE".to_string(),
			}),
		});
	}

	if let Some(wb) = wb_addr {
		ops.push(PcodeOp {
			seq_num: {
				let s = *seq;
				*seq = seq.saturating_add(1);
				s
			},
			opcode: PcodeOpcode::Copy,
			address,
			output: Some(a64_reg(rn, 8)),
			inputs: vec![wb],
			asm_mnemonic: Some("WB".to_string()),
		});
	}

	Some(ops)
}
pub(super) fn decode_ldst_unsigned_imm(
	word: u32,
	address: u64,
	temp: &mut A64TempFactory,
	seq: &mut u32,
) -> Option<Vec<PcodeOp>> {
	if (word & 0x3B00_0000) != 0x3900_0000 {
		return None;
	}

	let size_code = (word >> 30) & 0x3;
	let elem_size = 1u32 << size_code;
	if !matches!(elem_size, 1 | 2 | 4 | 8) {
		return None;
	}

	let opc = (word >> 22) & 0x3;
	if opc > 1 {
		return None;
	}

	let imm12 = ((word >> 10) & 0x0FFF) as u64;
	let offset = imm12 * u64::from(elem_size);
	let rn = (word >> 5) & 0x1F;
	let rt = word & 0x1F;

	let mut ops = Vec::new();
	let mut addr_vn = a64_reg(rn, 8);
	if offset != 0 {
		let tmp = temp.alloc(8);
		ops.push(PcodeOp {
			seq_num: {
				let s = *seq;
				*seq = seq.saturating_add(1);
				s
			},
			opcode: PcodeOpcode::IntAdd,
			address,
			output: Some(tmp.clone()),
			inputs: vec![addr_vn, const_u64(offset, 8)],
			asm_mnemonic: Some("ADDR".to_string()),
		});
		addr_vn = tmp;
	}

	if opc == 0 {
		ops.push(PcodeOp {
			seq_num: {
				let s = *seq;
				*seq = seq.saturating_add(1);
				s
			},
			opcode: PcodeOpcode::Store,
			address,
			output: None,
			inputs: vec![const_u64(RAM_SPACE_ID, 8), addr_vn, a64_reg(rt, elem_size)],
			asm_mnemonic: Some("STR".to_string()),
		});
	} else {
		ops.push(PcodeOp {
			seq_num: {
				let s = *seq;
				*seq = seq.saturating_add(1);
				s
			},
			opcode: PcodeOpcode::Load,
			address,
			output: Some(a64_reg(rt, elem_size)),
			inputs: vec![const_u64(RAM_SPACE_ID, 8), addr_vn],
			asm_mnemonic: Some("LDR".to_string()),
		});
	}

	Some(ops)
}
pub(super) fn decode_ldst_pair(
	word: u32,
	address: u64,
	temp: &mut A64TempFactory,
	seq: &mut u32,
) -> Option<Vec<PcodeOp>> {
	if (word & 0x3B00_0000) != 0x2900_0000 {
		return None;
	}

	let size_code = (word >> 30) & 0x3;
	let elem_size = match size_code {
		0 => 4,
		2 => 8,
		_ => return None,
	};

	let is_load = ((word >> 22) & 1) != 0;
	let addr_mode = (word >> 23) & 0x3;
	let is_post = addr_mode == 0b01;
	let is_offset = addr_mode == 0b10;
	let is_pre = addr_mode == 0b11;
	if !(is_post || is_offset || is_pre) {
		return None;
	}

	let imm7 = ((word >> 15) & 0x7F) as i64;
	let signed_imm7 = sign_extend(imm7, 7);
	let offset = signed_imm7 * i64::from(elem_size);
	let rt2 = (word >> 10) & 0x1F;
	let rn = (word >> 5) & 0x1F;
	let rt = word & 0x1F;

	let mut ops = Vec::new();
	let base_reg = a64_reg(rn, 8);
	let mut base_addr = base_reg.clone();
	let mut wb_addr: Option<Varnode> = None;

	if is_offset {
		if offset != 0 {
			let addr0 = temp.alloc(8);
			let imm = if offset < 0 {
				const_u64(offset.unsigned_abs(), 8)
			} else {
				const_u64(offset as u64, 8)
			};
			ops.push(PcodeOp {
				seq_num: {
					let s = *seq;
					*seq = seq.saturating_add(1);
					s
				},
				opcode: if offset < 0 {
					PcodeOpcode::IntSub
				} else {
					PcodeOpcode::IntAdd
				},
				address,
				output: Some(addr0.clone()),
				inputs: vec![base_reg, imm],
				asm_mnemonic: Some("PAIR_ADDR".to_string()),
			});
			base_addr = addr0;
		}
	} else if is_pre {
		let wb = if offset == 0 {
			base_reg.clone()
		} else {
			let t = temp.alloc(8);
			ops.push(PcodeOp {
				seq_num: {
					let s = *seq;
					*seq = seq.saturating_add(1);
					s
				},
				opcode: if offset < 0 {
					PcodeOpcode::IntSub
				} else {
					PcodeOpcode::IntAdd
				},
				address,
				output: Some(t.clone()),
				inputs: vec![
					base_reg.clone(),
					const_u64(
						if offset < 0 {
							offset.unsigned_abs()
						} else {
							offset as u64
						},
						8,
					),
				],
				asm_mnemonic: Some("PAIR_PRE_WB_ADDR".to_string()),
			});
			t
		};
		base_addr = wb.clone();
		wb_addr = Some(wb);
	} else if is_post {
		base_addr = base_reg.clone();
		let wb = if offset == 0 {
			base_reg.clone()
		} else {
			let t = temp.alloc(8);
			ops.push(PcodeOp {
				seq_num: {
					let s = *seq;
					*seq = seq.saturating_add(1);
					s
				},
				opcode: if offset < 0 {
					PcodeOpcode::IntSub
				} else {
					PcodeOpcode::IntAdd
				},
				address,
				output: Some(t.clone()),
				inputs: vec![
					base_reg,
					const_u64(
						if offset < 0 {
							offset.unsigned_abs()
						} else {
							offset as u64
						},
						8,
					),
				],
				asm_mnemonic: Some("PAIR_POST_WB_ADDR".to_string()),
			});
			t
		};
		wb_addr = Some(wb);
	}

	let addr1 = temp.alloc(8);
	ops.push(PcodeOp {
		seq_num: {
			let s = *seq;
			*seq = seq.saturating_add(1);
			s
		},
		opcode: PcodeOpcode::IntAdd,
		address,
		output: Some(addr1.clone()),
		inputs: vec![base_addr.clone(), const_u64(u64::from(elem_size), 8)],
		asm_mnemonic: Some("PAIR_ADDR_NEXT".to_string()),
	});

	if is_load {
		ops.push(PcodeOp {
			seq_num: {
				let s = *seq;
				*seq = seq.saturating_add(1);
				s
			},
			opcode: PcodeOpcode::Load,
			address,
			output: Some(a64_reg(rt, elem_size)),
			inputs: vec![const_u64(RAM_SPACE_ID, 8), base_addr],
			asm_mnemonic: Some(if is_pre {
				"LDP_PRE".to_string()
			} else if is_post {
				"LDP_POST".to_string()
			} else {
				"LDP".to_string()
			}),
		});
		ops.push(PcodeOp {
			seq_num: {
				let s = *seq;
				*seq = seq.saturating_add(1);
				s
			},
			opcode: PcodeOpcode::Load,
			address,
			output: Some(a64_reg(rt2, elem_size)),
			inputs: vec![const_u64(RAM_SPACE_ID, 8), addr1],
			asm_mnemonic: Some(if is_pre {
				"LDP_PRE".to_string()
			} else if is_post {
				"LDP_POST".to_string()
			} else {
				"LDP".to_string()
			}),
		});
	} else {
		ops.push(PcodeOp {
			seq_num: {
				let s = *seq;
				*seq = seq.saturating_add(1);
				s
			},
			opcode: PcodeOpcode::Store,
			address,
			output: None,
			inputs: vec![const_u64(RAM_SPACE_ID, 8), base_addr, a64_reg(rt, elem_size)],
			asm_mnemonic: Some(if is_pre {
				"STP_PRE".to_string()
			} else if is_post {
				"STP_POST".to_string()
			} else {
				"STP".to_string()
			}),
		});
		ops.push(PcodeOp {
			seq_num: {
				let s = *seq;
				*seq = seq.saturating_add(1);
				s
			},
			opcode: PcodeOpcode::Store,
			address,
			output: None,
			inputs: vec![const_u64(RAM_SPACE_ID, 8), addr1, a64_reg(rt2, elem_size)],
			asm_mnemonic: Some(if is_pre {
				"STP_PRE".to_string()
			} else if is_post {
				"STP_POST".to_string()
			} else {
				"STP".to_string()
			}),
		});
	}

	if let Some(wb) = wb_addr {
		ops.push(PcodeOp {
			seq_num: {
				let s = *seq;
				*seq = seq.saturating_add(1);
				s
			},
			opcode: PcodeOpcode::Copy,
			address,
			output: Some(a64_reg(rn, 8)),
			inputs: vec![wb],
			asm_mnemonic: Some("PAIR_WB".to_string()),
		});
	}

	Some(ops)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn decode_ldr_unsigned_immediate_load() {
		// LDR W0, [X1, #4]
		let word = 0xB940_0420;
		let mut temp = A64TempFactory::new(0x1000);
		let mut seq = 1u32;
		let ops = decode_ldst_unsigned_imm(word, 0x1000, &mut temp, &mut seq)
			.expect("expected unsigned-imm load/store decode");

		assert!(ops.iter().any(|op| {
			op.opcode == PcodeOpcode::Load && op.asm_mnemonic.as_deref() == Some("LDR")
		}));
	}

	#[test]
	fn decode_stp_preindexed_pair_store_with_writeback() {
		// STP X29, X30, [SP, #-16]!
		let word = 0xA9BF_7BFD;
		let mut temp = A64TempFactory::new(0x1000);
		let mut seq = 1u32;
		let ops = decode_ldst_pair(word, 0x1000, &mut temp, &mut seq)
			.expect("expected pair load/store decode");

		assert!(ops.iter().filter(|op| op.opcode == PcodeOpcode::Store).count() >= 2);
		assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("STP_PRE")));
		assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("PAIR_WB")));
	}
}
