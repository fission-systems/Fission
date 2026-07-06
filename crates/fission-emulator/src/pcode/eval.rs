use anyhow::{Result, bail};
use fission_pcode::ir::{PcodeOp, PcodeOpcode, Varnode};
use crate::pcode::state::MachineState;

pub enum StepResult {
    Next,
    Branch(u64),
}

pub struct Evaluator<'a> {
    pub state: &'a mut MachineState,
}

impl<'a> Evaluator<'a> {
    pub fn new(state: &'a mut MachineState) -> Self {
        Self { state }
    }

    fn read_varnode_u64(&mut self, vn: &Varnode) -> Result<u64> {
        if vn.is_constant {
            Ok(vn.constant_val as u64)
        } else {
            let data = self.state.read_space(vn.space_id, vn.offset, vn.size as usize)?;
            let mut val = 0u64;
            for (i, &b) in data.iter().enumerate() {
                val |= (b as u64) << (i * 8);
            }
            Ok(val)
        }
    }

    fn write_varnode_u64(&mut self, vn: &Varnode, val: u64) -> Result<()> {
        let val_bytes = val.to_le_bytes();
        self.state.write_space(vn.space_id, vn.offset, &val_bytes[..vn.size as usize])
    }

    /// Evaluates a single P-Code operation against the current machine state.
    pub fn step(&mut self, op: &PcodeOp) -> Result<StepResult> {
        match op.opcode {
            PcodeOpcode::Copy => {
                let val = self.read_varnode_u64(&op.inputs[0])?;
                let output = op.output.as_ref().expect("COPY must have an output");
                self.write_varnode_u64(output, val)?;
            }
            PcodeOpcode::Store => {
                let space_id_to_store = op.inputs[0].constant_val as u64;
                let dest_addr = self.read_varnode_u64(&op.inputs[1])?;
                let val = self.read_varnode_u64(&op.inputs[2])?;
                let val_bytes = val.to_le_bytes();
                self.state.write_space(space_id_to_store, dest_addr, &val_bytes[..op.inputs[2].size as usize])?;
            }
            PcodeOpcode::Load => {
                let space_id_to_load = op.inputs[0].constant_val as u64;
                let src_addr = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("LOAD must have an output");
                let val_data = self.state.read_space(space_id_to_load, src_addr, output.size as usize)?;
                self.state.write_space(output.space_id, output.offset, &val_data)?;
            }
            PcodeOpcode::IntAdd => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_ADD must have output");
                // Wrapping addition and masking according to output size
                let sum = val1.wrapping_add(val2);
                self.write_varnode_u64(output, sum)?;
            }
            PcodeOpcode::IntSub => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_SUB must have output");
                let diff = val1.wrapping_sub(val2);
                self.write_varnode_u64(output, diff)?;
            }
            PcodeOpcode::IntAnd => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_AND must have output");
                self.write_varnode_u64(output, val1 & val2)?;
            }
            PcodeOpcode::IntOr => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_OR must have output");
                self.write_varnode_u64(output, val1 | val2)?;
            }
            PcodeOpcode::IntXor => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_XOR must have output");
                self.write_varnode_u64(output, val1 ^ val2)?;
            }
            PcodeOpcode::IntLeft => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_LEFT must have output");
                self.write_varnode_u64(output, val1 << (val2 as u32))?;
            }
            PcodeOpcode::IntRight => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_RIGHT must have output");
                self.write_varnode_u64(output, val1 >> (val2 as u32))?;
            }
            PcodeOpcode::IntMult => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_MULT must have output");
                self.write_varnode_u64(output, val1.wrapping_mul(val2))?;
            }
            PcodeOpcode::IntEqual => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_EQUAL must have output");
                self.write_varnode_u64(output, if val1 == val2 { 1 } else { 0 })?;
            }
            PcodeOpcode::IntNotEqual => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_NOTEQUAL must have output");
                self.write_varnode_u64(output, if val1 != val2 { 1 } else { 0 })?;
            }
            PcodeOpcode::Branch => {
                let target = self.read_varnode_u64(&op.inputs[0])?;
                // If it is absolute branch:
                return Ok(StepResult::Branch(target));
            }
            PcodeOpcode::CBranch => {
                let target = self.read_varnode_u64(&op.inputs[0])?;
                let condition = self.read_varnode_u64(&op.inputs[1])?;
                if condition != 0 {
                    return Ok(StepResult::Branch(target));
                }
            }
            PcodeOpcode::Call => {
                let target = self.read_varnode_u64(&op.inputs[0])?;
                return Ok(StepResult::Branch(target)); // The loop will handle pushing RIP if it's a call
            }
            PcodeOpcode::CallInd | PcodeOpcode::BranchInd => {
                let target = self.read_varnode_u64(&op.inputs[0])?;
                return Ok(StepResult::Branch(target));
            }
            PcodeOpcode::Return => {
                let target = self.read_varnode_u64(&op.inputs[0])?; // Return typically gets target from indirection or stack? Wait, usually RETURN input 0 is the target.
                return Ok(StepResult::Branch(target));
            }
            PcodeOpcode::IntZExt => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?; // Zero extended automatically by read_varnode_u64
                let output = op.output.as_ref().expect("INT_ZEXT must have output");
                self.write_varnode_u64(output, val1)?;
            }
            PcodeOpcode::IntSExt => {
                let data = self.state.read_space(op.inputs[0].space_id, op.inputs[0].offset, op.inputs[0].size as usize)?;
                let mut val = 0i64;
                for (i, &b) in data.iter().enumerate() {
                    val |= (b as i64) << (i * 8);
                }
                // Sign extend
                let shift = 64 - (op.inputs[0].size * 8);
                let sext_val = (val << shift) >> shift;
                let output = op.output.as_ref().expect("INT_SEXT must have output");
                self.write_varnode_u64(output, sext_val as u64)?;
            }
            _ => {
                tracing::warn!("Unimplemented opcode: {:?}", op.opcode);
            }
        }
        Ok(StepResult::Next)
    }
}
