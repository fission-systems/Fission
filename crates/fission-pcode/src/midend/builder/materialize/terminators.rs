use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn output_used_only_as_stack_return_target(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        op_idx: usize,
        terminator_index: Option<usize>,
        op: &PcodeOp,
        output: &Varnode,
    ) -> bool {
        if op.opcode != PcodeOpcode::Load || op.inputs.len() < 2 {
            return false;
        }
        if !self
            .stack_pointer_register_name(&op.inputs[1])
            .is_some_and(|name| matches!(name.as_str(), "rsp" | "esp" | "sp"))
        {
            return false;
        }
        let Some(term_idx) = terminator_index else {
            return false;
        };
        let Some(term) = block.ops.get(term_idx) else {
            return false;
        };
        term.opcode == PcodeOpcode::Return
            && term.inputs.last().is_some_and(|input| input == output)
            && self
                .output_use_sites_in_block(block, op_idx, output)
                .into_iter()
                .all(|(use_idx, _)| use_idx == term_idx)
    }

    pub(super) fn output_is_stack_pointer_register(&self, output: &Varnode) -> bool {
        self.stack_pointer_register_name(output)
            .is_some_and(|name| matches!(name.as_str(), "rsp" | "esp" | "sp"))
    }

    /// True when `op` defines a condition flag from stack-pointer arithmetic only
    /// (prologue/epilogue `sub/add rsp` flag noise).
    pub(super) fn flag_def_is_stack_pointer_only(&self, op: &PcodeOp, output: &Varnode) -> bool {
        if !is_register_space_id(output.space_id) {
            return false;
        }
        // x86 flag-bank offsets used by SLEIGH (CF/PF/AF/ZF/SF/OF-class).
        let is_flag = matches!(output.offset, 0x200 | 0x201 | 0x202 | 0x206 | 0x207 | 0x20b)
            && output.size <= 1;
        if !is_flag {
            return false;
        }
        let mut saw_stack = false;
        for input in &op.inputs {
            if input.is_constant {
                continue;
            }
            if self.output_is_stack_pointer_register(input) {
                saw_stack = true;
                continue;
            }
            // Any non-const, non-stack input means a real data predicate.
            return false;
        }
        saw_stack
    }

    pub(super) fn is_predicate_passthrough_to_terminator(op: &PcodeOp) -> bool {
        matches!(
            op.opcode,
            PcodeOpcode::BoolNegate
                | PcodeOpcode::BoolAnd
                | PcodeOpcode::BoolOr
                | PcodeOpcode::BoolXor
                | PcodeOpcode::IntEqual
                | PcodeOpcode::IntNotEqual
                | PcodeOpcode::IntLess
                | PcodeOpcode::IntLessEqual
                | PcodeOpcode::IntSLess
                | PcodeOpcode::IntSLessEqual
        )
    }

    pub(in crate::midend::builder) fn block_terminator_index(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
    ) -> Option<usize> {
        block.ops.iter().rposition(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Branch
                    | PcodeOpcode::CBranch
                    | PcodeOpcode::BranchInd
                    | PcodeOpcode::Return
            )
        })
    }

    /// Terminator index for statement materialization.
    ///
    /// Same-block-forward CBranch tails (cmov body after a branch to the next
    /// machine instruction / next BB start) stay in the op stream so
    /// `lower_block_ops_range` can emit `if (!cond) { body }`. Other consumers
    /// of [`Self::block_terminator_index`] keep the raw control-flow op.
    pub(super) fn materialize_block_terminator_index(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
    ) -> Option<usize> {
        let mut idx = self.block_terminator_index(block)?;
        loop {
            let op = block.ops.get(idx)?;
            let is_tail_cmov = op.opcode == PcodeOpcode::CBranch
                && op.inputs.len() >= 2
                && crate::midend::cfg::same_block_forward_branch_target_op_idx(
                    block,
                    idx,
                    block.ops.len(),
                    op,
                    &op.inputs[0],
                )
                .is_some_and(|target| target > idx + 1);
            if !is_tail_cmov {
                return Some(idx);
            }
            // Walk earlier for a real CFG terminator (branch/return).
            idx = block.ops[..idx].iter().rposition(|candidate| {
                matches!(
                    candidate.opcode,
                    PcodeOpcode::Branch
                        | PcodeOpcode::CBranch
                        | PcodeOpcode::BranchInd
                        | PcodeOpcode::Return
                )
            })?;
        }
    }
}
