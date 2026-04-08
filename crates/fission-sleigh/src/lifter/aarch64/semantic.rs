use fission_pcode::PcodeOp;

use super::common::A64TempFactory;

mod arithmetic;
mod logical;
mod memory;
mod misc;

use arithmetic::{decode_add_sub_imm, decode_add_sub_reg};
use logical::{decode_logical_imm, decode_logical_shifted_reg};
use memory::{decode_ldst_pair, decode_ldst_unscaled_or_indexed, decode_ldst_unsigned_imm};
use misc::{decode_adrp, decode_move_wide};

pub(crate) fn decode_semantic_with_state(
	insn: &[u8],
	address: u64,
	seq_start: u32,
	temp_base: u64,
) -> Vec<PcodeOp> {
	if insn.len() < 4 {
		return Vec::new();
	}

	let word = u32::from_le_bytes([insn[0], insn[1], insn[2], insn[3]]);
	let mut temp = A64TempFactory::with_base(temp_base);
	let mut seq = seq_start;

	if let Some(ops) = decode_add_sub_imm(word, address, &mut temp, &mut seq) {
		return ops;
	}
	if let Some(ops) = decode_add_sub_reg(word, address, &mut temp, &mut seq) {
		return ops;
	}
	if let Some(ops) = decode_logical_shifted_reg(word, address, &mut temp, &mut seq) {
		return ops;
	}
	if let Some(ops) = decode_logical_imm(word, address, &mut temp, &mut seq) {
		return ops;
	}
	if let Some(ops) = decode_ldst_unscaled_or_indexed(word, address, &mut temp, &mut seq) {
		return ops;
	}
	if let Some(ops) = decode_move_wide(word, address, &mut temp, &mut seq) {
		return ops;
	}
	if let Some(ops) = decode_adrp(word, address, &mut seq) {
		return ops;
	}
	if let Some(ops) = decode_ldst_unsigned_imm(word, address, &mut temp, &mut seq) {
		return ops;
	}
	if let Some(ops) = decode_ldst_pair(word, address, &mut temp, &mut seq) {
		return ops;
	}

	Vec::new()
}
