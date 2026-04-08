use fission_pcode::{PcodeOp, PcodeOpcode};

use super::super::common::{a64_reg, const_u64, sign_extend, A64TempFactory};

pub(super) fn decode_move_wide(
	word: u32,
	address: u64,
	temp: &mut A64TempFactory,
	seq: &mut u32,
) -> Option<Vec<PcodeOp>> {
	if (word & 0x1F80_0000) != 0x1280_0000 {
		return None;
	}

	let sf = ((word >> 31) & 1) != 0;
	let size = if sf { 8 } else { 4 };
	let opc = (word >> 29) & 0x3;
	if opc == 1 {
		return None;
	}
	let hw = ((word >> 21) & 0x3) as u64;
	if !sf && hw > 1 {
		return None;
	}
	let imm16 = ((word >> 5) & 0xFFFF) as u64;
	let rd = word & 0x1F;
	let shift = hw * 16;
	let width_mask = if size == 8 {
		u64::MAX
	} else {
		(1u64 << (size * 8)) - 1
	};
	let imm_shifted = (imm16 << shift) & width_mask;

	match opc {
		0 => {
			let value = (!imm_shifted) & width_mask;
			Some(vec![PcodeOp {
				seq_num: {
					let s = *seq;
					*seq = seq.saturating_add(1);
					s
				},
				opcode: PcodeOpcode::Copy,
				address,
				output: Some(a64_reg(rd, size)),
				inputs: vec![const_u64(value, size)],
				asm_mnemonic: Some("MOVN".to_string()),
			}])
		}
		2 => Some(vec![PcodeOp {
			seq_num: {
				let s = *seq;
				*seq = seq.saturating_add(1);
				s
			},
			opcode: PcodeOpcode::Copy,
			address,
			output: Some(a64_reg(rd, size)),
			inputs: vec![const_u64(imm_shifted, size)],
			asm_mnemonic: Some("MOVZ".to_string()),
		}]),
		3 => {
			let clear_mask = (!(0xFFFFu64 << shift)) & width_mask;
			let tmp = temp.alloc(size);
			Some(vec![
				PcodeOp {
					seq_num: {
						let s = *seq;
						*seq = seq.saturating_add(1);
						s
					},
					opcode: PcodeOpcode::IntAnd,
					address,
					output: Some(tmp.clone()),
					inputs: vec![a64_reg(rd, size), const_u64(clear_mask, size)],
					asm_mnemonic: Some("MOVK_CLR".to_string()),
				},
				PcodeOp {
					seq_num: {
						let s = *seq;
						*seq = seq.saturating_add(1);
						s
					},
					opcode: PcodeOpcode::IntOr,
					address,
					output: Some(a64_reg(rd, size)),
					inputs: vec![tmp, const_u64(imm_shifted, size)],
					asm_mnemonic: Some("MOVK".to_string()),
				},
			])
		}
		_ => None,
	}
}

pub(super) fn decode_adrp(word: u32, address: u64, seq: &mut u32) -> Option<Vec<PcodeOp>> {
	if (word & 0x9F00_0000) != 0x9000_0000 {
		return None;
	}

	let rd = word & 0x1F;
	let immlo = ((word >> 29) & 0x3) as i64;
	let immhi = ((word >> 5) & 0x7F_FFFF) as i64;
	let imm21 = sign_extend((immhi << 2) | immlo, 21);
	let page = address & !0xFFF;
	let target = page.wrapping_add_signed(imm21 << 12);

	Some(vec![PcodeOp {
		seq_num: {
			let s = *seq;
			*seq = seq.saturating_add(1);
			s
		},
		opcode: PcodeOpcode::Copy,
		address,
		output: Some(a64_reg(rd, 8)),
		inputs: vec![const_u64(target, 8)],
		asm_mnemonic: Some("ADRP".to_string()),
	}])
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn decode_movz_copy_immediate() {
		// MOVZ X0, #1
		let word = 0xD280_0020;
		let mut temp = A64TempFactory::new(0x1000);
		let mut seq = 1u32;
		let ops = decode_move_wide(word, 0x1000, &mut temp, &mut seq)
			.expect("expected move-wide decode");

		assert_eq!(ops.len(), 1);
		assert_eq!(ops[0].opcode, PcodeOpcode::Copy);
		assert_eq!(ops[0].asm_mnemonic.as_deref(), Some("MOVZ"));
		assert_eq!(ops[0].inputs.len(), 1);
		assert!(ops[0].inputs[0].is_constant);
		assert_eq!(ops[0].inputs[0].constant_val, 1);
	}

	#[test]
	fn decode_adrp_zero_immediate_page_base() {
		// ADRP X3, <current page>
		let word = 0x9000_0003;
		let mut seq = 1u32;
		let ops = decode_adrp(word, 0x1234, &mut seq).expect("expected ADRP decode");

		assert_eq!(ops.len(), 1);
		assert_eq!(ops[0].opcode, PcodeOpcode::Copy);
		assert_eq!(ops[0].asm_mnemonic.as_deref(), Some("ADRP"));
		assert_eq!(ops[0].inputs.len(), 1);
		assert!(ops[0].inputs[0].is_constant);
		assert_eq!(ops[0].inputs[0].constant_val, 0x1000);
	}
}
