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
                tracing::debug!("      COPY Src(space={}, offset=0x{:X}, size={}, is_const={}) -> Dest(space={}, offset=0x{:X}, size={}) Val=0x{:X}", 
                    op.inputs[0].space_id, op.inputs[0].offset, op.inputs[0].size, op.inputs[0].is_constant,
                    output.space_id, output.offset, output.size, val);
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
                let mut val = 0u64;
                for (i, &b) in val_data.iter().enumerate() {
                    val |= (b as u64) << (i * 8);
                }
                tracing::debug!("      LOAD Space: {} Addr: 0x{:X} -> Val: 0x{:X}", space_id_to_load, src_addr, val);
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
            PcodeOpcode::IntSDiv => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_SDIV must have output");
                let size = op.inputs[0].size;
                let sval1 = sign_extend(val1, size);
                let sval2 = sign_extend(val2, size);
                if sval2 == 0 {
                    tracing::warn!("INT_SDIV division by zero");
                    self.write_varnode_u64(output, 0)?;
                } else {
                    let res = sval1.wrapping_div(sval2);
                    self.write_varnode_u64(output, res as u64)?;
                }
            }
            PcodeOpcode::IntDiv => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_DIV must have output");
                if val2 == 0 {
                    tracing::warn!("INT_DIV division by zero");
                    self.write_varnode_u64(output, 0)?;
                } else {
                    let res = val1.wrapping_div(val2);
                    self.write_varnode_u64(output, res)?;
                }
            }
            PcodeOpcode::IntRem => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_REM must have output");
                if val2 == 0 {
                    tracing::warn!("INT_REM division by zero");
                    self.write_varnode_u64(output, 0)?;
                } else {
                    let res = val1.wrapping_rem(val2);
                    self.write_varnode_u64(output, res)?;
                }
            }
            PcodeOpcode::IntSRem => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_SREM must have output");
                let size = op.inputs[0].size;
                let sval1 = sign_extend(val1, size);
                let sval2 = sign_extend(val2, size);
                if sval2 == 0 {
                    tracing::warn!("INT_SREM division by zero");
                    self.write_varnode_u64(output, 0)?;
                } else {
                    let res = sval1.wrapping_rem(sval2);
                    self.write_varnode_u64(output, res as u64)?;
                }
            }
            PcodeOpcode::IntLess => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_LESS must have output");
                self.write_varnode_u64(output, if val1 < val2 { 1 } else { 0 })?;
            }
            PcodeOpcode::IntSLess => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let size = op.inputs[0].size;
                let sval1 = sign_extend(val1, size);
                let sval2 = sign_extend(val2, size);
                let output = op.output.as_ref().expect("INT_SLESS must have output");
                self.write_varnode_u64(output, if sval1 < sval2 { 1 } else { 0 })?;
            }
            PcodeOpcode::IntLessEqual => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_LESSEQUAL must have output");
                self.write_varnode_u64(output, if val1 <= val2 { 1 } else { 0 })?;
            }
            PcodeOpcode::IntSLessEqual => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let size = op.inputs[0].size;
                let sval1 = sign_extend(val1, size);
                let sval2 = sign_extend(val2, size);
                let output = op.output.as_ref().expect("INT_SLESSEQUAL must have output");
                self.write_varnode_u64(output, if sval1 <= sval2 { 1 } else { 0 })?;
            }
            PcodeOpcode::IntCarry => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_CARRY must have output");
                // Shift down to input size to correctly calculate carry
                let size = op.inputs[0].size;
                let mask = if size >= 8 { u64::MAX } else { (1u64 << (size * 8)) - 1 };
                let v1 = val1 & mask;
                let v2 = val2 & mask;
                let res = v1.checked_add(v2).is_none() || (v1 + v2) > mask;
                self.write_varnode_u64(output, if res { 1 } else { 0 })?;
            }
            PcodeOpcode::IntSBorrow => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_SBORROW must have output");
                let size = op.inputs[0].size;
                let sval1 = sign_extend(val1, size);
                let sval2 = sign_extend(val2, size);
                let res = sval1.checked_sub(sval2).is_none();
                self.write_varnode_u64(output, if res { 1 } else { 0 })?;
            }
            PcodeOpcode::IntSCarry => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("INT_SCARRY must have output");
                let size = op.inputs[0].size;
                let sval1 = sign_extend(val1, size);
                let sval2 = sign_extend(val2, size);
                let overflow = sval1.checked_add(sval2).is_none();
                self.write_varnode_u64(output, if overflow { 1 } else { 0 })?;
            }
            PcodeOpcode::PopCount => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let output = op.output.as_ref().expect("POPCOUNT must have output");
                let size = op.inputs[0].size;
                let mask = if size >= 8 { u64::MAX } else { (1u64 << (size * 8)) - 1 };
                let count = (val1 & mask).count_ones() as u64;
                self.write_varnode_u64(output, count)?;
            }
            PcodeOpcode::Int2Comp => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let output = op.output.as_ref().expect("INT_2COMP must have output");
                let res = (!val1).wrapping_add(1);
                self.write_varnode_u64(output, res)?;
            }
            PcodeOpcode::IntNegate => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let output = op.output.as_ref().expect("INT_NEGATE must have output");
                let res = !val1;
                self.write_varnode_u64(output, res)?;
            }
            PcodeOpcode::BoolNegate => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let output = op.output.as_ref().expect("BOOL_NEGATE must have output");
                self.write_varnode_u64(output, if val1 == 0 { 1 } else { 0 })?;
            }
            PcodeOpcode::BoolAnd => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("BOOL_AND must have output");
                self.write_varnode_u64(output, if val1 != 0 && val2 != 0 { 1 } else { 0 })?;
            }
            PcodeOpcode::BoolOr => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("BOOL_OR must have output");
                self.write_varnode_u64(output, if val1 != 0 || val2 != 0 { 1 } else { 0 })?;
            }
            PcodeOpcode::BoolXor => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let val2 = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("BOOL_XOR must have output");
                let b1 = val1 != 0;
                let b2 = val2 != 0;
                self.write_varnode_u64(output, if b1 ^ b2 { 1 } else { 0 })?;
            }
            PcodeOpcode::SubPiece => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let trunc_amount = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("SUBPIECE must have output");
                let res = val1 >> (trunc_amount * 8);
                self.write_varnode_u64(output, res)?;
            }
            PcodeOpcode::Piece => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?; // high part
                let val2 = self.read_varnode_u64(&op.inputs[1])?; // low part
                let output = op.output.as_ref().expect("PIECE must have output");
                let low_size = op.inputs[1].size;
                let res = (val1 << (low_size * 8)) | val2;
                self.write_varnode_u64(output, res)?;
            }
            PcodeOpcode::Cast => {
                let val1 = self.read_varnode_u64(&op.inputs[0])?;
                let output = op.output.as_ref().expect("CAST must have output");
                self.write_varnode_u64(output, val1)?;
            }
            PcodeOpcode::PtrAdd => {
                let ptr = self.read_varnode_u64(&op.inputs[0])?;
                let offset = self.read_varnode_u64(&op.inputs[1])?;
                let multiplier = self.read_varnode_u64(&op.inputs[2])?;
                let output = op.output.as_ref().expect("PTRADD must have output");
                let res = ptr.wrapping_add(offset.wrapping_mul(multiplier));
                self.write_varnode_u64(output, res)?;
            }
            PcodeOpcode::PtrSub => {
                let ptr = self.read_varnode_u64(&op.inputs[0])?;
                let offset = self.read_varnode_u64(&op.inputs[1])?;
                let output = op.output.as_ref().expect("PTRSUB must have output");
                let res = ptr.wrapping_add(offset);
                self.write_varnode_u64(output, res)?;
            }
            _ => {
                tracing::warn!("Unimplemented opcode: {:?}", op.opcode);
            }
        }
        Ok(StepResult::Next)
    }
}

fn sign_extend(val: u64, size: u32) -> i64 {
    let shift = 64 - (size * 8);
    ((val as i64) << shift) >> shift
}
