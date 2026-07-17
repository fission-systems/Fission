//! Guarded-tail shared types.

use fission_midend_core::ir::{HirExpr, HirStmt};
use crate::regions::{RegionKind, RegionLegality, RegionRejectionReason};
use std::collections::HashMap;

pub fn guarded_tail_call_target_is_known_pure_helper(target: &str) -> bool {
    matches!(target, "__carry" | "__scarry" | "__sborrow" | "__popcount")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardedTailCanonicalizationFailure {
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
pub enum NestedBeforeOwnershipClass {
    GuardFamilyInternalizable,
    PairedBoundaryInternalizable,
    NestedBeforeExternalOwner,
    NestedBeforeCrossesTerminalJoin,
    NestedBeforeNonlocalPayload,
    NestedBeforeUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasOwnershipLegalityReason {
    Complete,
    ExternalOwner,
    CrossesTerminalJoin,
    NonlocalPayload,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NestedBeforeAliasWitness {
    pub stmt_idx: usize,
    pub cond: Option<HirExpr>,
    pub class: NestedBeforeOwnershipClass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliasOwnershipProof {
    pub label: String,
    pub raw_nested_before: usize,
    pub internalized_nested_before: usize,
    pub class: NestedBeforeOwnershipClass,
    pub legality_reason: AliasOwnershipLegalityReason,
    pub witnesses: Vec<NestedBeforeAliasWitness>,
}

impl AliasOwnershipProof {
    pub fn effective_nested_before(&self) -> usize {
        self.raw_nested_before
            .saturating_sub(self.internalized_nested_before)
    }

    pub fn is_complete(&self) -> bool {
        matches!(self.legality_reason, AliasOwnershipLegalityReason::Complete)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardedTailWitnessRejection {
    MissingTerminalJoin,
    SideEntryConflict,
    AliasInterleaveConflict,
    AmbiguousFollow,
    NonCanonicalLayout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionShapeWitness {
    pub target_label: String,
    pub label_idx: usize,
    pub keep_middle_when_cond_true: bool,
    pub middle: Vec<HirStmt>,
    pub external_redirects: Vec<(String, String)>,
    pub terminal_join_present: bool,
    pub follow_witness: bool,
    pub side_entry_free: bool,
    pub alias_interleave_legal: bool,
}

impl RegionShapeWitness {
    pub fn is_complete(&self) -> bool {
        self.terminal_join_present
            && self.follow_witness
            && self.side_entry_free
            && self.alias_interleave_legal
    }

    pub fn region_legality(&self) -> RegionLegality {
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
pub enum GuardedTailReadKind {
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
pub struct GuardedTailReplacementRead {
    pub stmt_idx: usize,
    pub kind: GuardedTailReadKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardedTailExportedBinding {
    pub def_stmt_idx: usize,
    pub binding_name: String,
    pub replacement_source: HirExpr,
    pub read_sites: Vec<GuardedTailReplacementRead>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardedTailSyntheticMerge {
    pub binding_name: String,
    pub replacement_target: String,
    pub then_value: HirExpr,
    pub else_value: HirExpr,
    pub read_sites: Vec<GuardedTailReplacementRead>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardedTailTrial {
    pub witness: RegionShapeWitness,
    pub follow_block: Option<String>,
    pub candidate_reads: Vec<GuardedTailReplacementRead>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardedTailExecutionRejection {
    Witness(GuardedTailWitnessRejection),
    ReplacementIncomplete,
    MustEmitLabelConflict,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardedTailVerification {
    pub region_legality: RegionLegality,
    pub replacement_complete: bool,
    pub removable_ops_legal: bool,
    pub rewritten_middle: Vec<HirStmt>,
    pub exported_bindings: Vec<GuardedTailExportedBinding>,
    pub rejection_reason: Option<GuardedTailExecutionRejection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardedTailExecutionPlan {
    pub synthetic_merges: Vec<GuardedTailSyntheticMerge>,
    pub redirects: Vec<(String, String)>,
    pub rewritten_middle: Vec<HirStmt>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuardedTailReplacementCache {
    pub else_sources: HashMap<String, HirExpr>,
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
pub enum PromotionGateRejection {
    MustEmitLabel,
    MustEmitLabelSurvivingMiddleRef,
    MustEmitLabelSurvivingExternalRef,
    MustEmitLabelOwnerConflict,
    NotSinglePredSucc,
    ExternalEntry,
    LoopOrSwitchTarget,
}

#[derive(Clone, Copy)]
pub enum PromotionShapeRejection {
    MissingTerminalJoinTarget,
    EmptyNonterminalTail,
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionAssumption {
    pub expr: HirExpr,
    pub value: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SuffixTailRejection {
    SuffixHasSideEffect { stmt_idx: usize },
    SuffixHasNonTerminalGoto { stmt_idx: usize, target: String },
    SuffixHasNestedOrNonlocalRef { stmt_idx: usize },
    SuffixHasLabelCrossing { stmt_idx: usize, label: String },
    SuffixHasExternalEntry { stmt_idx: usize, label: String },
    SuffixHasLoopOrSwitchCrossing { stmt_idx: usize },
    SuffixAliasRedirectUnresolved { stmt_idx: usize, label: String },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SuffixExternalEntryBudget {
    raw_refs: usize,
    internal_top_level_refs: usize,
    suffix_safe_refs: usize,
    guard_family_internalized_refs: usize,
    paired_nested_boundary_refs: usize,
    effective_external_refs: usize,
    allowed_external_refs: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExternalEntryRefKind {
    TopLevelExternalGoto,
    NestedConditionalGoto,
    AliasRedirectDerived,
    LoopSwitchDerived,
    UnknownExternalEntry,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NestedSuffixShapeKind {
    NestedSingleGotoThen,
    NestedSingleGotoElse,
    NestedBothBranches,
    NestedMultiStmtBranch,
    NestedNonlocalTarget,
    NestedGuardFamilyMismatch,
    NestedCrossesTerminalJoin,
    NestedUnknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SuffixSideEffectShapeKind {
    PureRegisterAssign,
    PureTempAssign,
    MemoryReadOnlyAssign,
    CallExprSideEffect,
    MemoryWrite,
    VolatileOrUnknownLoad,
    CompoundAssignOrPhiLike,
    UnknownSideEffect,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SuffixCallEffectShapeKind {
    VoidUnknownCall,
    ReturnValueIgnoredCall,
    ReturnValueAssignedLocal,
    PureKnownHelperCall,
    MemoryMutatingCall,
    ControlEffectCall,
    UnknownCallEffect,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NestedEntryBoundaryContext {
    label_idx: Option<usize>,
    label_in_current_suffix_window: bool,
    raw_refs: usize,
    internal_candidate_refs: usize,
    suffix_safe_refs: usize,
    external_pre_guard_internalization: usize,
    external_entry_kind: Option<ExternalEntryRefKind>,
    external_entry_ref_stmt_idx: Option<usize>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct NestedBoundaryRefTrace {
    stmt_idx: usize,
    kind: ExternalEntryRefKind,
    cond: Option<HirExpr>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct NestedBoundaryPairTrace {
    ref_count: usize,
    same_guard_family: bool,
    relation_reason: Option<&'static str>,
    conds: Vec<HirExpr>,
}
