use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn op_defines_x86_gpr_family(&self, op: &PcodeOp, family_idx: usize) -> bool {
        op.output
            .as_ref()
            .and_then(|output| self.canonical_x86_gpr64_name_for_value(output))
            .is_some_and(|(_, output_family)| output_family == family_idx)
    }

    pub(super) fn single_successor_index(&self, block_idx: usize) -> Option<usize> {
        let successors = self.successors.get(block_idx)?;
        if successors.len() == 1 {
            Some(successors[0])
        } else {
            None
        }
    }

    pub(super) fn block_reads_merge_input_before_redefinition(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        output: &Varnode,
    ) -> bool {
        for op in &block.ops {
            if op
                .inputs
                .iter()
                .any(|input| self.varnode_aliases_value(input, output))
            {
                return true;
            }
            if op
                .output
                .as_ref()
                .is_some_and(|candidate| self.varnode_aliases_value(candidate, output))
            {
                return false;
            }
        }
        false
    }

    pub(super) fn block_returns_without_redefining_output(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        output: &Varnode,
    ) -> bool {
        let Some(term_idx) = self.block_terminator_index(block) else {
            return false;
        };
        if block.ops[term_idx].opcode != PcodeOpcode::Return {
            return false;
        }
        !block.ops.iter().take(term_idx).any(|op| {
            op.output
                .as_ref()
                .is_some_and(|candidate| self.varnode_aliases_value(candidate, output))
        })
    }

    pub(super) fn last_redefinition_index_before_terminator(
        &self,
        block: &crate::pcode::PcodeBasicBlock,
        output: &Varnode,
    ) -> Option<usize> {
        block.ops.iter().enumerate().rev().find_map(|(idx, op)| {
            op.output
                .as_ref()
                .is_some_and(|candidate| self.varnode_aliases_value(candidate, output))
                .then_some(idx)
        })
    }

    pub(super) fn output_def_is_safe_direct_successor_merge(op: &PcodeOp) -> bool {
        matches!(
            op.opcode,
            PcodeOpcode::Copy
                | PcodeOpcode::Load
                | PcodeOpcode::SubPiece
                | PcodeOpcode::IntZExt
                | PcodeOpcode::Cast
                | PcodeOpcode::IntAdd
                | PcodeOpcode::IntSub
                | PcodeOpcode::IntMult
                | PcodeOpcode::IntAnd
                | PcodeOpcode::IntOr
                | PcodeOpcode::IntXor
                | PcodeOpcode::IntNegate
                | PcodeOpcode::IntLeft
                | PcodeOpcode::IntRight
                | PcodeOpcode::IntSRight
        )
    }

    pub(super) fn has_side_effect_between_ops(
        block: &crate::pcode::PcodeBasicBlock,
        start: usize,
        end: usize,
    ) -> bool {
        block.ops[start..end.min(block.ops.len())].iter().any(|op| {
            matches!(
                op.opcode,
                PcodeOpcode::Store
                    | PcodeOpcode::Call
                    | PcodeOpcode::CallInd
                    | PcodeOpcode::CallOther
            )
        })
    }
}
