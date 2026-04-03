use anyhow::{Context, Result};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};
use sleigh_rs::execution::{Execution, UserCall};
use sleigh_rs::Sleigh;

use super::IRConverter;

impl IRConverter {
    pub(super) fn convert_user_call(
        &mut self,
        user_call: &UserCall,
        current_address: u64,
        next_address: u64,
        next2_address: u64,
        sleigh: &Sleigh,
        execution: &Execution,
    ) -> Result<Vec<PcodeOp>> {
        let mut ops = Vec::new();

        let userop_id = i64::try_from(user_call.function.0)
            .context("User function id does not fit i64")?;
        let mut inputs = Vec::with_capacity(user_call.params.len() + 1);
        inputs.push(Varnode::constant(userop_id, 4));

        for param in &user_call.params {
            let lowered = self.lower_expr(
                param,
                current_address,
                next_address,
                next2_address,
                sleigh,
                execution,
                &mut ops,
            )?;
            inputs.push(lowered);
        }

        let userop_name = sleigh.user_function(user_call.function).name();
        ops.push(PcodeOp {
            seq_num: self.take_seq(),
            opcode: PcodeOpcode::CallOther,
            address: current_address,
            output: None,
            inputs,
            asm_mnemonic: Some(format!("USERCALL_{userop_name}")),
        });

        Ok(ops)
    }
}
