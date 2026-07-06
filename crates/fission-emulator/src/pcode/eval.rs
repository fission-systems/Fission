use anyhow::{Result};
use fission_pcode::ir::{PcodeOp, PcodeOpcode};
use crate::pcode::state::MachineState;

pub struct Evaluator<'a> {
    pub state: &'a mut MachineState,
}

impl<'a> Evaluator<'a> {
    pub fn new(state: &'a mut MachineState) -> Self {
        Self { state }
    }

    /// Evaluates a single P-Code operation against the current machine state.
    pub fn step(&mut self, op: &PcodeOp) -> Result<()> {
        match op.opcode {
            PcodeOpcode::Copy => {
                let input = &op.inputs[0];
                let output = op.output.as_ref().expect("COPY must have an output");
                let mut data = vec![0u8; input.size as usize];
                if input.is_constant {
                    let val_bytes = input.constant_val.to_le_bytes();
                    data.copy_from_slice(&val_bytes[..input.size as usize]);
                } else {
                    data = self.state.read_space(input.space_id, input.offset, input.size as usize)?;
                }
                self.state.write_space(output.space_id, output.offset, &data)?;
            }
            PcodeOpcode::Store => {
                let space_id_to_store = op.inputs[0].constant_val as u64; // STORE usually takes space ID as first param
                let ptr_data = self.state.read_space(op.inputs[1].space_id, op.inputs[1].offset, op.inputs[1].size as usize)?;
                let mut dest_addr = 0u64;
                for (i, &b) in ptr_data.iter().enumerate() {
                    dest_addr |= (b as u64) << (i * 8);
                }
                
                let mut val_data = vec![0u8; op.inputs[2].size as usize];
                if op.inputs[2].is_constant {
                    let val_bytes = op.inputs[2].constant_val.to_le_bytes();
                    val_data.copy_from_slice(&val_bytes[..op.inputs[2].size as usize]);
                } else {
                    val_data = self.state.read_space(op.inputs[2].space_id, op.inputs[2].offset, op.inputs[2].size as usize)?;
                }
                self.state.write_space(space_id_to_store, dest_addr, &val_data)?;
            }
            PcodeOpcode::Load => {
                let space_id_to_load = op.inputs[0].constant_val as u64; // LOAD takes space ID as first param
                let ptr_data = self.state.read_space(op.inputs[1].space_id, op.inputs[1].offset, op.inputs[1].size as usize)?;
                let mut src_addr = 0u64;
                for (i, &b) in ptr_data.iter().enumerate() {
                    src_addr |= (b as u64) << (i * 8);
                }
                
                let output = op.output.as_ref().expect("LOAD must have an output");
                let val_data = self.state.read_space(space_id_to_load, src_addr, output.size as usize)?;
                self.state.write_space(output.space_id, output.offset, &val_data)?;
            }
            // More opcodes (INT_ADD, INT_SUB, BRANCH, CBRANCH) will be implemented here
            _ => {
                tracing::warn!("Unimplemented opcode: {:?}", op.opcode);
            }
        }
        Ok(())
    }
}
