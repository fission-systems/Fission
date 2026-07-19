use super::*;
use crate::midend::structuring::SccAnalysis;

impl<'a> PreviewBuilder<'a> {
    pub(super) fn internalized_guard_family_nested_before_refs_for_join_owner(
        body: &[HirStmt],
        if_idx: usize,
        label: &str,
        candidate_cond: &HirExpr,
    ) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::internalized_guard_family_nested_before_refs_for_join_owner(body, if_idx, label, candidate_cond)
    }

    pub(super) fn surviving_label_refs_after_guarded_tail_promotion(
        body: &[HirStmt],
        middle: &[HirStmt],
        if_idx: usize,
        label_idx: usize,
        label: &str,
    ) -> (usize, usize) {
        fission_midend_structuring::guarded_tail::pure_hir::surviving_label_refs_after_guarded_tail_promotion(body, middle, if_idx, label_idx, label)
    }

    pub(super) fn trailing_middle_fallthrough_equivalent_refs(
        middle: &[HirStmt],
        label: &str,
    ) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::trailing_middle_fallthrough_equivalent_refs(middle, label)
    }

    /// True when the middle segment is only join glue: empty blocks, labels, and `Goto(label)`.
    /// Such segments impose no semantic work beyond reaching the join label; all `Goto` refs are
    /// fallthrough-equivalent for promotion bookkeeping (matches Ghidra-style join chains).
    pub(super) fn middle_is_join_label_only_glue(middle: &[HirStmt], label: &str) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::middle_is_join_label_only_glue(middle, label)
    }

    /// Subtract trailing duplicate `Goto(label)` hops, or zero when the whole middle is join glue.
    pub(super) fn effective_middle_refs_for_promotion(
        middle: &[HirStmt],
        label: &str,
        middle_refs: usize,
    ) -> usize {
        fission_midend_structuring::guarded_tail::pure_hir::effective_middle_refs_for_promotion(middle, label, middle_refs)
    }

    pub(super) fn outside_refs_preserve_forward_owner(
        body: &[HirStmt],
        if_idx: usize,
        label_idx: usize,
        label: &str,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::outside_refs_preserve_forward_owner(body, if_idx, label_idx, label)
    }

    pub(super) fn outside_refs_are_elidable_next_flow(
        body: &[HirStmt],
        if_idx: usize,
        label_idx: usize,
        label: &str,
    ) -> bool {
        fission_midend_structuring::guarded_tail::pure_hir::outside_refs_are_elidable_next_flow(body, if_idx, label_idx, label)
    }

    pub(super) fn find_top_level_label_after(
        body: &[HirStmt],
        start_idx: usize,
        label: &str,
    ) -> Option<usize> {
        fission_midend_structuring::guarded_tail::pure_hir::find_top_level_label_after(body, start_idx, label)
    }

    pub(super) fn is_nontrivial_internal_target_entry(&self, idx: usize) -> bool {
        let preds = &self.predecessors[idx];
        if preds.len() != 1 {
            return true;
        }
        preds[0] + 1 != idx
    }

    pub(crate) fn region_has_external_entry(
        &self,
        region: &HashSet<usize>,
        header_idx: usize,
    ) -> bool {
        region.iter().copied().any(|idx| {
            idx != header_idx
                && self.predecessors[idx]
                    .iter()
                    .any(|pred| !region.contains(pred))
        })
    }

    pub(super) fn region_has_targeted_internal_entry(
        &self,
        start_idx: usize,
        skip_to: usize,
        targeted: &HashSet<u64>,
    ) -> bool {
        if skip_to <= start_idx + 1 {
            return false;
        }
        (start_idx + 1..skip_to).any(|idx| {
            targeted.contains(&self.block_target_key(idx))
                && self.is_nontrivial_internal_target_entry(idx)
        })
    }

    pub(super) fn targeted_internal_entries(
        &self,
        start_idx: usize,
        skip_to: usize,
        targeted: &HashSet<u64>,
    ) -> Vec<usize> {
        if skip_to <= start_idx + 1 {
            return Vec::new();
        }
        (start_idx + 1..skip_to)
            .filter(|idx| {
                targeted.contains(&self.block_target_key(*idx))
                    && self.is_nontrivial_internal_target_entry(*idx)
            })
            .collect()
    }

    pub(super) fn region_external_exit_nodes(&self, region: &HashSet<usize>) -> Vec<usize> {
        let mut exits: Vec<usize> = region
            .iter()
            .copied()
            .filter(|idx| {
                self.successors[*idx]
                    .iter()
                    .any(|succ| !region.contains(succ))
            })
            .collect();
        // `region` is a HashSet: its iteration order is randomized per-process.
        // Callers (e.g. guarded-tail promotion gating) pick `.first()` off this
        // list, so an unsorted result makes structuring/materialize order —
        // and therefore synthetic temp naming — nondeterministic across runs
        // on binaries with more than one valid exit node. Sort for a stable,
        // reproducible choice.
        exits.sort_unstable();
        exits
    }

    pub(super) fn ensure_graph_invariant_promotion_region(
        &self,
        start_idx: usize,
        internal_entries: &[usize],
        region: &HashSet<usize>,
    ) -> Result<(), PromotionGateRejection> {
        let scc = SccAnalysis::analyze(&self.successors, &self.predecessors);
        if region
            .iter()
            .copied()
            .any(|idx| scc.is_irreducible_node(idx))
        {
            return Err(PromotionGateRejection::NotSinglePredSucc);
        }

        let dom = self.analyze_cfg_dominators();
        if !internal_entries
            .iter()
            .copied()
            .all(|idx| dom.dominates(start_idx, idx))
        {
            return Err(PromotionGateRejection::NotSinglePredSucc);
        }

        if let Some(exit_idx) = self.region_external_exit_nodes(region).first().copied() {
            let Some(postdom) =
                PostDomTree::analyze_window_with_exit(&self.successors, region, exit_idx)
            else {
                return Err(PromotionGateRejection::NotSinglePredSucc);
            };
            let start_postdom = postdom
                .postdominators()
                .get(&start_idx)
                .is_some_and(|set| set.contains(&exit_idx));
            if !start_postdom {
                return Err(PromotionGateRejection::NotSinglePredSucc);
            }
        }

        Ok(())
    }

    pub(super) fn is_minimal_structured_promotion_candidate(
        &self,
        start_idx: usize,
        skip_to: usize,
        targeted: &HashSet<u64>,
    ) -> Result<(), PromotionGateRejection> {
        let internal = self.targeted_internal_entries(start_idx, skip_to, targeted);
        if internal.is_empty() {
            return Err(PromotionGateRejection::NotSinglePredSucc);
        }
        if internal.len() > 2 {
            return Err(PromotionGateRejection::NotSinglePredSucc);
        }

        let region: HashSet<usize> = (start_idx..skip_to).collect();
        if self.region_has_external_entry(&region, start_idx) {
            return Err(PromotionGateRejection::ExternalEntry);
        }

        let single_pred = internal.iter().all(|idx| {
            let preds = &self.predecessors[*idx];
            !preds.is_empty() && preds.iter().all(|pred| region.contains(pred))
        });
        if !single_pred {
            return Err(PromotionGateRejection::NotSinglePredSucc);
        }

        let legacy_single_pred_succ = internal.iter().all(|idx| {
            let preds = &self.predecessors[*idx];
            !preds.is_empty()
                && preds
                    .iter()
                    .all(|pred| region.contains(pred) && *pred < *idx)
        });
        if legacy_single_pred_succ {
            // Fresh SCC on current successors (tests and some passes mutate `successors` without
            // refreshing `cfg_facts`).
            let scc = SccAnalysis::analyze(&self.successors, &self.predecessors);
            if region.iter().any(|&idx| scc.is_irreducible_node(idx)) {
                return self.ensure_graph_invariant_promotion_region(start_idx, &internal, &region);
            }
            return Ok(());
        }

        self.ensure_graph_invariant_promotion_region(start_idx, &internal, &region)
    }
}

