use anyhow::Result;
use fission_pcode::PcodeOp;
use std::collections::HashMap;
use sleigh_rs::execution::{BlockId, Build, Execution, Statement};
use sleigh_rs::pattern::{BitConstraint, Pattern};
use sleigh_rs::Sleigh;

mod expr;
mod export;
mod assignment;
mod branch;
mod user_call;
mod memory;
mod unary;
mod helpers;

#[cfg(test)]
mod tests;

pub struct IRConverter {
    next_seq: u32,
    instruction_bytes: Vec<u8>,
    context_bits: Vec<u8>,
    table_export_values: HashMap<sleigh_rs::TableId, fission_pcode::Varnode>,
    build_stack: Vec<sleigh_rs::TableId>,
}

impl IRConverter {
    pub fn new() -> Self {
        Self {
            next_seq: 0,
            instruction_bytes: Vec::new(),
            context_bits: Vec::new(),
            table_export_values: HashMap::new(),
            build_stack: Vec::new(),
        }
    }

    pub fn new_with_decode_state(
        instruction_bytes: &[u8],
        context_bits: &[u8],
    ) -> Self {
        Self {
            next_seq: 0,
            instruction_bytes: instruction_bytes.to_vec(),
            context_bits: context_bits.to_vec(),
            table_export_values: HashMap::new(),
            build_stack: Vec::new(),
        }
    }

    /// Convert a semantic Sleigh statement into a Pcode operation stream.
    pub fn convert_statement(
        &mut self,
        stmt: &Statement,
        current_address: u64,
        next_address: u64,
        next2_address: u64,
        sleigh: &Sleigh,
        execution: &Execution,
    ) -> Result<Vec<PcodeOp>> {
        match stmt {
            Statement::Delayslot(_) => Ok(Vec::new()),
            Statement::Declare(_) => Ok(Vec::new()),
            Statement::Build(build) => self.convert_build(
                build,
                current_address,
                next_address,
                next2_address,
                sleigh,
            ),
            Statement::UserCall(call) => self.convert_user_call(
                call,
                current_address,
                next_address,
                next2_address,
                sleigh,
                execution,
            ),
            Statement::Export(export) => {
                self.convert_export(
                    export,
                    current_address,
                    next_address,
                    next2_address,
                    sleigh,
                    execution,
                )
            }
            Statement::CpuBranch(branch) => {
                self.convert_cpu_branch(
                    branch,
                    current_address,
                    next_address,
                    next2_address,
                    sleigh,
                    execution,
                )
            }
            Statement::Assignment(assign) => {
                self.convert_assignment(
                    assign,
                    current_address,
                    next_address,
                    next2_address,
                    sleigh,
                    execution,
                )
            }
            Statement::LocalGoto(_) => anyhow::bail!(
                "LocalGoto must be converted through convert_execution to resolve block targets"
            ),
        }
    }

