use std::path::Path;

use anyhow::{Context, Result};
use fission_pcode::PcodeOp;
use sleigh_rs::pattern::{BitConstraint, Pattern};
use sleigh_rs::table::{Constructor, VariantId};
use sleigh_rs::Sleigh;

use crate::converter::IRConverter;

pub struct SleighLifter {
    sleigh_context: Sleigh,
}

pub struct DecodeState<'a> {
    pub bytes: &'a [u8],
    pub address: u64,
    context_bits: Vec<u8>,
}

impl<'a> DecodeState<'a> {
    pub fn new(bytes: &'a [u8], address: u64, context_bits_len: usize) -> Self {
        let context_bytes_len = context_bits_len.div_ceil(8);
        Self {
            bytes,
            address,
            context_bits: vec![0; context_bytes_len],
        }
    }

    fn instruction_bit(&self, bit_index: usize) -> Option<bool> {
        let byte_index = bit_index / 8;
        let bit_in_byte = bit_index % 8;
        let byte = *self.bytes.get(byte_index)?;
        Some(((byte >> bit_in_byte) & 1) != 0)
    }

    fn context_bit(&self, bit_index: usize) -> Option<bool> {
        let byte_index = bit_index / 8;
        let bit_in_byte = bit_index % 8;
        let byte = *self.context_bits.get(byte_index)?;
        Some(((byte >> bit_in_byte) & 1) != 0)
    }
}

impl SleighLifter {
    pub fn new(spec_path: &Path) -> Result<Self> {
        let sleigh_context = sleigh_rs::file_to_sleigh(spec_path)
            .map_err(|e| anyhow::anyhow!("Failed to parse Sleigh specs: {:?}", e))?;

        Ok(Self { sleigh_context })
    }

    /// Primary decoding function corresponding to Ghidra's Sleigh engine decoding.
    pub fn decode_and_lift(&self, bytes: &[u8], address: u64) -> Result<Vec<PcodeOp>> {
        let context_bits_len = usize::try_from(self.sleigh_context.context_memory().memory_bits)
            .context("Context bit length does not fit usize")?;
        let state = DecodeState::new(bytes, address, context_bits_len);
        let inst_table_id = self.sleigh_context.instruction_table();

        let inst_table = self.sleigh_context.table(inst_table_id);

        // matcher_order() is pre-sorted to follow Sleigh constructor matching priority.
        let matchers = inst_table.matcher_order();
        let mut matched_constructor = None;

        for matcher in matchers {
            let constructor = inst_table.constructor(matcher.constructor);
            let is_match = self
                .match_pattern(&state, constructor, matcher.variant_id)
                .with_context(|| {
                    format!(
                        "Failed to evaluate constructor pattern at address {:#x}",
                        address
                    )
                })?;

            if is_match {
                matched_constructor = Some(constructor);
                break;
            }
        }

        if let Some(constructor) = matched_constructor {
            let mut ops = Vec::new();
            if let Some(exec) = &constructor.execution {
                let mut converter = IRConverter::new();
                for block in exec.blocks() {
                    for stmt in &block.statements {
                        let converted_ops = converter
                            .convert_statement(stmt, address, &self.sleigh_context, exec)
                            .with_context(|| {
                                format!(
                                    "Failed to convert statement at address {:#x}",
                                    address
                                )
                            })?;
                        ops.extend(converted_ops);
                    }
                }
            }
            Ok(ops)
        } else {
            anyhow::bail!("No matching Sleigh constructor found at address {:#x}", address)
        }
    }

    fn match_pattern(
        &self,
        state: &DecodeState,
        constructor: &Constructor,
        variant_id: VariantId,
    ) -> Result<bool> {
        if !self.pattern_len_matches(state, &constructor.pattern) {
            return Ok(false);
        }

        let (context_constraints, token_constraints) = constructor.variant(variant_id);
        self.match_variant_constraints(state, context_constraints, token_constraints)
    }

    fn pattern_len_matches(&self, state: &DecodeState, pattern: &Pattern) -> bool {
        let available_bytes = state.bytes.len() as u64;
        if available_bytes < pattern.len.min() {
            return false;
        }
        if let Some(max) = pattern.len.max() {
            if available_bytes > max {
                return false;
            }
        }
        true
    }

    fn match_variant_constraints(
        &self,
        state: &DecodeState,
        context_constraints: &[BitConstraint],
        token_constraints: &[BitConstraint],
    ) -> Result<bool> {
        for (bit_index, constraint) in token_constraints.iter().enumerate() {
            match constraint {
                BitConstraint::Unrestrained => {}
                BitConstraint::Defined(expected) => {
                    let Some(actual) = state.instruction_bit(bit_index) else {
                        return Ok(false);
                    };
                    if actual != *expected {
                        return Ok(false);
                    }
                }
                BitConstraint::Restrained => {
                    anyhow::bail!(
                        "Unsupported restrained token bit constraint at bit {}",
                        bit_index
                    );
                }
            }
        }

        for (bit_index, constraint) in context_constraints.iter().enumerate() {
            match constraint {
                BitConstraint::Unrestrained => {}
                BitConstraint::Defined(expected) => {
                    let actual = state.context_bit(bit_index).unwrap_or(false);
                    if actual != *expected {
                        return Ok(false);
                    }
                }
                BitConstraint::Restrained => {
                    anyhow::bail!(
                        "Unsupported restrained context bit constraint at bit {}",
                        bit_index
                    );
                }
            }
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::DecodeState;

    #[test]
    fn instruction_bit_reads_lsb_first() {
        let state = DecodeState::new(&[0b0000_0101], 0, 0);
        assert_eq!(state.instruction_bit(0), Some(true));
        assert_eq!(state.instruction_bit(1), Some(false));
        assert_eq!(state.instruction_bit(2), Some(true));
    }

    #[test]
    fn out_of_range_instruction_bit_is_none() {
        let state = DecodeState::new(&[0x00], 0, 0);
        assert_eq!(state.instruction_bit(8), None);
    }
}