#[cfg(test)]
mod region_exit_determinism_tests {
    use super::*;
    use crate::midend::ir::MlilPreviewOptions;
    use crate::pcode::{PcodeBasicBlock, PcodeFunction};

    fn make_builder_with_successors(successors: Vec<Vec<usize>>) -> PreviewBuilder<'static> {
        let blocks = successors
            .iter()
            .enumerate()
            .map(|(i, succs)| PcodeBasicBlock {
                index: i as u32,
                start_address: 0x1000 + (i as u64) * 0x10,
                successors: succs.iter().map(|&s| s as u32).collect(),
                ops: Vec::new(),
            })
            .collect();
        let pcode = Box::leak(Box::new(PcodeFunction { blocks }));
        let options = Box::leak(Box::new(MlilPreviewOptions {
            is_64bit: true,
            pointer_size: 8,
            ..Default::default()
        }));
        let mut builder = PreviewBuilder::new(pcode, options, None);
        builder.successors = successors;
        builder
    }

    /// A region with multiple external exit nodes (blocks 1 and 2 both branch
    /// outside the {0,1,2} region to block 3) must always report exits in the
    /// same order regardless of the region HashSet's internal iteration
    /// order — `.first()` on this list is a real gating decision, not just
    /// display order.
    #[test]
    fn region_external_exit_nodes_is_sorted_regardless_of_hashset_iteration_order() {
        let builder = make_builder_with_successors(vec![vec![1, 2], vec![3], vec![3], vec![]]);
        let region: std::collections::HashSet<usize> = [0, 1, 2].into_iter().collect();
        let exits = builder.region_external_exit_nodes(&region);
        assert_eq!(
            exits,
            vec![1, 2],
            "exit nodes must be returned in ascending, deterministic order"
        );
    }
}
