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
    RepresentativeRootAttribution,
    TempOnlyRepresentativeLifecycle,
    DeadTempRepresentative,
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
pub(super) enum JoinMergeMissingReason {
    AllIncomingSame,
    MissingIncomingForSomePred,
    ConflictingIncomingValues,
    SinglePredValueOnly,
    StoreValueMerge,
    OtherDataMerge,
    PredicateMerge,
    UnknownJoinMerge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct JoinMergeMissingProof {
    pub(super) event_block: u64,
    pub(super) merge_block: u64,
    pub(super) predecessor_blocks: Vec<u64>,
    pub(super) incoming_value_count: usize,
    pub(super) incoming_values: Vec<String>,
    pub(super) values_same_across_preds: bool,
    pub(super) has_missing_incoming: bool,
    pub(super) has_conflicting_incoming: bool,
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(super) reason: JoinMergeMissingReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum MergeBindingCandidateIncomingKind {
    VarOrConst,
    Predicate,
    Arithmetic,
    LoadLike,
    CallLike,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MergeBindingCandidateResult {
    MissingIncomingSemanticsRequired,
    PhiLikeBindingCandidate,
    IncomingKindsUnsafe,
    InsufficientConflictingIncoming,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MergeBindingCandidateProof {
    pub(super) merge_block: u64,
    pub(super) predecessor_count: usize,
    pub(super) missing_incoming_count: usize,
    pub(super) conflicting_incoming_count: usize,
    pub(super) incoming_value_kinds: Vec<MergeBindingCandidateIncomingKind>,
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(super) can_synthesize_phi_like_binding: bool,
    pub(super) result: MergeBindingCandidateResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MissingIncomingSemanticsResult {
    DeadOnlyMissing,
    EntryDefaultRequired,
    PathSensitiveMissing,
    TempOnlyLeakage,
    UnsafePriorDefReuse,
    NoSafeSemantics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MissingIncomingSemanticsProof {
    pub(super) merge_block: u64,
    pub(super) predecessor_count: usize,
    pub(super) missing_pred_count: usize,
    pub(super) defined_pred_count: usize,
    pub(super) defined_incoming_values: Vec<String>,
    pub(super) missing_pred_kinds: Vec<String>,
    pub(super) missing_pred_has_prior_def: bool,
    pub(super) missing_pred_prior_def_status: String,
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(super) candidate_semantics: String,
    pub(super) result: MissingIncomingSemanticsResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExplicitMergeBindingTrialReason {
    PhiLikeBindingMaterialized,
    RejectedMissingIncoming,
    RejectedUnsafeIncomingKind,
    RejectedConsumerKind,
    RejectedRootAttribution,
    RejectedNonBinaryPreds,
    RejectedMultipleConflicts,
    RejectedNotJoinMerge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MissingIncomingPredKind {
    MissingBecauseNoPriorDef,
    MissingBecausePriorDefDominates,
    MissingBecauseDeadPred,
    MissingBecauseEntryDefault,
    MissingBecauseLoopBackedge,
    MissingBecausePathSensitive,
    UnknownMissingIncoming,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MissingIncomingPredProof {
    pub(super) event_block: u64,
    pub(super) merge_block: u64,
    pub(super) pred_block: u64,
    pub(super) pred_reaches_merge: bool,
    pub(super) pred_has_definition: bool,
    pub(super) pred_has_prior_definition: bool,
    pub(super) prior_def_block: Option<u64>,
    pub(super) prior_def_op_seq: Option<u32>,
    pub(super) incoming_kind: MissingIncomingPredKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MissingNoPriorDefReason {
    TrueNoPriorDef,
    EntryDefaultCandidate,
    DeadPredNoDef,
    UndefinedIncoming,
    StackSlotDefault,
    RegisterDefault,
    TempOnlyNoDef,
    UnknownNoPriorDef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MissingNoPriorDefProof {
    pub(super) merge_block: u64,
    pub(super) pred_block: u64,
    pub(super) pred_reaches_merge: bool,
    pub(super) pred_is_entry: bool,
    pub(super) pred_is_dead: bool,
    pub(super) output_space: u64,
    pub(super) output_size: u32,
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(super) default_candidate: String,
    pub(super) reason: MissingNoPriorDefReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TempOnlyRepresentativeReason {
    TempRepresentativeResidue,
    RootAttributedTemp,
    MergeCrossingTemp,
    DeadTempRepresentative,
    StoreValueTemp,
    OtherDataTemp,
    UnknownTempRepresentative,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TempOnlyRepresentativeProof {
    pub(super) merge_block: u64,
    pub(super) pred_block: Option<u64>,
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(super) defining_event: String,
    pub(super) materialization_event: String,
    pub(super) has_real_storage: bool,
    pub(super) has_later_use: bool,
    pub(super) crosses_merge: bool,
    pub(super) root_attributed: bool,
    pub(super) reason: TempOnlyRepresentativeReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StableRepresentativeOwnerReason {
    RootRepresentativeStableRequired,
    TempLifecycleStableRequired,
    RealMergeStableRequired,
    PredicateStableRequired,
    StoreValueStableRequired,
    AliasStableRequired,
    UnknownStableRepresentative,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StableRepresentativeOwnerProof {
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(super) overlaps_representative_root_attribution: bool,
    pub(super) overlaps_temp_only_lifecycle: bool,
    pub(super) overlaps_real_missing_merge: bool,
    pub(super) downstream_opcode: Option<PcodeOpcode>,
    pub(super) reason: StableRepresentativeOwnerReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AliasStableRequiredFamily {
    LoadAddrStableRequired,
    StoreAddrStableRequired,
    OtherDataLoadLikeStable,
    OtherDataCopyStable,
    BranchIndStableRequired,
    ArithmeticStableRequired,
    UnknownAliasStable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AliasStableRequiredProof {
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(super) downstream_opcode: Option<PcodeOpcode>,
    pub(super) same_block_use_count: usize,
    pub(super) rhs_has_load: bool,
    pub(super) rhs_has_call: bool,
    pub(super) requires_preserved_expr: bool,
    pub(super) reason: AliasStableRequiredFamily,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AddressStableRequiredFamily {
    AddressExprHasLoad,
    AddressExprHasCall,
    AddressExprPureArithmetic,
    AddressExprStackRelative,
    AddressExprGlobalRelative,
    AddressExprRegisterBase,
    AddressExprUnknownBase,
    AddressExprCrossesStore,
    AddressExprCrossesCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AddressStableRequiredBaseKind {
    StackRelative,
    GlobalRelative,
    RegisterBase,
    UnknownBase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AddressStableRequiredExprKind {
    PureArithmetic,
    HasLoad,
    HasCall,
    UnknownAddressExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AddressStableRequiredProof {
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(super) downstream_opcode: Option<PcodeOpcode>,
    pub(super) same_block_use_count: usize,
    pub(super) rhs_has_load: bool,
    pub(super) rhs_has_call: bool,
    pub(super) address_base_kind: AddressStableRequiredBaseKind,
    pub(super) address_expr_kind: AddressStableRequiredExprKind,
    pub(super) has_intervening_store: bool,
    pub(super) has_intervening_call: bool,
    pub(super) reason: AddressStableRequiredFamily,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StackAddressStabilityReason {
    StackAddrSingleUse,
    StackAddrMultipleUse,
    StackAddrEscapes,
    StackAddrFrameStable,
    StackAddrRspMutatedBeforeUse,
    StackAddrCrossesCall,
    StackAddrCrossesStore,
    StackAddrUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StackAddressBaseReg {
    Rsp,
    Rbp,
    Esp,
    Ebp,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct StackAddressStabilityProof {
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) downstream_opcode: Option<PcodeOpcode>,
    pub(super) base_reg: StackAddressBaseReg,
    pub(super) offset: Option<i64>,
    pub(super) same_block_use_count: usize,
    pub(super) crosses_call: bool,
    pub(super) crosses_store: bool,
    pub(super) rsp_redefined_before_use: bool,
    pub(super) frame_relative_candidate: bool,
    pub(super) reason: StackAddressStabilityReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum StackAddrFrameStableTrialReason {
    StackAddrFrameStableReplaced,
    RejectedNonFrameStable,
    RejectedMultipleUse,
    RejectedEscapes,
    RejectedBaseMutation,
    RejectedCrossesCallOrStore,
    RejectedConsumerKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DominatingPriorDefProofResult {
    PriorDefStableToMerge,
    PriorDefRedefinedBeforeMerge,
    PriorDefDoesNotDominateMerge,
    PriorDefPathSensitive,
    PriorDefCrossesCallOrStore,
    PriorDefUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DominatingPriorDefIncomingProof {
    pub(super) merge_block: u64,
    pub(super) pred_block: u64,
    pub(super) prior_def_block: u64,
    pub(super) prior_def_op_seq: u32,
    pub(super) prior_def_rhs: String,
    pub(super) prior_def_dominates_pred: bool,
    pub(super) prior_def_dominates_merge: bool,
    pub(super) redefined_between_prior_and_merge: bool,
    pub(super) redefined_on_pred_path: bool,
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(super) proof_result: DominatingPriorDefProofResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UnknownMissingMergeAttributionReason {
    EntryBlockAttribution,
    SyntheticRootBlock,
    MissingCfgPredecessors,
    SelfMergeAtFunctionEntry,
    StoreValueRepresentative,
    OtherDataRepresentative,
    UnknownAttribution,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UnknownMissingMergeAttributionProof {
    pub(super) merge_block: u64,
    pub(super) function_entry_block: u64,
    pub(super) merge_block_is_entry: bool,
    pub(super) predecessor_count: usize,
    pub(super) successor_count: usize,
    pub(super) incoming_value_count: usize,
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(super) reason: UnknownMissingMergeAttributionReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SyntheticRootMergeAttributionReason {
    EntryBlockAsMergeFallback,
    NoNearestJoinFound,
    ForwardJoinExistsButNotSelected,
    RootRepresentativeOnly,
    StoreValueAtRoot,
    OtherDataAtRoot,
    UnknownRootAttribution,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SyntheticRootMergeAttributionProof {
    pub(super) event_block: u64,
    pub(super) entry_block: u64,
    pub(super) selected_merge_block: u64,
    pub(super) selected_is_entry: bool,
    pub(super) event_block_is_entry: bool,
    pub(super) event_block_dominates: bool,
    pub(super) nearest_join_block: Option<u64>,
    pub(super) nearest_join_distance: Option<usize>,
    pub(super) nearest_postdom_join: Option<u64>,
    pub(super) postdom_distance: Option<usize>,
    pub(super) block_successor_count: usize,
    pub(super) entry_successor_count: usize,
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(super) reason: SyntheticRootMergeAttributionReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ForwardJoinNotSelectedRejectedReason {
    JoinDoesNotPostdominate,
    JoinHasMultipleAmbiguousPreds,
    JoinCrossesLoopBoundary,
    JoinCrossesSwitchBoundary,
    JoinNotReachableFromEvent,
    JoinDominanceUnknown,
    JoinRejectedByCurrentSelectionPolicy,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ForwardJoinNotSelectedProof {
    pub(super) event_block: u64,
    pub(super) selected_merge_block: u64,
    pub(super) forward_join_block: u64,
    pub(super) forward_join_distance: Option<usize>,
    pub(super) forward_join_predecessor_count: usize,
    pub(super) forward_join_successor_count: usize,
    pub(super) event_reaches_forward_join: bool,
    pub(super) forward_join_postdominates_event: bool,
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(super) rejected_reason: ForwardJoinNotSelectedRejectedReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AmbiguousJoinPredReason {
    AllIncomingSame,
    MissingIncomingForSomePred,
    ConflictingIncomingValues,
    EventPredOnlyValue,
    StoreValueAmbiguous,
    OtherDataAmbiguous,
    UnknownAmbiguousJoin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AmbiguousJoinPredProof {
    pub(super) event_block: u64,
    pub(super) forward_join_block: u64,
    pub(super) predecessor_blocks: Vec<u64>,
    pub(super) incoming_value_count: usize,
    pub(super) incoming_values: Vec<String>,
    pub(super) event_pred_index: Option<usize>,
    pub(super) event_pred_value: Option<String>,
    pub(super) values_same_across_preds: bool,
    pub(super) has_missing_incoming: bool,
    pub(super) has_conflicting_incoming: bool,
    pub(super) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(super) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(super) reason: AmbiguousJoinPredReason,
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
    pub(super) join_merge_missing_reason: BTreeMap<String, usize>,
    pub(super) merge_binding_candidate_result: BTreeMap<String, usize>,
    pub(super) merge_binding_candidate_incoming_kind: BTreeMap<String, usize>,
    pub(super) explicit_merge_binding_trial_reason: BTreeMap<String, usize>,
    pub(super) missing_incoming_pred_kind: BTreeMap<String, usize>,
    pub(super) missing_incoming_semantics_result: BTreeMap<String, usize>,
    pub(super) missing_incoming_missing_pred_kind: BTreeMap<String, usize>,
    pub(super) missing_no_prior_def_reason: BTreeMap<String, usize>,
    pub(super) temp_only_representative_reason: BTreeMap<String, usize>,
    pub(super) stable_representative_owner_reason: BTreeMap<String, usize>,
    pub(super) stable_representative_consumer_kind: BTreeMap<String, usize>,
    pub(super) stable_representative_downstream_opcode: BTreeMap<String, usize>,
    pub(super) alias_stable_required_family: BTreeMap<String, usize>,
    pub(super) alias_stable_required_consumer_kind: BTreeMap<String, usize>,
    pub(super) alias_stable_required_downstream_opcode: BTreeMap<String, usize>,
    pub(super) address_stable_required_family: BTreeMap<String, usize>,
    pub(super) address_stable_required_base_kind: BTreeMap<String, usize>,
    pub(super) address_stable_required_expr_kind: BTreeMap<String, usize>,
    pub(super) stack_address_stability_reason: BTreeMap<String, usize>,
    pub(super) stack_address_base_reg: BTreeMap<String, usize>,
    pub(super) stack_address_frame_relative_candidate: BTreeMap<String, usize>,
    pub(super) stack_address_frame_stable_trial_reason: BTreeMap<String, usize>,
    pub(super) dominating_prior_def_proof_result: BTreeMap<String, usize>,
    pub(super) unknown_missing_merge_attribution_reason: BTreeMap<String, usize>,
    pub(super) unknown_missing_merge_consumer_kind: BTreeMap<String, usize>,
    pub(super) unknown_missing_merge_rhs_kind: BTreeMap<String, usize>,
    pub(super) synthetic_root_merge_attribution_reason: BTreeMap<String, usize>,
    pub(super) forward_join_not_selected_rejected_reason: BTreeMap<String, usize>,
    pub(super) ambiguous_join_pred_reason: BTreeMap<String, usize>,
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
    SuppressAlways,
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
    Select,
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