    pub fn convert_execution(
        &mut self,
        execution: &Execution,
        current_address: u64,
        next_address: u64,
        next2_address: u64,
        sleigh: &Sleigh,
    ) -> Result<Vec<PcodeOp>> {
        struct PendingLocalGoto {
            op_index: usize,
            dst: BlockId,
        }

        let mut ops = Vec::new();
        let mut block_entry_seq = HashMap::new();
        let mut pending = Vec::new();

        for (idx, block) in execution.blocks().iter().enumerate() {
            let block_id = BlockId(idx);
            block_entry_seq.entry(block_id).or_insert(self.next_seq);

            for stmt in &block.statements {
                if let Statement::LocalGoto(local_goto) = stmt {
                    let known_target_seq = self
                        .resolve_local_goto_target_seq(execution, local_goto.dst, &block_entry_seq);
                    let mut converted = self.convert_local_goto(
                        local_goto,
                        current_address,
                        next_address,
                        next2_address,
                        sleigh,
                        execution,
                        known_target_seq,
                    )?;

                    if known_target_seq.is_none() {
                        let branch_local_idx = converted
                            .len()
                            .checked_sub(1)
                            .ok_or_else(|| anyhow::anyhow!("LocalGoto conversion emitted no branch op"))?;
                        pending.push(PendingLocalGoto {
                            op_index: ops.len() + branch_local_idx,
                            dst: local_goto.dst,
                        });
                    }

                    ops.append(&mut converted);
                    continue;
                }

                let mut converted = self.convert_statement(
                    stmt,
                    current_address,
                    next_address,
                    next2_address,
                    sleigh,
                    execution,
                )?;
                ops.append(&mut converted);
            }
        }

        for pending_fixup in pending {
            let target_seq = self
                .resolve_local_goto_target_seq(execution, pending_fixup.dst, &block_entry_seq)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Unable to resolve LocalGoto target block {:?}",
                        pending_fixup.dst
                    )
                })?;

            let op = ops.get_mut(pending_fixup.op_index).ok_or_else(|| {
                anyhow::anyhow!(
                    "LocalGoto patch index out of range: {}",
                    pending_fixup.op_index
                )
            })?;

            let delta = i64::from(target_seq) - i64::from(op.seq_num);
            if delta == 0 {
                anyhow::bail!(
                    "Unsupported zero LocalGoto delta {} (target_seq={}, branch_seq={})",
                    delta,
                    target_seq,
                    op.seq_num
                );
            }

            if op.inputs.is_empty() {
                anyhow::bail!("LocalGoto branch op has no inputs to patch");
            }
            op.inputs[0] = fission_pcode::Varnode::constant(delta, 1);
            op.asm_mnemonic = Some(format!("LOCAL_GOTO <pcode+{delta}>"));
        }

        Ok(ops)
    }

    fn resolve_local_goto_target_seq(
        &self,
        execution: &Execution,
        dst: BlockId,
        block_entry_seq: &HashMap<BlockId, u32>,
    ) -> Option<u32> {
        let mut current = dst;
        let mut visited = 0usize;

        while visited < execution.blocks().len() {
            let seq = *block_entry_seq.get(&current)?;
            let block = execution.block(current);
            if !block.statements.is_empty() {
                return Some(seq);
            }

            if let Some(next) = block.next {
                current = next;
                visited += 1;
                continue;
            }

            return Some(seq);
        }

        None
    }

    fn convert_build(
        &mut self,
        build: &Build,
        current_address: u64,
        next_address: u64,
        next2_address: u64,
        sleigh: &Sleigh,
    ) -> Result<Vec<PcodeOp>> {
        self.execute_build_table(
            build.table,
            current_address,
            next_address,
            next2_address,
            sleigh,
        )
    }

    fn execute_build_table(
        &mut self,
        table_id: sleigh_rs::TableId,
        current_address: u64,
        next_address: u64,
        next2_address: u64,
        sleigh: &Sleigh,
    ) -> Result<Vec<PcodeOp>> {
        if self.build_stack.contains(&table_id) {
            anyhow::bail!("Recursive Build detected for table {:?}", table_id);
        }

        self.build_stack.push(table_id);
        let result = self.execute_build_table_inner(
            table_id,
            current_address,
            next_address,
            next2_address,
            sleigh,
        );
        self.build_stack.pop();
        result
    }

    fn execute_build_table_inner(
        &mut self,
        table_id: sleigh_rs::TableId,
        current_address: u64,
        next_address: u64,
        next2_address: u64,
        sleigh: &Sleigh,
    ) -> Result<Vec<PcodeOp>> {
        let table = sleigh.table(table_id);
        let mut matched_constructor = None;

        for matcher in table.matcher_order() {
            let constructor = table.constructor(matcher.constructor);
            if !self.pattern_len_matches(&constructor.pattern) {
                continue;
            }

            let (context_constraints, token_constraints) =
                constructor.variant(matcher.variant_id);
            if !self.token_constraints_match(token_constraints) {
                continue;
            }
            if !self.context_constraints_match(context_constraints) {
                continue;
            }

            matched_constructor = Some(constructor);
            break;
        }

        let constructor = matched_constructor.ok_or_else(|| {
            anyhow::anyhow!("No matching Build constructor for table {:?}", table_id)
        })?;

        let mut ops = Vec::new();
        if let Some(exec) = &constructor.execution {
            let mut converted = self.convert_execution(
                exec,
                current_address,
                next_address,
                next2_address,
                sleigh,
            )?;
            ops.append(&mut converted);
        } else if table.export.is_some() {
            let _ = self.ensure_table_export_slot(table_id, current_address, sleigh, &mut ops)?;
        }

        Ok(ops)
    }

    fn pattern_len_matches(&self, pattern: &Pattern) -> bool {
        let available_bytes = self.instruction_bytes.len() as u64;
        available_bytes >= pattern.len.min()
    }

    fn token_constraints_match(&self, token_constraints: &[BitConstraint]) -> bool {
        for (bit_index, constraint) in token_constraints.iter().enumerate() {
            match constraint {
                BitConstraint::Unrestrained => {}
                BitConstraint::Defined(expected) => {
                    let Some(actual) = self.instruction_bit(bit_index) else {
                        return false;
                    };
                    if actual != *expected {
                        return false;
                    }
                }
                BitConstraint::Restrained => return false,
            }
        }
        true
    }

    fn context_constraints_match(&self, context_constraints: &[BitConstraint]) -> bool {
        for (bit_index, constraint) in context_constraints.iter().enumerate() {
            match constraint {
                BitConstraint::Unrestrained => {}
                BitConstraint::Defined(expected) => {
                    let actual = self.context_bit(bit_index).unwrap_or(false);
                    if actual != *expected {
                        return false;
                    }
                }
                BitConstraint::Restrained => return false,
            }
        }
        true
    }

    fn instruction_bit(&self, bit_index: usize) -> Option<bool> {
        let byte_index = bit_index / 8;
        let bit_in_byte = bit_index % 8;
        let byte = *self.instruction_bytes.get(byte_index)?;
        Some(((byte >> bit_in_byte) & 1) != 0)
    }

    fn context_bit(&self, bit_index: usize) -> Option<bool> {
        let byte_index = bit_index / 8;
        let bit_in_byte = bit_index % 8;
        let byte = *self.context_bits.get(byte_index)?;
        Some(((byte >> bit_in_byte) & 1) != 0)
    }
}
