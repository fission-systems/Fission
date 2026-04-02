use anyhow::Result;
use fission_pcode::PcodeOp;
use sleigh_rs::execution::{Execution, Statement};
use sleigh_rs::Sleigh;

mod expr;
mod assignment;
mod branch;
mod memory;
mod unary;
mod helpers;

#[cfg(test)]
mod tests;

pub struct IRConverter {
    next_seq: u32,
}

impl IRConverter {
    pub fn new() -> Self {
        Self { next_seq: 0 }
    }

    /// Convert a semantic Sleigh statement into a Pcode operation stream.
    pub fn convert_statement(
        &mut self,
        stmt: &Statement,
        current_address: u64,
        sleigh: &Sleigh,
        execution: &Execution,
    ) -> Result<Vec<PcodeOp>> {
        match stmt {
            Statement::Delayslot(_) => Ok(Vec::new()),
            Statement::Export(_) => Ok(Vec::new()),
            Statement::CpuBranch(branch) => {
                self.convert_cpu_branch(branch, current_address, sleigh, execution)
            }
            Statement::Assignment(assign) => {
                self.convert_assignment(assign, current_address, sleigh, execution)
            }
            _ => anyhow::bail!("Unsupported statement variant in converter MVP: {:?}", stmt),
        }
    }
}
