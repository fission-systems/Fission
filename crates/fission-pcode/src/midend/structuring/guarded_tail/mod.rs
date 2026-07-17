use super::cleanup::{
    collect_referenced_label_counts, has_non_ignorable_payload, has_top_level_label,
    is_ignorable_discovery_stmt, normalize_guarded_tail_layout, single_goto_target,
    trim_ignorable_stmt_bounds,
};
use super::*;

mod alias_refs;
mod canonicalize;
mod execution;
mod promotion;
mod promotion_graph;
mod replacement;
mod suffix_window;

// Types owned by fission-midend-structuring (ADR 0012 residual conversion).
pub(super) use fission_midend_structuring::guarded_tail::types::{
    AliasOwnershipLegalityReason, AliasOwnershipProof, ExternalEntryRefKind,
    GuardedTailCanonicalizationFailure, GuardedTailExecutionPlan, GuardedTailExecutionRejection,
    GuardedTailExportedBinding, GuardedTailReadKind, GuardedTailReplacementCache,
    GuardedTailReplacementRead, GuardedTailSyntheticMerge, GuardedTailTrial,
    GuardedTailVerification, GuardedTailWitnessRejection, NestedBeforeAliasWitness,
    NestedBeforeOwnershipClass, NestedBoundaryPairTrace, NestedBoundaryRefTrace,
    NestedEntryBoundaryContext, NestedSuffixShapeKind, PromotionGateRejection,
    PromotionShapeRejection, RegionShapeWitness, SuffixCallEffectShapeKind,
    SuffixExternalEntryBudget, SuffixSideEffectShapeKind, SuffixTailRejection,
    guarded_tail_call_target_is_known_pure_helper,
};

// Promotion entry free functions.
pub use fission_midend_structuring::{
    discover_guarded_tail_candidates, promote_guarded_tail_regions_until_stable,
    promote_single_entry_guarded_tail_regions,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PcodeBasicBlock;

    fn test_options() -> MlilPreviewOptions {
        MlilPreviewOptions {
            pe_x64_only: true,
            is_64bit: true,
            is_big_endian: false,
            pointer_size: 8,
            format: "PE".to_string(),
            image_base: 0,
            sections: Vec::new(),
            region_linearize_structuring: false,
            force_linear_structuring: false,
            conservative_irreducible_fallback: false,
            structuring_engine: StructuringEngineKind::GraphCollapseV1,
            global_names: Default::default(),
            global_sizes: Default::default(),
            relocation_names: Default::default(),
            calling_convention: Default::default(),
            ..Default::default()
        }
    }

    fn test_pcode_with_blocks(count: usize) -> PcodeFunction {
        let blocks = (0..count)
            .map(|idx| PcodeBasicBlock {
                index: idx as u32,
                start_address: 0x1000 + (idx as u64) * 0x10,
                successors: vec![],
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
