use super::cleanup::{
    collect_referenced_label_counts, has_non_ignorable_payload, has_top_level_label,
    is_ignorable_discovery_stmt, normalize_guarded_tail_layout, single_goto_target,
    trim_ignorable_stmt_bounds,
};
use super::*;

mod canonicalize;
mod alias_refs;
mod promotion_graph;
mod promotion;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GuardedTailCanonicalizationFailure {
    MultiplePayloadEntries,
    InterleavedJoinUses,
    NonterminalJoinLabel,
    NestedTailEscape,
    AliasNotFallthrough,
    AliasHasMultipleInternalPredecessors,
    AliasHasNonlocalRef,
    AliasBodyNotTrivial,
    PayloadCrossesJoin,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PromotionGateRejection {
    MustEmitLabel,
    MustEmitLabelSurvivingMiddleRef,
    MustEmitLabelSurvivingExternalRef,
    MustEmitLabelOwnerConflict,
    NotSinglePredSucc,
    ExternalEntry,
    LoopOrSwitchTarget,
}

#[derive(Clone, Copy)]
pub(super) enum PromotionShapeRejection {
    MissingTerminalJoinTarget,
    EmptyNonterminalTail,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PcodeBasicBlock;

    fn test_options() -> MlilPreviewOptions {
        MlilPreviewOptions {
            pe_x64_only: true,
            is_64bit: true,
            pointer_size: 8,
            format: "PE".to_string(),
            image_base: 0,
            sections: Vec::new(),
            region_linearize_structuring: false,
            force_linear_structuring: false,
            conservative_irreducible_fallback: false,
        }
    }

    fn test_pcode_with_blocks(count: usize) -> PcodeFunction {
        let blocks = (0..count)
            .map(|idx| PcodeBasicBlock {
                index: idx as u32,
                start_address: 0x1000 + (idx as u64) * 0x10,
                ops: Vec::new(),
            })
            .collect();
        PcodeFunction { blocks }
    }

    #[test]
    fn minimal_structured_promotion_accepts_non_monotonic_layout_when_graph_invariants_hold() {
        let pcode = test_pcode_with_blocks(4);
        let options = test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        let successors = vec![vec![2], vec![3], vec![1], vec![]];
        builder.successors = successors.clone();
        builder.predecessors = build_predecessor_index_map(&successors);

        let targeted = HashSet::from([builder.block_target_key(1)]);
        let result = builder.is_minimal_structured_promotion_candidate(0, 3, &targeted);
        assert!(result.is_ok());
    }

    #[test]
    fn minimal_structured_promotion_rejects_irreducible_region() {
        let pcode = test_pcode_with_blocks(4);
        let options = test_options();
        let mut builder = PreviewBuilder::new(&pcode, &options, None);

        let successors = vec![vec![1, 2], vec![2], vec![1, 3], vec![]];
        builder.successors = successors.clone();
        builder.predecessors = build_predecessor_index_map(&successors);

        let targeted = HashSet::from([builder.block_target_key(1)]);
        let result = builder.is_minimal_structured_promotion_candidate(0, 3, &targeted);
        assert_eq!(result, Err(PromotionGateRejection::NotSinglePredSucc));
    }
}
