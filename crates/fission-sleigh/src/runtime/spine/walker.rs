use super::RuntimeConstructNode;
use anyhow::{anyhow, bail, Result};

#[derive(Debug, Clone)]
pub struct RuntimeParserWalker {
    construct_nodes: Vec<RuntimeConstructNode>,
}

impl RuntimeParserWalker {
    pub fn new(root_offset: usize, root_length: usize) -> Self {
        Self {
            construct_nodes: vec![RuntimeConstructNode {
                operand_index: None,
                parent_index: None,
                absolute_offset: root_offset,
                relative_length: root_length,
                handle_index: None,
            }],
        }
    }

    pub fn record_operand_node(
        &mut self,
        operand_index: usize,
        parent_index: usize,
        absolute_offset: usize,
        relative_length: usize,
        handle_index: usize,
    ) {
        self.construct_nodes.push(RuntimeConstructNode {
            operand_index: Some(operand_index),
            parent_index: Some(parent_index),
            absolute_offset,
            relative_length,
            handle_index: Some(handle_index),
        });
    }

    /// Extract instruction bits at `[start_bit, start_bit + bit_size)` from the
    /// instruction byte stream.  This is the SLA-native equivalent of Ghidra's
    /// `ParserWalker::getInstructionBits(startbit, size)` (slghsymbol.cc:2293).
    ///
    /// Bit numbering follows Ghidra's little-endian convention:
    /// - bit 0 is the LSB of byte 0 of the instruction.
    /// - Bits are extracted MSB-first within each bit-slice group.
    ///
    /// The result is zero-extended to u64. Sign extension is the caller's
    /// responsibility (use `sign_bit` from the SLA token field spec).
    pub fn instruction_bits(&self, bytes: &[u8], start_bit: u32, bit_size: u32) -> Result<u64> {
        if bit_size > 64 {
            bail!("instruction bit read width {bit_size} exceeds u64");
        }
        let end_bit = start_bit
            .checked_add(bit_size)
            .ok_or_else(|| anyhow!("instruction bit range overflow"))?;
        let required_bytes = end_bit.div_ceil(8) as usize;
        if bytes.len() < required_bytes {
            bail!(
                "instruction bit read [{}..{}) requires {required_bytes} bytes, got {}",
                start_bit,
                end_bit,
                bytes.len()
            );
        }
        let mut result = 0u64;
        for i in 0..bit_size {
            let bit_pos = start_bit + i;
            let byte_idx = (bit_pos / 8) as usize;
            let bit_in_byte = bit_pos % 8;
            let bit = (bytes[byte_idx] >> bit_in_byte) & 1;
            result |= u64::from(bit) << i;
        }
        Ok(result)
    }

    pub fn into_nodes(self) -> Vec<RuntimeConstructNode> {
        self.construct_nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instruction_bits_reads_available_instruction_bits() {
        let walker = RuntimeParserWalker::new(0, 1);

        assert_eq!(
            walker.instruction_bits(&[0b1010_1100], 2, 4).expect("bits"),
            0b1011
        );
    }

    #[test]
    fn instruction_bits_fails_closed_on_short_instruction_bytes() {
        let walker = RuntimeParserWalker::new(0, 1);

        let err = walker
            .instruction_bits(&[0xff], 8, 1)
            .expect_err("short byte window must not be zero-padded");

        assert!(
            err.to_string().contains("requires 2 bytes, got 1"),
            "{err:#}"
        );
    }

    #[test]
    fn instruction_bits_fails_closed_on_oversized_result_width() {
        let walker = RuntimeParserWalker::new(0, 9);

        let err = walker
            .instruction_bits(&[0xff; 9], 0, 65)
            .expect_err("oversized bit read must fail");

        assert!(err.to_string().contains("exceeds u64"), "{err:#}");
    }
}
