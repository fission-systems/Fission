use super::RuntimeConstructNode;

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
    pub fn instruction_bits(&self, bytes: &[u8], start_bit: u32, bit_size: u32) -> u64 {
        let mut result = 0u64;
        for i in 0..bit_size {
            let bit_pos = start_bit + i;
            let byte_idx = (bit_pos / 8) as usize;
            let bit_in_byte = bit_pos % 8;
            let bit = bytes
                .get(byte_idx)
                .map(|b| (b >> bit_in_byte) & 1)
                .unwrap_or(0);
            result |= u64::from(bit) << i;
        }
        result
    }

    pub fn into_nodes(self) -> Vec<RuntimeConstructNode> {
        self.construct_nodes
    }
}
