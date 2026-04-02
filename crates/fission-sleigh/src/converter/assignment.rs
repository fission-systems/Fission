use anyhow::{Context, Result};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};
use sleigh_rs::execution::{
    Assignment, AssignmentOp, AssignmentWrite, AssignmentWriteVariable, Execution,
};
use sleigh_rs::Sleigh;

use super::IRConverter;

impl IRConverter {
    pub(super) fn convert_assignment(
        &mut self,
        assign: &Assignment,
        current_address: u64,
        sleigh: &Sleigh,
        execution: &Execution,
    ) -> Result<Vec<PcodeOp>> {
        let mut ops = Vec::new();
        let rhs =
            self.lower_expr(&assign.right, current_address, sleigh, execution, &mut ops)?;

        match &assign.var {
            AssignmentWrite::Variable { value, op } => {
                let rhs = match op {
                    Some(op) => {
                        self.apply_assignment_op(op, rhs, current_address, &mut ops)?
                    }
                    None => rhs,
                };
                let out = self.lower_assignment_target(value, sleigh, execution)?;
                ops.push(PcodeOp {
                    seq_num: self.take_seq(),
                    opcode: PcodeOpcode::Copy,
                    address: current_address,
                    output: Some(out),
                    inputs: vec![rhs],
                    asm_mnemonic: Some("ASSIGN".to_string()),
                });
                Ok(ops)
            }
            AssignmentWrite::Memory { mem, addr } => {
                let addr_vn =
                    self.lower_expr(addr, current_address, sleigh, execution, &mut ops)?;
                self.emit_store(mem, addr_vn, rhs, current_address, &mut ops);
                Ok(ops)
            }
            AssignmentWrite::TableExport { .. } => {
                anyhow::bail!("Unsupported table export assignment in converter MVP")
            }
        }
    }

    pub(super) fn lower_assignment_target(
        &self,
        target: &AssignmentWriteVariable,
        sleigh: &Sleigh,
        execution: &Execution,
    ) -> Result<Varnode> {
        match target {
            AssignmentWriteVariable::Varnode(id) => self.varnode_from_sleigh(sleigh, *id),
            AssignmentWriteVariable::Variable(id) => {
                let bits = execution.variable(*id).len_bits.get();
                let size = Self::bits_to_bytes(bits)
                    .context("Invalid assignment variable size")?;
                Ok(self.execution_varnode(*id, size))
            }
            _ => anyhow::bail!(
                "Unsupported assignment target in converter MVP: {:?}",
                target
            ),
        }
    }

    pub(super) fn apply_assignment_op(
        &mut self,
        op: &AssignmentOp,
        rhs: Varnode,
        current_address: u64,
        emitted: &mut Vec<PcodeOp>,
    ) -> Result<Varnode> {
        match op {
            AssignmentOp::TakeLsb(len) => {
                let out_size = u32::try_from(len.get())
                    .context("Assignment TakeLsb size does not fit u32")?;
                let out = self.make_temp_varnode(self.next_seq, out_size);
                emitted.push(PcodeOp {
                    seq_num: self.take_seq(),
                    opcode: PcodeOpcode::SubPiece,
                    address: current_address,
                    output: Some(out.clone()),
                    inputs: vec![rhs, Varnode::constant(0, 4)],
                    asm_mnemonic: Some("ASSIGN_TAKELSB".to_string()),
                });
                Ok(out)
            }
            AssignmentOp::TrunkLsb(trunk) => {
                let trunk_bytes = u32::try_from(*trunk)
                    .context("Assignment TrunkLsb does not fit u32")?;
                let out_size = rhs.size.saturating_sub(trunk_bytes).max(1);
                let out = self.make_temp_varnode(self.next_seq, out_size);
                emitted.push(PcodeOp {
                    seq_num: self.take_seq(),
                    opcode: PcodeOpcode::SubPiece,
                    address: current_address,
                    output: Some(out.clone()),
                    inputs: vec![rhs, Varnode::constant((*trunk) as i64, 4)],
                    asm_mnemonic: Some("ASSIGN_TRUNKLSB".to_string()),
                });
                Ok(out)
            }
            AssignmentOp::BitRange(range) => {
                let bits = range.end.saturating_sub(range.start);
                if bits == 0 {
                    anyhow::bail!("Assignment BitRange cannot have zero width");
                }
                self.extract_bit_range(
                    rhs,
                    range.clone(),
                    bits,
                    current_address,
                    emitted,
                )
            }
        }
    }
}
