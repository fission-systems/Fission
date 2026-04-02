use std::ops::Range;

use anyhow::{Context, Result};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};
use sleigh_rs::execution::Unary;
use sleigh_rs::NumberUnsigned;

use super::IRConverter;

impl IRConverter {
    pub(super) fn lower_unary(
        &mut self,
        unary: Unary,
        input: Varnode,
        current_address: u64,
        emitted: &mut Vec<PcodeOp>,
    ) -> Result<Varnode> {
        match unary {
            Unary::Negation => {
                let out = self.make_temp_varnode(self.next_seq, input.size);
                emitted.push(PcodeOp {
                    seq_num: self.take_seq(),
                    opcode: PcodeOpcode::BoolNegate,
                    address: current_address,
                    output: Some(out.clone()),
                    inputs: vec![input],
                    asm_mnemonic: Some("UNARY_BOOLNEG".to_string()),
                });
                Ok(out)
            }
            Unary::BitNegation => {
                let out = self.make_temp_varnode(self.next_seq, input.size);
                emitted.push(PcodeOp {
                    seq_num: self.take_seq(),
                    opcode: PcodeOpcode::IntNegate,
                    address: current_address,
                    output: Some(out.clone()),
                    inputs: vec![input],
                    asm_mnemonic: Some("UNARY_BITNEG".to_string()),
                });
                Ok(out)
            }
            Unary::Negative => {
                let out = self.make_temp_varnode(self.next_seq, input.size);
                emitted.push(PcodeOp {
                    seq_num: self.take_seq(),
                    opcode: PcodeOpcode::Int2Comp,
                    address: current_address,
                    output: Some(out.clone()),
                    inputs: vec![input],
                    asm_mnemonic: Some("UNARY_NEG".to_string()),
                });
                Ok(out)
            }
            Unary::Zext(bits) => {
                let out_size =
                    Self::bits_to_bytes(bits.get()).context("Invalid zext output size")?;
                let out = self.make_temp_varnode(self.next_seq, out_size);
                emitted.push(PcodeOp {
                    seq_num: self.take_seq(),
                    opcode: PcodeOpcode::IntZExt,
                    address: current_address,
                    output: Some(out.clone()),
                    inputs: vec![input],
                    asm_mnemonic: Some("UNARY_ZEXT".to_string()),
                });
                Ok(out)
            }
            Unary::Sext(bits) => {
                let out_size =
                    Self::bits_to_bytes(bits.get()).context("Invalid sext output size")?;
                let out = self.make_temp_varnode(self.next_seq, out_size);
                emitted.push(PcodeOp {
                    seq_num: self.take_seq(),
                    opcode: PcodeOpcode::IntSExt,
                    address: current_address,
                    output: Some(out.clone()),
                    inputs: vec![input],
                    asm_mnemonic: Some("UNARY_SEXT".to_string()),
                });
                Ok(out)
            }
            Unary::TakeLsb(len) => {
                let out_size =
                    u32::try_from(len.get()).context("TakeLsb size does not fit u32")?;
                let out = self.make_temp_varnode(self.next_seq, out_size);
                emitted.push(PcodeOp {
                    seq_num: self.take_seq(),
                    opcode: PcodeOpcode::SubPiece,
                    address: current_address,
                    output: Some(out.clone()),
                    inputs: vec![input, Varnode::constant(0, 4)],
                    asm_mnemonic: Some("UNARY_TAKELSB".to_string()),
                });
                Ok(out)
            }
            Unary::TrunkLsb { trunk, bits } => {
                let out_size =
                    Self::bits_to_bytes(bits.get()).context("Invalid trunk output size")?;
                let out = self.make_temp_varnode(self.next_seq, out_size);
                emitted.push(PcodeOp {
                    seq_num: self.take_seq(),
                    opcode: PcodeOpcode::SubPiece,
                    address: current_address,
                    output: Some(out.clone()),
                    inputs: vec![input, Varnode::constant(trunk as i64, 4)],
                    asm_mnemonic: Some("UNARY_TRUNKLSB".to_string()),
                });
                Ok(out)
            }
            Unary::BitRange { range, bits } => {
                self.extract_bit_range(input, range, bits.get(), current_address, emitted)
            }
            Unary::Dereference(mem) => {
                self.lower_dereference(&mem, input, current_address, emitted)
            }
            _ => anyhow::bail!("Unsupported unary op in converter MVP: {:?}", unary),
        }
    }

    pub(super) fn extract_bit_range(
        &mut self,
        input: Varnode,
        range: Range<NumberUnsigned>,
        bits: NumberUnsigned,
        current_address: u64,
        emitted: &mut Vec<PcodeOp>,
    ) -> Result<Varnode> {
        if bits == 0 {
            anyhow::bail!("BitRange cannot have zero width");
        }

        let bit_offset = range.start % 8;
        let byte_offset = range.start / 8;
        let mut working = input;

        if bit_offset != 0 {
            let shifted = self.make_temp_varnode(self.next_seq, working.size);
            emitted.push(PcodeOp {
                seq_num: self.take_seq(),
                opcode: PcodeOpcode::IntRight,
                address: current_address,
                output: Some(shifted.clone()),
                inputs: vec![working, Varnode::constant(bit_offset as i64, 4)],
                asm_mnemonic: Some("BITRANGE_SHIFT".to_string()),
            });
            working = shifted;
        }

        let out_size = Self::bits_to_bytes(bits).context("Invalid BitRange size")?;
        let extracted = self.make_temp_varnode(self.next_seq, out_size);
        emitted.push(PcodeOp {
            seq_num: self.take_seq(),
            opcode: PcodeOpcode::SubPiece,
            address: current_address,
            output: Some(extracted.clone()),
            inputs: vec![working, Varnode::constant(byte_offset as i64, 4)],
            asm_mnemonic: Some("BITRANGE_SUBPIECE".to_string()),
        });

        if bits % 8 != 0 {
            let effective_bits = bits.min(63);
            let mask = ((1u64 << effective_bits) - 1) as i64;
            let masked = self.make_temp_varnode(self.next_seq, out_size);
            emitted.push(PcodeOp {
                seq_num: self.take_seq(),
                opcode: PcodeOpcode::IntAnd,
                address: current_address,
                output: Some(masked.clone()),
                inputs: vec![extracted, Varnode::constant(mask, out_size)],
                asm_mnemonic: Some("BITRANGE_MASK".to_string()),
            });
            Ok(masked)
        } else {
            Ok(extracted)
        }
    }
}
