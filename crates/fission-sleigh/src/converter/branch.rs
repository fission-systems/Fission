use anyhow::Result;
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};
use sleigh_rs::execution::{BranchCall, CpuBranch, Execution, LocalGoto};
use sleigh_rs::Sleigh;

use super::IRConverter;

impl IRConverter {
    pub(super) fn convert_cpu_branch(
        &mut self,
        branch: &CpuBranch,
        current_address: u64,
        next_address: u64,
        next2_address: u64,
        sleigh: &Sleigh,
        execution: &Execution,
    ) -> Result<Vec<PcodeOp>> {
        let mut ops = Vec::new();
        let dst = self.lower_expr(
            &branch.dst,
            current_address,
            next_address,
            next2_address,
            sleigh,
            execution,
            &mut ops,
        )?;

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
            let cond_vn = self.lower_expr(
                cond,
                current_address,
                next_address,
                next2_address,
                sleigh,
                execution,
                &mut ops,
            )?;
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

    pub(super) fn convert_local_goto(
        &mut self,
        local_goto: &LocalGoto,
        current_address: u64,
        next_address: u64,
        next2_address: u64,
        sleigh: &Sleigh,
        execution: &Execution,
        target_seq: Option<u32>,
    ) -> Result<Vec<PcodeOp>> {
        let mut ops = Vec::new();

        let opcode = if local_goto.cond.is_some() {
            PcodeOpcode::CBranch
        } else {
            PcodeOpcode::Branch
        };

        let branch_seq = self.next_seq;
        let mut relative_delta = 0i64;
        if let Some(target_seq) = target_seq {
            relative_delta = i64::from(target_seq) - i64::from(branch_seq);
            if relative_delta == 0 {
                anyhow::bail!(
                    "Unsupported zero LocalGoto delta {} (target_seq={}, branch_seq={})",
                    relative_delta,
                    target_seq,
                    branch_seq
                );
            }
        }

        let mut inputs = vec![Varnode::constant(relative_delta, 1)];
        if let Some(cond) = &local_goto.cond {
            let cond_vn = self.lower_expr(
                cond,
                current_address,
                next_address,
                next2_address,
                sleigh,
                execution,
                &mut ops,
            )?;
            inputs.push(cond_vn);
        }

        let asm = if target_seq.is_some() {
            format!("LOCAL_GOTO <pcode+{relative_delta}>")
        } else {
            format!("LOCAL_GOTO block_{}", local_goto.dst.0)
        };

        ops.push(PcodeOp {
            seq_num: self.take_seq(),
            opcode,
            address: current_address,
            output: None,
            inputs,
            asm_mnemonic: Some(asm),
        });

        Ok(ops)
    }
}
