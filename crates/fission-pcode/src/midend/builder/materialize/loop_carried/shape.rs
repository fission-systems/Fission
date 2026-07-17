use super::*;
use std::collections::{HashSet, VecDeque};

/// Evidence that one exact register definition, rather than merely the
/// register name, carries a value across a loop backedge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::midend::builder) struct LoopCarriedDefinitionProof {
    loop_head: usize,
    definition_block: usize,
    definition_op: usize,
}

impl LoopCarriedDefinitionProof {
    pub(super) fn loop_head(self) -> usize {
        self.loop_head
    }

    pub(super) fn definition_site(self) -> (usize, usize) {
        (self.definition_block, self.definition_op)
    }
}

impl<'a> PreviewBuilder<'a> {
    pub(in crate::midend::builder) fn is_loop_carried_register_update_candidate(
        output: &Varnode,
    ) -> bool {
        // Full-width GPRs (size >= 4) are always eligible. Narrow lanes (AL/AX)
        // are eligible too, but only when the carried proof still holds — the
        // gate itself no longer rejects them. Callers that need a wider identity
        // (e.g. primary-return zero seed + `movzx`) resolve that in binding
        // selection, not by dropping the candidate entirely.
        // Exact carried proof still requires self-read + kill-free backedge reach.
        !output.is_constant && is_register_space_id(output.space_id) && output.size >= 1
    }

    pub(in crate::midend::builder) fn prove_loop_carried_register_update(
        &self,
        block_idx: usize,
        op_idx: usize,
        output: &Varnode,
    ) -> Option<LoopCarriedDefinitionProof> {
        let output_key = VarnodeKey::from(output);
        self.loop_bodies
            .iter()
            .filter(|loop_body| loop_body.body.contains(&block_idx))
            .filter(|loop_body| {
                self.loop_entry_value_reaches_definition(loop_body, block_idx, op_idx, &output_key)
                    && self.definition_reaches_loop_backedge(
                        loop_body,
                        block_idx,
                        op_idx,
                        &output_key,
                    )
            })
            .min_by_key(|loop_body| loop_body.body.len())
            .map(|loop_body| LoopCarriedDefinitionProof {
                loop_head: loop_body.head,
                definition_block: block_idx,
                definition_op: op_idx,
            })
    }

    fn definition_reaches_loop_backedge(
        &self,
        loop_body: &crate::midend::structuring::loop_analysis::LoopBody,
        definition_block_idx: usize,
        definition_op_idx: usize,
        output_key: &VarnodeKey,
    ) -> bool {
        let Some(definition_block) = self.pcode.blocks.get(definition_block_idx) else {
            return false;
        };
        if definition_block
            .ops
            .iter()
            .skip(definition_op_idx + 1)
            .any(|op| Self::op_kills_varnode_definition(op, output_key))
        {
            return false;
        }

        let mut queue = VecDeque::new();
        queue.extend(
            self.successors
                .get(definition_block_idx)
                .into_iter()
                .flatten()
                .copied(),
        );
        let mut visited = HashSet::new();
        while let Some(block_idx) = queue.pop_front() {
            if block_idx == loop_body.head {
                return true;
            }
            if !loop_body.body.contains(&block_idx) || !visited.insert(block_idx) {
                continue;
            }
            let Some(block) = self.pcode.blocks.get(block_idx) else {
                continue;
            };
            if block
                .ops
                .iter()
                .any(|op| Self::op_kills_varnode_definition(op, output_key))
            {
                continue;
            }
            queue.extend(
                self.successors
                    .get(block_idx)
                    .into_iter()
                    .flatten()
                    .copied(),
            );
        }
        false
    }

    fn loop_entry_value_reaches_definition(
        &self,
        loop_body: &crate::midend::structuring::loop_analysis::LoopBody,
        definition_block_idx: usize,
        definition_op_idx: usize,
        output_key: &VarnodeKey,
    ) -> bool {
        let mut queue = VecDeque::from([(loop_body.head, false)]);
        let mut visited = HashSet::new();
        while let Some((block_idx, mut observed_read)) = queue.pop_front() {
            if !loop_body.body.contains(&block_idx) || !visited.insert((block_idx, observed_read)) {
                continue;
            }
            let Some(block) = self.pcode.blocks.get(block_idx) else {
                continue;
            };
            let mut killed = false;
            for (op_idx, op) in block.ops.iter().enumerate() {
                if block_idx == definition_block_idx && op_idx == definition_op_idx {
                    return observed_read || Self::op_reads_varnode_key(op, output_key);
                }
                observed_read |= Self::op_reads_varnode_key(op, output_key);
                if Self::op_kills_varnode_definition(op, output_key) {
                    killed = true;
                    break;
                }
            }
            if killed {
                continue;
            }
            queue.extend(
                self.successors
                    .get(block_idx)
                    .into_iter()
                    .flatten()
                    .copied()
                    .filter(|successor| *successor != loop_body.head)
                    .map(|successor| (successor, observed_read)),
            );
        }
        false
    }

    pub(in crate::midend::builder) fn loop_update_reaches_backedge_tail(
        &self,
        block_idx: usize,
        loop_body: &crate::midend::structuring::loop_analysis::LoopBody,
    ) -> bool {
        loop_body.tails.iter().any(|tail| {
            *tail == block_idx || self.block_can_reach(block_idx, *tail, loop_body.head)
        })
    }

    pub(in crate::midend::builder) fn loop_reads_varnode_before_update(
        &self,
        loop_body: &crate::midend::structuring::loop_analysis::LoopBody,
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

    pub(in crate::midend::builder) fn block_reads_varnode_before_redefinition(
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

    pub(in crate::midend::builder) fn op_reads_varnode_key(
        op: &PcodeOp,
        output_key: &VarnodeKey,
    ) -> bool {
        op.inputs
            .iter()
            .any(|input| Self::varnode_key_may_alias_output(&VarnodeKey::from(input), output_key))
    }
}
