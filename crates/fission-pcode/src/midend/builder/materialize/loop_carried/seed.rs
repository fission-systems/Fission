use super::*;
use std::collections::BTreeSet;

impl<'a> PreviewBuilder<'a> {
    pub(in crate::midend::builder) fn seed_loop_carried_binding_initializer_from_edge_zero(
        &mut self,
        block: &crate::pcode::PcodeBasicBlock,
        output: &Varnode,
        name: &str,
    ) {
        if self
            .temps
            .get(name)
            .is_none_or(|binding| binding.initializer.is_some())
        {
            return;
        }
        let Some(block_idx) = self.address_to_index.get(&block.start_address).copied() else {
            return;
        };
        let Some(merge_idx) = self.loop_carried_initializer_merge_predecessor(block_idx) else {
            return;
        };
        let Some(pred_idxs) = self.predecessors.get(merge_idx).cloned() else {
            return;
        };
        if pred_idxs.len() < 2 {
            return;
        }

        let mut saw_zero_edge = false;
        let mut saw_named_incoming = false;
        for pred_idx in pred_idxs {
            let mut visited = BTreeSet::new();
            let Some((zero, named)) = self.predecessor_incoming_zero_or_named_binding(
                pred_idx,
                merge_idx,
                output,
                name,
                &mut visited,
            ) else {
                return;
            };
            saw_zero_edge |= zero;
            saw_named_incoming |= named;
        }
        if saw_zero_edge
            && saw_named_incoming
            && let Some(binding) = self.temps.get_mut(name)
        {
            binding.initializer = Some(DirExpr::Const(0, type_from_size(output.size, false)));
        }
    }

    fn loop_carried_initializer_merge_predecessor(&self, block_idx: usize) -> Option<usize> {
        let pred_idxs = self.predecessors.get(block_idx)?;
        if pred_idxs.len() >= 2 {
            return Some(block_idx);
        }
        let pred_idx = *pred_idxs.first()?;
        self.predecessors
            .get(pred_idx)
            .is_some_and(|preds| preds.len() >= 2)
            .then_some(pred_idx)
    }

    fn predecessor_incoming_zero_or_named_binding(
        &self,
        pred_idx: usize,
        succ_idx: usize,
        output: &Varnode,
        name: &str,
        visited: &mut BTreeSet<(usize, usize)>,
    ) -> Option<(bool, bool)> {
        if !visited.insert((pred_idx, succ_idx)) {
            return None;
        }
        if self.predecessor_edge_forces_register_zero(pred_idx, succ_idx, output) {
            return Some((true, false));
        }
        let Some(pred_block) = self.pcode.blocks.get(pred_idx) else {
            return None;
        };
        let term_idx = self
            .block_terminator_index(pred_block)
            .unwrap_or(pred_block.ops.len());
        if let Some((_, pred_op)) =
            self.last_register_redefinition_before(pred_block, term_idx, output)
        {
            return pred_op
                .output
                .as_ref()
                .is_some_and(|pred_output| {
                    self.varnode_aliases_value(pred_output, output)
                        && self
                            .materialized_vns
                            .get(&MaterializedVarnodeKey::new(pred_output, pred_op))
                            .is_some_and(|candidate| candidate == name)
                })
                .then_some((false, true));
        }
        let upstream_preds = self.predecessors.get(pred_idx)?;
        if upstream_preds.is_empty() {
            return None;
        }
        let mut saw_zero_edge = false;
        let mut saw_named_incoming = false;
        for upstream_idx in upstream_preds {
            let (zero, named) = self.predecessor_incoming_zero_or_named_binding(
                *upstream_idx,
                pred_idx,
                output,
                name,
                visited,
            )?;
            saw_zero_edge |= zero;
            saw_named_incoming |= named;
        }
        Some((saw_zero_edge, saw_named_incoming))
    }
}
