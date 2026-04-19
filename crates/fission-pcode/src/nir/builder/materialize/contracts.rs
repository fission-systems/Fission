use super::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReplacementReadClass {
    SameBlockData,
    PredicateSensitive,
    SelectorSensitive,
    ReturnPath,
    Merge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MaterializationRejectionReason {
    AliasUnsafe,
    MissingMergeBinding,
    ConsumerRequiresStableRepresentative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AliasUnsafeHazardKind {
    MultipleSameBlockConsumers,
    DisallowedSingleConsumer,
    CallBetweenDefUse,
    LoadAfterStore,
    SameBlockStore,
    UnknownNoConsumerFound,
    UnknownConsumerAfterTerminator,
    UnknownUnhandledConsumerKind,
    UnknownMalformedDefUseWindow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DisallowedSingleConsumerConsumerKind {
    BranchCondition,
    Predicate,
    CallArg,
    StoreAddr,
    StoreValue,
    LoadAddr,
    PhiMerge,
    OtherData,
    UnknownConsumerKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DisallowedSingleConsumerRhsKind {
    VarOrConst,
    UnaryBoolean,
    BinaryBoolean,
    Arithmetic,
    LoadLike,
    CallLike,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DisallowedSingleConsumerReason {
    ConsumerIsBranchCondition,
    ConsumerIsPredicate,
    ConsumerIsCallArg,
    ConsumerIsStoreAddr,
    ConsumerIsStoreValue,
    ConsumerIsLoadAddr,
    ConsumerIsPhiMerge,
    RhsNotLowCost,
    RhsHasLoad,
    RhsHasCall,
    UnknownConsumerKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DisallowedSingleConsumerProof {
    pub(super) consumer_block_addr: u64,
    pub(super) consumer_op_seq: u32,
    pub(super) consumer_opcode: PcodeOpcode,
    pub(super) matched_input_indices: Vec<usize>,
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(super) rhs_low_cost: bool,
    pub(super) rhs_has_load: bool,
    pub(super) rhs_has_call: bool,
    pub(super) reason: DisallowedSingleConsumerReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SingleConsumerCallRhsFamily {
    KnownPureIntrinsic,
    PreviewCalleeAnalysisUnsafe,
    UnknownInternalCall,
    ImportCall,
    CallOther,
    IndirectCall,
    UnknownCall,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SingleConsumerCallRhsProof {
    pub(super) consumer_block_addr: u64,
    pub(super) consumer_op_seq: u32,
    pub(super) consumer_opcode: PcodeOpcode,
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) call_target: String,
    pub(super) family: SingleConsumerCallRhsFamily,
    pub(super) rhs_low_cost: bool,
    pub(super) call_effect_source: Option<CallEffectSummarySource>,
    pub(super) writes_memory: Option<bool>,
    pub(super) may_call_unknown: Option<bool>,
    pub(super) may_exit: Option<bool>,
    pub(super) return_used: bool,
    pub(super) downstream_opcode: Option<PcodeOpcode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CarryIntrinsicPredicateUseFamily {
    CarryFeedsBoolOr,
    CarryFeedsCompareZero,
    CarryFeedsCompareNonZero,
    CarryFeedsArithmetic,
    CarryFeedsUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BoolOrDownstreamUseFamily {
    BoolOrFeedsPredicate,
    BoolOrFeedsBranch,
    BoolOrFeedsCompare,
    BoolOrFeedsData,
    UnknownBoolOrUse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CarryIntrinsicFinalPredicateContext {
    BoolOrOnly,
    CompareZero,
    CompareNonZero,
    BranchPredicate,
    PredicateChain,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CarryIntrinsicPredicateProof {
    pub(super) call_target: String,
    pub(super) args: Vec<String>,
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) downstream_opcode: PcodeOpcode,
    pub(super) bool_chain_role: CarryIntrinsicPredicateUseFamily,
    pub(super) rhs_low_cost: bool,
    pub(super) args_side_effect_free: bool,
    pub(super) final_predicate_context: CarryIntrinsicFinalPredicateContext,
    pub(super) boolor_downstream_use: Option<BoolOrDownstreamUseFamily>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IntrinsicCompareOnlyFamily {
    BorrowCompareZero,
    CarryCompareZero,
    SignedCarryCompareZero,
    PopCountCompareZero,
    UnknownIntrinsicCompare,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IntrinsicCompareFinalPredicateContext {
    CompareZero,
    CompareOne,
    CompareNonZero,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct IntrinsicCompareOnlyProof {
    pub(super) call_target: String,
    pub(super) args: Vec<String>,
    pub(super) downstream_opcode: PcodeOpcode,
    pub(super) compare_const: Option<i64>,
    pub(super) family: IntrinsicCompareOnlyFamily,
    pub(super) rhs_low_cost: bool,
    pub(super) args_side_effect_free: bool,
    pub(super) final_predicate_context: IntrinsicCompareFinalPredicateContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SingleConsumerLoadRhsFamily {
    LoadFeedsPredicate,
    LoadFeedsArithmetic,
    LoadFeedsAddressComputation,
    LoadFeedsStoreOrCall,
    LoadFeedsUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SingleConsumerLoadAliasClass {
    ReadOnlyLocalLoad,
    MayAliasSameBlockStore,
    MayAliasCall,
    VolatileOrUnknownLoad,
    GlobalOrExternalLoad,
    UnknownLoad,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SingleConsumerLoadRhsProof {
    pub(super) consumer_block_addr: u64,
    pub(super) consumer_op_seq: u32,
    pub(super) consumer_opcode: PcodeOpcode,
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) load_ptr: String,
    pub(super) family: SingleConsumerLoadRhsFamily,
    pub(super) alias_class: SingleConsumerLoadAliasClass,
    pub(super) same_block_store_before: bool,
    pub(super) same_block_store_after: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MissingMergeBindingRelation {
    JoinMergeMissing,
    LoopHeaderMergeMissing,
    BackedgeMergeMissing,
    PredicateMergeMissing,
    PhiLikeMergeMissing,
    RepresentativeOnlyMissing,
    UnknownMissingMerge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MissingMergeBindingProof {
    pub(super) merge_block: u64,
    pub(super) predecessor_count: usize,
    pub(super) incoming_value_count: usize,
    pub(super) has_existing_binding: bool,
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(super) relation: MissingMergeBindingRelation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UnknownConsumerKindReason {
    ConsumerOpcodeUnhandled,
    ConsumerHasMultipleMatchedInputs,
    ConsumerInputRoleUnknown,
    ConsumerIsIndirectUse,
    ConsumerIsAddressComputation,
    ConsumerIsSubpieceOrCast,
    ConsumerIsControlLike,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UnknownConsumerKindProof {
    pub(super) consumer_block_addr: u64,
    pub(super) consumer_op_seq: u32,
    pub(super) consumer_opcode: PcodeOpcode,
    pub(super) matched_input_indices: Vec<usize>,
    pub(super) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(super) reason: UnknownConsumerKindReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PopCountResultUseFamily {
    PopCountFeedsPredicate,
    PopCountFeedsArithmetic,
    PopCountFeedsCompareZero,
    PopCountFeedsCompareConst,
    PopCountFeedsStoreOrCall,
    PopCountResultUnused,
    UnknownPopCountUse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PopCountConsumerProof {
    pub(super) consumer_op_seq: u32,
    pub(super) input_width: u32,
    pub(super) output_width: Option<u32>,
    pub(super) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(super) rhs_low_cost: bool,
    pub(super) rhs_has_call: bool,
    pub(super) rhs_has_load: bool,
    pub(super) popcount_result_used_by: PopCountResultUseFamily,
    pub(super) downstream_consumer_opcode: Option<PcodeOpcode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PopCountIntAndMaskKind {
    AndOne,
    AndByteMask,
    AndPowerOfTwoMinusOne,
    AndNonPowerOfTwoMask,
    UnknownMask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PopCountIntAndDownstreamUseFamily {
    FeedsPredicate,
    FeedsCompareZero,
    FeedsCompareConst,
    FeedsArithmetic,
    FeedsStoreOrCall,
    FeedsUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PopCountIntAndChainProof {
    pub(super) popcount_consumer_op_seq: u32,
    pub(super) intand_op_seq: u32,
    pub(super) popcount_result: String,
    pub(super) intand_mask: Option<u64>,
    pub(super) intand_mask_kind: PopCountIntAndMaskKind,
    pub(super) intand_result_consumer: PopCountIntAndDownstreamUseFamily,
    pub(super) downstream_consumer_opcode: Option<PcodeOpcode>,
    pub(super) chain_low_cost: bool,
    pub(super) chain_side_effect_free: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ParityChainRole {
    PopCountInput,
    PopCountResult,
    IntAndResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ParityChainKeepReason {
    PopCountHasMultipleConsumers,
    IntAndMaskNotOne,
    IntAndHasMultipleConsumers,
    FinalConsumerNotCompare,
    CompareConstUnsupported,
    InterveningSideEffect,
    RhsNotLowCost,
    RhsHasLoad,
    RhsHasCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ParityChainConsumerContext {
    CompareZero,
    CompareNonZero,
    CompareOne,
    CompareNotOne,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ParityChainProof {
    pub(super) role: ParityChainRole,
    pub(super) popcount_op_seq: u32,
    pub(super) intand_op_seq: u32,
    pub(super) compare_op_seq: u32,
    pub(super) compare_opcode: PcodeOpcode,
    pub(super) compare_const: u64,
    pub(super) chain_low_cost: bool,
    pub(super) chain_side_effect_free: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SingleConsumerPredicateFamily {
    DirectFlag,
    NegatedFlag,
    CompareZero,
    CompareNonZero,
    CompareConst,
    CompareOtherVar,
    ComposedPredicate,
    UnknownPredicate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SingleConsumerPredicateProof {
    pub(super) consumer_block_addr: u64,
    pub(super) consumer_op_seq: u32,
    pub(super) consumer_opcode: PcodeOpcode,
    pub(super) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(super) predicate_family: SingleConsumerPredicateFamily,
    pub(super) guard_family: SingleConsumerPredicateFamily,
    pub(super) same_guard_as_consumer: bool,
    pub(super) requires_stable_representative: bool,
    pub(super) low_cost_if_predicate: bool,
    pub(super) has_call: bool,
    pub(super) has_load: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ArithmeticPredicateShape {
    LowBitAndOne,
    PowerOfTwoMask,
    NonPowerOfTwoMask,
    ShiftAndMask,
    UnknownArithmetic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ArithmeticPredicateStableReason {
    PredicateSensitive,
    ArithmeticMask,
    ConsumerCompare,
    NonCanonicalPredicate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ArithmeticPredicateProof {
    pub(super) consumer_guard: SingleConsumerPredicateFamily,
    pub(super) mask_kind: ArithmeticPredicateShape,
    pub(super) mask_value: Option<u64>,
    pub(super) boolean_width: bool,
    pub(super) low_cost: bool,
    pub(super) stable_required: bool,
    pub(super) stable_required_reason: Option<ArithmeticPredicateStableReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LowBitMaskPredicateFamily {
    BooleanFlagMask,
    IntegerBitTest,
    MaskFromCompareResult,
    MaskFromArithmeticValue,
    UnknownLowBitMask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LowBitMaskInputOriginKind {
    Compare,
    BoolOp,
    Arithmetic,
    Load,
    Call,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LowBitMaskPredicateProof {
    pub(super) family: LowBitMaskPredicateFamily,
    pub(super) mask_input: String,
    pub(super) consumer_guard: SingleConsumerPredicateFamily,
    pub(super) feeds_only_predicate: bool,
    pub(super) input_is_boolean_like: bool,
    pub(super) input_origin_kind: LowBitMaskInputOriginKind,
    pub(super) stable_required_reason: Option<ArithmeticPredicateStableReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MalformedDefUseWindowRelation {
    DefAfterTerminator,
    ConsumerBeforeDef,
    ConsumerAfterTerminator,
    ConsumerInDifferentBlock,
    TerminatorMissing,
    OpIndexMissing,
    BlockMismatch,
    RedefinitionBeforeConsumer,
    UnknownWindow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MalformedDefUseWindowDetail {
    pub(super) relation: MalformedDefUseWindowRelation,
    pub(super) def_op_idx: usize,
    pub(super) terminator_idx: Option<usize>,
    pub(super) consumer_count: usize,
    pub(super) first_consumer_block: Option<u64>,
    pub(super) first_consumer_idx: Option<usize>,
    pub(super) first_consumer_op_seq: Option<u32>,
    pub(super) rhs_kind: NoConsumerSuppressionRhsKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CrossBlockConsumerRelation {
    SuccessorBlock,
    JoinBlock,
    LoopBackedge,
    PostDominatorBlock,
    UnreachableOrUnclassified,
    MergePhiConsumer,
    OrdinaryDataConsumer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CrossBlockConsumerProvenance {
    pub(super) relation: CrossBlockConsumerRelation,
    pub(super) consumer_opcode: Option<PcodeOpcode>,
    pub(super) consumer_is_multiequal: bool,
    pub(super) immediate_successor: bool,
    pub(super) consumer_is_join: bool,
    pub(super) redefined_before_consumer: bool,
    pub(super) def_successor_count: usize,
    pub(super) consumer_predecessor_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CrossBlockReplacementProof {
    pub(super) relation: CrossBlockConsumerRelation,
    pub(super) dominates_consumer: bool,
    pub(super) rhs_low_cost: bool,
    pub(super) preserve_materialization: bool,
    pub(super) no_redefinition_before_consumer: bool,
    pub(super) merge_phi: bool,
    pub(super) def_successor_count: usize,
    pub(super) consumer_predecessor_count: usize,
    pub(super) narrow_candidate: bool,
    pub(super) consumer_opcode: Option<PcodeOpcode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CrossBlockRedefinitionRelation {
    RedefinedInDefBlockAfterDef,
    RedefinedOnEdge,
    RedefinedInConsumerBlockBeforeUse,
    RedefinedInSiblingPredecessor,
    PhiRedefinition,
    LoopCarriedRedefinition,
    UnknownRedefinition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CrossBlockRedefinitionDetail {
    pub(super) relation: CrossBlockRedefinitionRelation,
    pub(super) redef_block_addr: u64,
    pub(super) redef_op_idx: usize,
    pub(super) redef_op_seq: u32,
    pub(super) redef_opcode: PcodeOpcode,
    pub(super) redef_rhs_kind: SameBlockOverwriteRhsKind,
    pub(super) overwrite_shape: SameBlockOverwriteShapeKind,
    pub(super) def_to_redef_gap: usize,
    pub(super) redef_to_terminator_gap: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CopyOverwriteRestartProof {
    pub(super) consumer_relation: CrossBlockConsumerRelation,
    pub(super) redef_op_seq: u32,
    pub(super) redef_rhs: String,
    pub(super) same_value: bool,
    pub(super) redef_dominates_consumer: bool,
    pub(super) old_def_has_pre_redef_use: bool,
    pub(super) consumer_block_addr: u64,
    pub(super) consumer_op_seq: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PredicateOverwriteRefreshProof {
    pub(super) consumer_relation: CrossBlockConsumerRelation,
    pub(super) redef_op_seq: u32,
    pub(super) redef_rhs: String,
    pub(super) predicate_consumer_block_addr: u64,
    pub(super) predicate_consumer_op_seq: u32,
    pub(super) predicate_rhs: String,
    pub(super) same_guard_family: bool,
    pub(super) old_def_has_pre_redef_use: bool,
    pub(super) redef_dominates_predicate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LoopCarriedValueKind {
    BooleanFlag,
    CounterIncrement,
    PointerAdvance,
    Accumulator,
    UnknownLoopCarried,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LoopBooleanGuardFamily {
    DirectFlag,
    NegatedFlag,
    EqZero,
    NeZero,
    NonPredicate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LoopBoundaryBindingFamily {
    BoolNegate,
    IntNotEqual,
    OtherBooleanFlag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LoopCarriedOverwriteProvenance {
    pub(super) loop_header: u64,
    pub(super) backedge_block: u64,
    pub(super) consumer_block: u64,
    pub(super) consumer_op_seq: u32,
    pub(super) redef_op_seq: u32,
    pub(super) redef_rhs: String,
    pub(super) has_multiequal: bool,
    pub(super) phi_input_count: usize,
    pub(super) induction_like: bool,
    pub(super) carried_value_kind: LoopCarriedValueKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LoopBooleanFlagProof {
    pub(super) consumer_opcode: PcodeOpcode,
    pub(super) exit_edge: Option<u64>,
    pub(super) backedge_edge: Option<u64>,
    pub(super) guard_family: LoopBooleanGuardFamily,
    pub(super) same_guard_as_exit: bool,
    pub(super) old_def_has_pre_redef_use: bool,
    pub(super) redef_dominates_backedge: bool,
    pub(super) consumer_is_loop_header_predicate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LoopGuardRefreshDominanceReason {
    ProvedBySingleBackedge,
    RedefAfterBackedgeBranch,
    RedefInNonBackedgeBlock,
    MultipleBackedgeBlocks,
    HeaderPredicateUsesIntermediate,
    MissingBackedgeTerminator,
    UnknownDominance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LoopGuardRefreshDominanceProof {
    pub(super) redef_block: u64,
    pub(super) backedge_block: u64,
    pub(super) redef_before_backedge_branch: bool,
    pub(super) all_backedge_paths_covered: bool,
    pub(super) header_predicate_uses_redef: bool,
    pub(super) reason: LoopGuardRefreshDominanceReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LoopBoundaryBindingCorrelation {
    pub(super) loop_header: u64,
    pub(super) family: LoopBoundaryBindingFamily,
    pub(super) missing_merge_binding: bool,
    pub(super) stable_representative_required: bool,
    pub(super) merge_block: Option<u64>,
    pub(super) candidate_binding: String,
    pub(super) existing_binding: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(in crate::nir::builder) struct MaterializeOwnerRepartition {
    pub(super) alias_unsafe_hazard_kind: BTreeMap<String, usize>,
    pub(super) disallowed_single_consumer_reason: BTreeMap<String, usize>,
    pub(super) disallowed_single_consumer_consumer_kind: BTreeMap<String, usize>,
    pub(super) disallowed_single_consumer_rhs_kind: BTreeMap<String, usize>,
    pub(super) single_consumer_call_rhs_family: BTreeMap<String, usize>,
    pub(super) single_consumer_call_rhs_effect_source: BTreeMap<String, usize>,
    pub(super) single_consumer_call_rhs_consumer_kind: BTreeMap<String, usize>,
    pub(super) single_consumer_call_rhs_downstream_opcode: BTreeMap<String, usize>,
    pub(super) carry_intrinsic_predicate_family: BTreeMap<String, usize>,
    pub(super) carry_intrinsic_boolor_downstream_use: BTreeMap<String, usize>,
    pub(super) carry_intrinsic_final_predicate_context: BTreeMap<String, usize>,
    pub(super) intrinsic_compare_only_family: BTreeMap<String, usize>,
    pub(super) intrinsic_compare_only_final_predicate_context: BTreeMap<String, usize>,
    pub(super) single_consumer_load_rhs_family: BTreeMap<String, usize>,
    pub(super) single_consumer_load_rhs_alias_class: BTreeMap<String, usize>,
    pub(super) missing_merge_binding_relation: BTreeMap<String, usize>,
    pub(super) unknown_consumer_kind_reason: BTreeMap<String, usize>,
    pub(super) unknown_consumer_kind_opcode: BTreeMap<String, usize>,
    pub(super) popcount_consumer_result_use: BTreeMap<String, usize>,
    pub(super) popcount_consumer_downstream_opcode: BTreeMap<String, usize>,
    pub(super) popcount_intand_mask_kind: BTreeMap<String, usize>,
    pub(super) popcount_intand_downstream_use: BTreeMap<String, usize>,
    pub(super) parity_chain_regression_role: BTreeMap<String, usize>,
    pub(super) parity_chain_regression_before_event: BTreeMap<String, usize>,
    pub(super) parity_chain_regression_consumer_context: BTreeMap<String, usize>,
    pub(super) single_consumer_predicate_family: BTreeMap<String, usize>,
    pub(super) single_consumer_predicate_guard_family: BTreeMap<String, usize>,
    pub(super) single_consumer_predicate_same_guard: BTreeMap<String, usize>,
    pub(super) single_consumer_predicate_requires_stable: BTreeMap<String, usize>,
    pub(super) arithmetic_predicate_shape: BTreeMap<String, usize>,
    pub(super) arithmetic_predicate_consumer_guard: BTreeMap<String, usize>,
    pub(super) arithmetic_predicate_boolean_width: BTreeMap<String, usize>,
    pub(super) arithmetic_predicate_stable_reason: BTreeMap<String, usize>,
    pub(super) low_bit_mask_predicate_family: BTreeMap<String, usize>,
    pub(super) low_bit_mask_input_origin_kind: BTreeMap<String, usize>,
    pub(super) low_bit_mask_feeds_only_predicate: BTreeMap<String, usize>,
    pub(super) low_bit_mask_input_is_boolean_like: BTreeMap<String, usize>,
    pub(super) materialization_rejection_reason: BTreeMap<String, usize>,
    pub(super) malformed_def_use_window_relation: BTreeMap<String, usize>,
    pub(super) cross_block_consumer_relation: BTreeMap<String, usize>,
    pub(super) cross_block_redefinition_relation: BTreeMap<String, usize>,
    pub(super) same_block_overwrite_shape_kind: BTreeMap<String, usize>,
    pub(super) loop_carried_value_kind: BTreeMap<String, usize>,
    pub(super) loop_boolean_guard_family: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SameBlockOverwriteShapeKind {
    OverwriteBeforeBranch,
    OverwriteAtPredicateProducer,
    OverwriteAtLoopUpdate,
    OverwriteAtCallResult,
    OverwriteAtLoadResult,
    OverwriteAtCopy,
    OverwriteUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SameBlockOverwriteRhsKind {
    CopyLike,
    Predicate,
    Arithmetic,
    Load,
    Call,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AliasUnsafeHazard {
    pub(super) kind: AliasUnsafeHazardKind,
    pub(super) use_stmt_idx: Option<usize>,
    pub(super) hazard_stmt_idx: Option<usize>,
    pub(super) hazard_opcode: Option<PcodeOpcode>,
}

impl AliasUnsafeHazard {
    pub(super) fn new(
        kind: AliasUnsafeHazardKind,
        use_stmt_idx: Option<usize>,
        hazard_stmt_idx: Option<usize>,
        hazard_opcode: Option<PcodeOpcode>,
    ) -> Self {
        Self {
            kind,
            use_stmt_idx,
            hazard_stmt_idx,
            hazard_opcode,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReplacementCompleteness {
    Complete,
    Incomplete(MaterializationRejectionReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ReplacementValuePlan {
    pub(super) dominant_read: ReplacementReadClass,
    pub(super) completeness: ReplacementCompleteness,
}

impl ReplacementValuePlan {
    pub(super) fn complete(dominant_read: ReplacementReadClass) -> Self {
        Self {
            dominant_read,
            completeness: ReplacementCompleteness::Complete,
        }
    }

    pub(super) fn incomplete(
        dominant_read: ReplacementReadClass,
        reason: MaterializationRejectionReason,
    ) -> Self {
        Self {
            dominant_read,
            completeness: ReplacementCompleteness::Incomplete(reason),
        }
    }

    pub(super) fn is_complete(self) -> bool {
        matches!(self.completeness, ReplacementCompleteness::Complete)
    }

    pub(super) fn rejection_reason(self) -> Option<MaterializationRejectionReason> {
        match self.completeness {
            ReplacementCompleteness::Complete => None,
            ReplacementCompleteness::Incomplete(reason) => Some(reason),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NoConsumerMaterializationProfile {
    pub(super) same_block_consumers: usize,
    pub(super) cross_block_consumers: usize,
    pub(super) has_later_block_use: bool,
    pub(super) has_phi_merge_use: bool,
    pub(super) has_debug_use: bool,
    pub(super) rhs_side_effectful: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NoConsumerMaterializationDecision {
    Suppress,
    Keep(NoConsumerMaterializationKeepReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NoConsumerMaterializationKeepReason {
    NotUnknownNoConsumerFound,
    SuppressionDisabled,
    StateVisibleOutput,
    SameBlockConsumerPresent,
    CrossBlockConsumerPresent,
    LaterBlockUsePresent,
    PhiMergeUsePresent,
    DebugUsePresent,
    LegacyInlineCandidate,
    PreserveMaterialization,
    RhsSideEffectful,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NoConsumerSuppressionRhsKind {
    Var,
    Const,
    Cast,
    Unary,
    Binary,
    Load,
    Call,
    Aggregate,
    PtrOffset,
    Index,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NoConsumerSuppressionBlockPosition {
    Local,
    PreBranch,
    PredicateAdjacent,
    ReturnAdjacent,
    MergeAdjacent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NoConsumerSuppressionOutputKind {
    TempOnly,
    RegisterVisible,
    MemoryDerived,
}
