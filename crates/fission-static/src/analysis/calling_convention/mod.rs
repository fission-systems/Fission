pub mod infer;

use super::control_flow_facts::FunctionControlFlowFacts;
use fission_loader::loader::LoadedBinary;
use fission_core::CallingConvention;
use fission_sleigh::runtime::RuntimeSleighFrontend;
use std::collections::BTreeSet;

/// calling convention analyzer that tracks register use-def
/// to infer parameters and return registers.
pub struct CallingConventionAnalyzer<'a> {
    binary: &'a LoadedBinary,
    frontend: &'a RuntimeSleighFrontend,
}

impl<'a> CallingConventionAnalyzer<'a> {
    pub fn new(binary: &'a LoadedBinary, frontend: &'a RuntimeSleighFrontend) -> Self {
        Self { binary, frontend }
    }

    /// Analyzes the calling convention of a single function.
    pub fn analyze_function(
        &self,
        entry_address: u64,
        facts: &FunctionControlFlowFacts,
    ) -> Option<CallingConvention> {
        let mut read_before_write_registers = BTreeSet::new();
        let mut written_registers = BTreeSet::new();

        // Perform a conservative linear scan over instructions in flow-edges order.
        // We can just iterate all decoded operations starting from entry_address.
        // For a more robust DFA, we should trace blocks, but here we approximate
        // by tracking all instructions within the function bounds.

        // Get the bytes for the function.
        let mut max_bytes = 0;
        if let Some(&idx) = self.binary.inner().function_addr_index.get(&entry_address) {
            if let Some(info) = self.binary.inner().functions.get(idx) {
                max_bytes = info.size as usize;
            }
        }
        if max_bytes == 0 {
            // fallback
            max_bytes = 1024;
        }

        let Some(section) = self.binary.sections.iter().find(|s| {
            entry_address >= s.virtual_address && entry_address < s.virtual_address + s.virtual_size
        }) else {
            return None;
        };

        let file_offset =
            (entry_address - section.virtual_address) as usize + section.file_offset as usize;
        let bytes_avail = self
            .binary
            .data
            .as_slice()
            .len()
            .saturating_sub(file_offset)
            .min(max_bytes);
        if bytes_avail == 0 {
            return None;
        }

        let bytes = &self.binary.data.as_slice()[file_offset..file_offset + bytes_avail];

        if let Ok(ops) = self.frontend.decode_and_lift(bytes, entry_address) {
            for op in ops {
                // Check inputs (Reads)
                for input in &op.inputs {
                    if let Some(compiled) = self.frontend.compiled_frontend() {
                        if input.space_id == compiled.sla_register_space_index {
                            if !written_registers.contains(&input.offset) {
                                read_before_write_registers.insert(input.offset);
                            }
                        }
                    }
                }

                // Check outputs (Writes)
                if let Some(output) = &op.output {
                    if let Some(compiled) = self.frontend.compiled_frontend() {
                        if output.space_id == compiled.sla_register_space_index {
                            written_registers.insert(output.offset);
                        }
                    }
                }
            }
        }

        // Now infer the calling convention from the read registers.
        infer::infer_calling_convention(self.binary, &read_before_write_registers)
    }
}
