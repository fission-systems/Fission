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

pub(super) fn guarded_tail_call_target_is_known_pure_helper(target: &str) -> bool {
    matches!(target, "__carry" | "__scarry" | "__sborrow" | "__popcount")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GuardedTailCanonicalizationFailure {
    MultiplePayloadEntries,
    InterleavedJoinUses,
    NonterminalJoinLabel,
    NestedTailEscape,
    AliasNotFallthrough,
    AliasHasMultipleInternalPredecessors,
    AliasHasNonlocalRef,
    PayloadCrossesJoin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NestedBeforeOwnershipClass {
    GuardFamilyInternalizable,
    PairedBoundaryInternalizable,
    NestedBeforeExternalOwner,
    NestedBeforeCrossesTerminalJoin,
    NestedBeforeNonlocalPayload,
    NestedBeforeUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AliasOwnershipLegalityReason {
    Complete,
    ExternalOwner,
    CrossesTerminalJoin,
    NonlocalPayload,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NestedBeforeAliasWitness {
    pub(super) stmt_idx: usize,
    pub(super) cond: Option<HirExpr>,
    pub(super) class: NestedBeforeOwnershipClass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AliasOwnershipProof {
    pub(super) label: String,
    pub(super) raw_nested_before: usize,
    pub(super) internalized_nested_before: usize,
    pub(super) class: NestedBeforeOwnershipClass,
    pub(super) legality_reason: AliasOwnershipLegalityReason,
    pub(super) witnesses: Vec<NestedBeforeAliasWitness>,
}

impl AliasOwnershipProof {
    pub(super) fn effective_nested_before(&self) -> usize {
        self.raw_nested_before
            .saturating_sub(self.internalized_nested_before)
    }

    pub(super) fn is_complete(&self) -> bool {
        matches!(self.legality_reason, AliasOwnershipLegalityReason::Complete)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GuardedTailWitnessRejection {
    MissingTerminalJoin,
    SideEntryConflict,
    AliasInterleaveConflict,
    AmbiguousFollow,
    NonCanonicalLayout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RegionShapeWitness {
    pub(super) target_label: String,
    pub(super) label_idx: usize,
    pub(super) keep_middle_when_cond_true: bool,
    pub(super) middle: Vec<HirStmt>,
    pub(super) external_redirects: Vec<(String, String)>,
    pub(super) terminal_join_present: bool,
    pub(super) follow_witness: bool,
    pub(super) side_entry_free: bool,
    pub(super) alias_interleave_legal: bool,
}

impl RegionShapeWitness {
    pub(super) fn is_complete(&self) -> bool {
        self.terminal_join_present
            && self.follow_witness
            && self.side_entry_free
            && self.alias_interleave_legal
    }

    pub(super) fn region_legality(&self) -> RegionLegality {
        RegionLegality {
            entry_unique: true,
            terminal_join_present: self.terminal_join_present,
            follow_witness: self.follow_witness,
            postdom_witness: self.terminal_join_present && self.follow_witness,
            side_entry_free: self.side_entry_free,
            side_exit_legal: self.follow_witness,
            alias_interleave_legal: self.alias_interleave_legal,
            selector_side_effect_free: false,
            ordinal_domain_complete: false,
            shared_tail_conflict_free: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GuardedTailReadKind {
    AssignRhs,
    ConditionExpr,
    ReturnExpr,
    CallArg,
    SwitchSelector,
    NestedExpr,
    JoinPhiLikeUse,
    MiddleGoto,
    ExternalForwardGoto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GuardedTailReplacementRead {
    pub(super) stmt_idx: usize,
    pub(super) kind: GuardedTailReadKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GuardedTailExportedBinding {
    pub(super) def_stmt_idx: usize,
    pub(super) binding_name: String,
    pub(super) replacement_source: HirExpr,
    pub(super) read_sites: Vec<GuardedTailReplacementRead>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GuardedTailSyntheticMerge {
    pub(super) binding_name: String,
    pub(super) replacement_target: String,
    pub(super) then_value: HirExpr,
    pub(super) else_value: HirExpr,
    pub(super) read_sites: Vec<GuardedTailReplacementRead>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GuardedTailTrial {
    pub(super) witness: RegionShapeWitness,
    pub(super) follow_block: Option<String>,
    pub(super) candidate_reads: Vec<GuardedTailReplacementRead>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GuardedTailExecutionRejection {
    Witness(GuardedTailWitnessRejection),
    ReplacementIncomplete,
    MustEmitLabelConflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GuardedTailVerification {
    pub(super) region_legality: RegionLegality,
    pub(super) replacement_complete: bool,
    pub(super) removable_ops_legal: bool,
    pub(super) rewritten_middle: Vec<HirStmt>,
    pub(super) exported_bindings: Vec<GuardedTailExportedBinding>,
    pub(super) rejection_reason: Option<GuardedTailExecutionRejection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GuardedTailExecutionPlan {
    pub(super) synthetic_merges: Vec<GuardedTailSyntheticMerge>,
    pub(super) redirects: Vec<(String, String)>,
    pub(super) rewritten_middle: Vec<HirStmt>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct GuardedTailReplacementCache {
    pub(super) else_sources: HashMap<String, HirExpr>,
}

impl From<GuardedTailWitnessRejection> for RegionRejectionReason {
    fn from(value: GuardedTailWitnessRejection) -> Self {
        match value {
            GuardedTailWitnessRejection::MissingTerminalJoin => Self::MissingTerminalJoin,
            GuardedTailWitnessRejection::SideEntryConflict => Self::SideEntryConflict,
            GuardedTailWitnessRejection::AliasInterleaveConflict => Self::AliasInterleaveConflict,
            GuardedTailWitnessRejection::AmbiguousFollow => Self::AmbiguousFollow,
            GuardedTailWitnessRejection::NonCanonicalLayout => Self::NonCanonicalLayout,
        }
    }
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
