use anyhow::Result;
use fission_pcode::{PcodeOp, PcodeOpcode};
use sleigh_rs::execution::{BranchCall, CpuBranch, Execution};
use sleigh_rs::Sleigh;

use super::IRConverter;

impl IRConverter {
    pub(super) fn convert_cpu_branch(
        &mut self,
        branch: &CpuBranch,
        current_address: u64,
        sleigh: &Sleigh,
        execution: &Execution,
    ) -> Result<Vec<PcodeOp>> {
        let mut ops = Vec::new();
        let dst = self.lower_expr(&branch.dst, current_address, sleigh, execution, &mut ops)?;

        let opcode = if branch.cond.is_some() {
            PcodeOpcode::CBranch
        } else {
            match branch.call {
                BranchCall::Goto => PcodeOpcode::Branch,
                BranchCall::Call => PcodeOpcode::Call,
                BranchCall::Return => PcodeOpcode::Return,
            }
        };

        let mut inputs = vec![dst];
        if let Some(cond) = &branch.cond {
            let cond_vn =
                self.lower_expr(cond, current_address, sleigh, execution, &mut ops)?;
            inputs.push(cond_vn);
        }

        ops.push(PcodeOp {
            seq_num: self.take_seq(),
            opcode,
            address: current_address,
            output: None,
            inputs,
            asm_mnemonic: Some("BRANCH".to_string()),
        });

        Ok(ops)
    }
}
