use super::*;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::nir::builder) fn is_loop_carried_register_update_candidate(output: &Varnode) -> bool {
        !output.is_constant && is_register_space_id(output.space_id) && output.size >= 4
    }

    pub(in crate::nir::builder) fn output_is_loop_carried_register_update(
        &self,
        block_idx: usize,
        op_idx: usize,
        op: &PcodeOp,
        output: &Varnode,
    ) -> bool {
        let output_key = VarnodeKey::from(output);
        self.loop_bodies.iter().any(|loop_body| {
            loop_body.body.contains(&block_idx)
                && self.loop_update_reaches_backedge_tail(block_idx, loop_body)
                && (Self::op_reads_varnode_key(op, &output_key)
                    || self.loop_reads_varnode_before_update(
                        loop_body,
                        block_idx,
                        op_idx,
                        &output_key,
                    ))
        })
    }

    pub(in crate::nir::builder) fn loop_update_reaches_backedge_tail(
        &self,
        block_idx: usize,
        loop_body: &crate::nir::structuring::loop_analysis::LoopBody,
    ) -> bool {
        loop_body.tails.iter().any(|tail| {
            *tail == block_idx || self.block_can_reach(block_idx, *tail, loop_body.head)
        })
    }

    pub(in crate::nir::builder) fn loop_reads_varnode_before_update(
        &self,
        loop_body: &crate::nir::structuring::loop_analysis::LoopBody,
        block_idx: usize,
        op_idx: usize,
        output_key: &VarnodeKey,
    ) -> bool {
        let Some(block) = self.pcode.blocks.get(block_idx) else {
            return false;
        };
        if Self::block_reads_varnode_before_redefinition(block, op_idx, output_key) {
            return true;
        }

        if loop_body.head == block_idx {
            return false;
        }
        self.pcode.blocks.get(loop_body.head).is_some_and(|head| {
            Self::block_reads_varnode_before_redefinition(head, head.ops.len(), output_key)
        })
    }

    pub(in crate::nir::builder) fn block_reads_varnode_before_redefinition(
        block: &crate::pcode::PcodeBasicBlock,
        limit: usize,
        output_key: &VarnodeKey,
    ) -> bool {
        for candidate in block.ops.iter().take(limit) {
            if Self::op_reads_varnode_key(candidate, output_key) {
                return true;
            }
            if candidate.output.as_ref().is_some_and(|output| {
                Self::varnode_key_may_alias_output(&VarnodeKey::from(output), output_key)
            }) {
                return false;
            }
        }
        false
    }

    pub(in crate::nir::builder) fn op_reads_varnode_key(op: &PcodeOp, output_key: &VarnodeKey) -> bool {
        op.inputs
            .iter()
            .any(|input| Self::varnode_key_may_alias_output(&VarnodeKey::from(input), output_key))
    }
}
