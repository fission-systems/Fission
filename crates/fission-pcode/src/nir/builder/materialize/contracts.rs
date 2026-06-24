use super::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplacementReadClass {
    SameBlockData,
    PredicateSensitive,
    SelectorSensitive,
    ReturnPath,
    Merge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MaterializationRejectionReason {
    AliasUnsafe,
    MissingMergeBinding,
    RepresentativeRootAttribution,
    TempOnlyRepresentativeLifecycle,
    DeadTempRepresentative,
    ConsumerRequiresStableRepresentative,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AliasUnsafeHazardKind {
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
pub(crate) enum DisallowedSingleConsumerConsumerKind {
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
pub(crate) enum DisallowedSingleConsumerRhsKind {
    VarOrConst,
    UnaryBoolean,
    BinaryBoolean,
    Arithmetic,
    LoadLike,
    CallLike,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DisallowedSingleConsumerReason {
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
pub(crate) struct DisallowedSingleConsumerProof {
    pub(crate) consumer_block_addr: u64,
    pub(crate) consumer_op_seq: u32,
    pub(crate) consumer_opcode: PcodeOpcode,
    pub(crate) matched_input_indices: Vec<usize>,
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(crate) rhs_low_cost: bool,
    pub(crate) rhs_has_load: bool,
    pub(crate) rhs_has_call: bool,
    pub(crate) reason: DisallowedSingleConsumerReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SingleConsumerCallRhsFamily {
    KnownPureIntrinsic,
    PreviewCalleeAnalysisUnsafe,
    UnknownInternalCall,
    ImportCall,
    CallOther,
    IndirectCall,
    UnknownCall,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SingleConsumerCallRhsProof {
    pub(crate) consumer_block_addr: u64,
    pub(crate) consumer_op_seq: u32,
    pub(crate) consumer_opcode: PcodeOpcode,
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) call_target: String,
    pub(crate) family: SingleConsumerCallRhsFamily,
    pub(crate) rhs_low_cost: bool,
    pub(crate) call_effect_source: Option<CallEffectSummarySource>,
    pub(crate) writes_memory: Option<bool>,
    pub(crate) may_call_unknown: Option<bool>,
    pub(crate) may_exit: Option<bool>,
    pub(crate) return_used: bool,
    pub(crate) downstream_opcode: Option<PcodeOpcode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CarryIntrinsicPredicateUseFamily {
    CarryFeedsBoolOr,
    CarryFeedsCompareZero,
    CarryFeedsCompareNonZero,
    CarryFeedsArithmetic,
    CarryFeedsUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoolOrDownstreamUseFamily {
    BoolOrFeedsPredicate,
    BoolOrFeedsBranch,
    BoolOrFeedsCompare,
    BoolOrFeedsData,
    UnknownBoolOrUse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CarryIntrinsicFinalPredicateContext {
    BoolOrOnly,
    CompareZero,
    CompareNonZero,
    BranchPredicate,
    PredicateChain,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CarryIntrinsicPredicateProof {
    pub(crate) call_target: String,
    pub(crate) args: Vec<String>,
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) downstream_opcode: PcodeOpcode,
    pub(crate) bool_chain_role: CarryIntrinsicPredicateUseFamily,
    pub(crate) rhs_low_cost: bool,
    pub(crate) args_side_effect_free: bool,
    pub(crate) final_predicate_context: CarryIntrinsicFinalPredicateContext,
    pub(crate) boolor_downstream_use: Option<BoolOrDownstreamUseFamily>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntrinsicCompareOnlyFamily {
    BorrowCompareZero,
    CarryCompareZero,
    SignedCarryCompareZero,
    PopCountCompareZero,
    UnknownIntrinsicCompare,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntrinsicCompareFinalPredicateContext {
    CompareZero,
    CompareOne,
    CompareNonZero,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IntrinsicCompareOnlyProof {
    pub(crate) call_target: String,
    pub(crate) args: Vec<String>,
    pub(crate) downstream_opcode: PcodeOpcode,
    pub(crate) compare_const: Option<i64>,
    pub(crate) family: IntrinsicCompareOnlyFamily,
    pub(crate) rhs_low_cost: bool,
    pub(crate) args_side_effect_free: bool,
    pub(crate) final_predicate_context: IntrinsicCompareFinalPredicateContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SingleConsumerLoadRhsFamily {
    LoadFeedsPredicate,
    LoadFeedsArithmetic,
    LoadFeedsAddressComputation,
    LoadFeedsStoreOrCall,
    LoadFeedsUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SingleConsumerLoadAliasClass {
    ReadOnlyLocalLoad,
    MayAliasSameBlockStore,
    MayAliasCall,
    VolatileOrUnknownLoad,
    GlobalOrExternalLoad,
    UnknownLoad,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SingleConsumerLoadRhsProof {
    pub(crate) consumer_block_addr: u64,
    pub(crate) consumer_op_seq: u32,
    pub(crate) consumer_opcode: PcodeOpcode,
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) load_ptr: String,
    pub(crate) family: SingleConsumerLoadRhsFamily,
    pub(crate) alias_class: SingleConsumerLoadAliasClass,
    pub(crate) same_block_store_before: bool,
    pub(crate) same_block_store_after: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MissingMergeBindingRelation {
    JoinMergeMissing,
    LoopHeaderMergeMissing,
    BackedgeMergeMissing,
    PredicateMergeMissing,
    PhiLikeMergeMissing,
    RepresentativeOnlyMissing,
    UnknownMissingMerge,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MissingMergeBindingProof {
    pub(crate) merge_block: u64,
    pub(crate) predecessor_count: usize,
    pub(crate) incoming_value_count: usize,
    pub(crate) has_existing_binding: bool,
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(crate) relation: MissingMergeBindingRelation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JoinMergeMissingReason {
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
pub(crate) struct JoinMergeMissingProof {
    pub(crate) event_block: u64,
    pub(crate) merge_block: u64,
    pub(crate) predecessor_blocks: Vec<u64>,
    pub(crate) incoming_value_count: usize,
    pub(crate) incoming_values: Vec<String>,
    pub(crate) values_same_across_preds: bool,
    pub(crate) has_missing_incoming: bool,
    pub(crate) has_conflicting_incoming: bool,
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(crate) reason: JoinMergeMissingReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum MergeBindingCandidateIncomingKind {
    VarOrConst,
    Predicate,
    Arithmetic,
    LoadLike,
    CallLike,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MergeBindingCandidateResult {
    MissingIncomingSemanticsRequired,
    PhiLikeBindingCandidate,
    IncomingKindsUnsafe,
    InsufficientConflictingIncoming,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MergeBindingCandidateProof {
    pub(crate) merge_block: u64,
    pub(crate) predecessor_count: usize,
    pub(crate) missing_incoming_count: usize,
    pub(crate) conflicting_incoming_count: usize,
    pub(crate) incoming_value_kinds: Vec<MergeBindingCandidateIncomingKind>,
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(crate) can_synthesize_phi_like_binding: bool,
    pub(crate) result: MergeBindingCandidateResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MissingIncomingSemanticsResult {
    DeadOnlyMissing,
    EntryDefaultRequired,
    PathSensitiveMissing,
    TempOnlyLeakage,
    UnsafePriorDefReuse,
    NoSafeSemantics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MissingIncomingSemanticsProof {
    pub(crate) merge_block: u64,
    pub(crate) predecessor_count: usize,
    pub(crate) missing_pred_count: usize,
    pub(crate) defined_pred_count: usize,
    pub(crate) defined_incoming_values: Vec<String>,
    pub(crate) missing_pred_kinds: Vec<String>,
    pub(crate) missing_pred_has_prior_def: bool,
    pub(crate) missing_pred_prior_def_status: String,
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(crate) candidate_semantics: String,
    pub(crate) result: MissingIncomingSemanticsResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExplicitMergeBindingTrialReason {
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
pub(crate) enum MissingIncomingPredKind {
    MissingBecauseNoPriorDef,
    MissingBecausePriorDefDominates,
    MissingBecauseDeadPred,
    MissingBecauseEntryDefault,
    MissingBecauseLoopBackedge,
    MissingBecausePathSensitive,
    UnknownMissingIncoming,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MissingIncomingPredProof {
    pub(crate) event_block: u64,
    pub(crate) merge_block: u64,
    pub(crate) pred_block: u64,
    pub(crate) pred_reaches_merge: bool,
    pub(crate) pred_has_definition: bool,
    pub(crate) pred_has_prior_definition: bool,
    pub(crate) prior_def_block: Option<u64>,
    pub(crate) prior_def_op_seq: Option<u32>,
    pub(crate) incoming_kind: MissingIncomingPredKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MissingNoPriorDefReason {
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
pub(crate) struct MissingNoPriorDefProof {
    pub(crate) merge_block: u64,
    pub(crate) pred_block: u64,
    pub(crate) pred_reaches_merge: bool,
    pub(crate) pred_is_entry: bool,
    pub(crate) pred_is_dead: bool,
    pub(crate) output_space: u64,
    pub(crate) output_size: u32,
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(crate) default_candidate: String,
    pub(crate) reason: MissingNoPriorDefReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TempOnlyRepresentativeReason {
    TempRepresentativeResidue,
    RootAttributedTemp,
    MergeCrossingTemp,
    DeadTempRepresentative,
    StoreValueTemp,
    OtherDataTemp,
    UnknownTempRepresentative,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TempOnlyRepresentativeProof {
    pub(crate) merge_block: u64,
    pub(crate) pred_block: Option<u64>,
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(crate) defining_event: String,
    pub(crate) materialization_event: String,
    pub(crate) has_real_storage: bool,
    pub(crate) has_later_use: bool,
    pub(crate) crosses_merge: bool,
    pub(crate) root_attributed: bool,
    pub(crate) reason: TempOnlyRepresentativeReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StableRepresentativeOwnerReason {
    RootRepresentativeStableRequired,
    TempLifecycleStableRequired,
    RealMergeStableRequired,
    PredicateStableRequired,
    StoreValueStableRequired,
    AliasStableRequired,
    UnknownStableRepresentative,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StableRepresentativeOwnerProof {
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(crate) overlaps_representative_root_attribution: bool,
    pub(crate) overlaps_temp_only_lifecycle: bool,
    pub(crate) overlaps_real_missing_merge: bool,
    pub(crate) downstream_opcode: Option<PcodeOpcode>,
    pub(crate) reason: StableRepresentativeOwnerReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AliasStableRequiredFamily {
    LoadAddrStableRequired,
    StoreAddrStableRequired,
    OtherDataLoadLikeStable,
    OtherDataCopyStable,
    BranchIndStableRequired,
    ArithmeticStableRequired,
    UnknownAliasStable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AliasStableRequiredProof {
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(crate) downstream_opcode: Option<PcodeOpcode>,
    pub(crate) same_block_use_count: usize,
    pub(crate) rhs_has_load: bool,
    pub(crate) rhs_has_call: bool,
    pub(crate) requires_preserved_expr: bool,
    pub(crate) reason: AliasStableRequiredFamily,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AddressStableRequiredFamily {
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
pub(crate) enum AddressStableRequiredBaseKind {
    StackRelative,
    GlobalRelative,
    RegisterBase,
    UnknownBase,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AddressStableRequiredExprKind {
    PureArithmetic,
    HasLoad,
    HasCall,
    UnknownAddressExpr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AddressStableRequiredProof {
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(crate) downstream_opcode: Option<PcodeOpcode>,
    pub(crate) same_block_use_count: usize,
    pub(crate) rhs_has_load: bool,
    pub(crate) rhs_has_call: bool,
    pub(crate) address_base_kind: AddressStableRequiredBaseKind,
    pub(crate) address_expr_kind: AddressStableRequiredExprKind,
    pub(crate) has_intervening_store: bool,
    pub(crate) has_intervening_call: bool,
    pub(crate) reason: AddressStableRequiredFamily,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StackAddressStabilityReason {
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
pub(crate) enum StackAddressBaseReg {
    Rsp,
    Rbp,
    Esp,
    Ebp,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StackAddressStabilityProof {
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) downstream_opcode: Option<PcodeOpcode>,
    pub(crate) base_reg: StackAddressBaseReg,
    pub(crate) offset: Option<i64>,
    pub(crate) same_block_use_count: usize,
    pub(crate) crosses_call: bool,
    pub(crate) crosses_store: bool,
    pub(crate) rsp_redefined_before_use: bool,
    pub(crate) frame_relative_candidate: bool,
    pub(crate) reason: StackAddressStabilityReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StackAddrFrameStableTrialReason {
    StackAddrFrameStableReplaced,
    RejectedNonFrameStable,
    RejectedMultipleUse,
    RejectedEscapes,
    RejectedBaseMutation,
    RejectedCrossesCallOrStore,
    RejectedConsumerKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DominatingPriorDefProofResult {
    PriorDefStableToMerge,
    PriorDefRedefinedBeforeMerge,
    PriorDefDoesNotDominateMerge,
    PriorDefPathSensitive,
    PriorDefCrossesCallOrStore,
    PriorDefUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DominatingPriorDefIncomingProof {
    pub(crate) merge_block: u64,
    pub(crate) pred_block: u64,
    pub(crate) prior_def_block: u64,
    pub(crate) prior_def_op_seq: u32,
    pub(crate) prior_def_rhs: String,
    pub(crate) prior_def_dominates_pred: bool,
    pub(crate) prior_def_dominates_merge: bool,
    pub(crate) redefined_between_prior_and_merge: bool,
    pub(crate) redefined_on_pred_path: bool,
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(crate) proof_result: DominatingPriorDefProofResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnknownMissingMergeAttributionReason {
    EntryBlockAttribution,
    SyntheticRootBlock,
    MissingCfgPredecessors,
    SelfMergeAtFunctionEntry,
    StoreValueRepresentative,
    OtherDataRepresentative,
    UnknownAttribution,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnknownMissingMergeAttributionProof {
    pub(crate) merge_block: u64,
    pub(crate) function_entry_block: u64,
    pub(crate) merge_block_is_entry: bool,
    pub(crate) predecessor_count: usize,
    pub(crate) successor_count: usize,
    pub(crate) incoming_value_count: usize,
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(crate) reason: UnknownMissingMergeAttributionReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyntheticRootMergeAttributionReason {
    EntryBlockAsMergeFallback,
    NoNearestJoinFound,
    ForwardJoinExistsButNotSelected,
    RootRepresentativeOnly,
    StoreValueAtRoot,
    OtherDataAtRoot,
    UnknownRootAttribution,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyntheticRootMergeAttributionProof {
    pub(crate) event_block: u64,
    pub(crate) entry_block: u64,
    pub(crate) selected_merge_block: u64,
    pub(crate) selected_is_entry: bool,
    pub(crate) event_block_is_entry: bool,
    pub(crate) event_block_dominates: bool,
    pub(crate) nearest_join_block: Option<u64>,
    pub(crate) nearest_join_distance: Option<usize>,
    pub(crate) nearest_postdom_join: Option<u64>,
    pub(crate) postdom_distance: Option<usize>,
    pub(crate) block_successor_count: usize,
    pub(crate) entry_successor_count: usize,
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(crate) reason: SyntheticRootMergeAttributionReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForwardJoinNotSelectedRejectedReason {
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
pub(crate) struct ForwardJoinNotSelectedProof {
    pub(crate) event_block: u64,
    pub(crate) selected_merge_block: u64,
    pub(crate) forward_join_block: u64,
    pub(crate) forward_join_distance: Option<usize>,
    pub(crate) forward_join_predecessor_count: usize,
    pub(crate) forward_join_successor_count: usize,
    pub(crate) event_reaches_forward_join: bool,
    pub(crate) forward_join_postdominates_event: bool,
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(crate) rejected_reason: ForwardJoinNotSelectedRejectedReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AmbiguousJoinPredReason {
    AllIncomingSame,
    MissingIncomingForSomePred,
    ConflictingIncomingValues,
    EventPredOnlyValue,
    StoreValueAmbiguous,
    OtherDataAmbiguous,
    UnknownAmbiguousJoin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AmbiguousJoinPredProof {
    pub(crate) event_block: u64,
    pub(crate) forward_join_block: u64,
    pub(crate) predecessor_blocks: Vec<u64>,
    pub(crate) incoming_value_count: usize,
    pub(crate) incoming_values: Vec<String>,
    pub(crate) event_pred_index: Option<usize>,
    pub(crate) event_pred_value: Option<String>,
    pub(crate) values_same_across_preds: bool,
    pub(crate) has_missing_incoming: bool,
    pub(crate) has_conflicting_incoming: bool,
    pub(crate) consumer_kind: DisallowedSingleConsumerConsumerKind,
    pub(crate) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(crate) reason: AmbiguousJoinPredReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnknownConsumerKindReason {
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
pub(crate) struct UnknownConsumerKindProof {
    pub(crate) consumer_block_addr: u64,
    pub(crate) consumer_op_seq: u32,
    pub(crate) consumer_opcode: PcodeOpcode,
    pub(crate) matched_input_indices: Vec<usize>,
    pub(crate) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(crate) reason: UnknownConsumerKindReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PopCountResultUseFamily {
    PopCountFeedsPredicate,
    PopCountFeedsArithmetic,
    PopCountFeedsCompareZero,
    PopCountFeedsCompareConst,
    PopCountFeedsStoreOrCall,
    PopCountResultUnused,
    UnknownPopCountUse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PopCountConsumerProof {
    pub(crate) consumer_op_seq: u32,
    pub(crate) input_width: u32,
    pub(crate) output_width: Option<u32>,
    pub(crate) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(crate) rhs_low_cost: bool,
    pub(crate) rhs_has_call: bool,
    pub(crate) rhs_has_load: bool,
    pub(crate) popcount_result_used_by: PopCountResultUseFamily,
    pub(crate) downstream_consumer_opcode: Option<PcodeOpcode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PopCountIntAndMaskKind {
    AndOne,
    AndByteMask,
    AndPowerOfTwoMinusOne,
    AndNonPowerOfTwoMask,
    UnknownMask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PopCountIntAndDownstreamUseFamily {
    FeedsPredicate,
    FeedsCompareZero,
    FeedsCompareConst,
    FeedsArithmetic,
    FeedsStoreOrCall,
    FeedsUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PopCountIntAndChainProof {
    pub(crate) popcount_consumer_op_seq: u32,
    pub(crate) intand_op_seq: u32,
    pub(crate) popcount_result: String,
    pub(crate) intand_mask: Option<u64>,
    pub(crate) intand_mask_kind: PopCountIntAndMaskKind,
    pub(crate) intand_result_consumer: PopCountIntAndDownstreamUseFamily,
    pub(crate) downstream_consumer_opcode: Option<PcodeOpcode>,
    pub(crate) chain_low_cost: bool,
    pub(crate) chain_side_effect_free: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParityChainRole {
    PopCountInput,
    PopCountResult,
    IntAndResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ParityChainKeepReason {
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
pub(crate) enum ParityChainConsumerContext {
    CompareZero,
    CompareNonZero,
    CompareOne,
    CompareNotOne,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParityChainProof {
    pub(crate) role: ParityChainRole,
    pub(crate) popcount_op_seq: u32,
    pub(crate) intand_op_seq: u32,
    pub(crate) compare_op_seq: u32,
    pub(crate) compare_opcode: PcodeOpcode,
    pub(crate) compare_const: u64,
    pub(crate) chain_low_cost: bool,
    pub(crate) chain_side_effect_free: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SingleConsumerPredicateFamily {
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
pub(crate) struct SingleConsumerPredicateProof {
    pub(crate) consumer_block_addr: u64,
    pub(crate) consumer_op_seq: u32,
    pub(crate) consumer_opcode: PcodeOpcode,
    pub(crate) rhs_kind: DisallowedSingleConsumerRhsKind,
    pub(crate) predicate_family: SingleConsumerPredicateFamily,
    pub(crate) guard_family: SingleConsumerPredicateFamily,
    pub(crate) same_guard_as_consumer: bool,
    pub(crate) requires_stable_representative: bool,
    pub(crate) low_cost_if_predicate: bool,
    pub(crate) has_call: bool,
    pub(crate) has_load: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArithmeticPredicateShape {
    LowBitAndOne,
    PowerOfTwoMask,
    NonPowerOfTwoMask,
    ShiftAndMask,
    UnknownArithmetic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArithmeticPredicateStableReason {
    PredicateSensitive,
    ArithmeticMask,
    ConsumerCompare,
    NonCanonicalPredicate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArithmeticPredicateProof {
    pub(crate) consumer_guard: SingleConsumerPredicateFamily,
    pub(crate) mask_kind: ArithmeticPredicateShape,
    pub(crate) mask_value: Option<u64>,
    pub(crate) boolean_width: bool,
    pub(crate) low_cost: bool,
    pub(crate) stable_required: bool,
    pub(crate) stable_required_reason: Option<ArithmeticPredicateStableReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LowBitMaskPredicateFamily {
    BooleanFlagMask,
    IntegerBitTest,
    MaskFromCompareResult,
    MaskFromArithmeticValue,
    UnknownLowBitMask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LowBitMaskInputOriginKind {
    Compare,
    BoolOp,
    Arithmetic,
    Load,
    Call,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LowBitMaskPredicateProof {
    pub(crate) family: LowBitMaskPredicateFamily,
    pub(crate) mask_input: String,
    pub(crate) consumer_guard: SingleConsumerPredicateFamily,
    pub(crate) feeds_only_predicate: bool,
    pub(crate) input_is_boolean_like: bool,
    pub(crate) input_origin_kind: LowBitMaskInputOriginKind,
    pub(crate) stable_required_reason: Option<ArithmeticPredicateStableReason>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MalformedDefUseWindowRelation {
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
pub(crate) struct MalformedDefUseWindowDetail {
    pub(crate) relation: MalformedDefUseWindowRelation,
    pub(crate) def_op_idx: usize,
    pub(crate) terminator_idx: Option<usize>,
    pub(crate) consumer_count: usize,
    pub(crate) first_consumer_block: Option<u64>,
    pub(crate) first_consumer_idx: Option<usize>,
    pub(crate) first_consumer_op_seq: Option<u32>,
    pub(crate) rhs_kind: NoConsumerSuppressionRhsKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CrossBlockConsumerRelation {
    SuccessorBlock,
    JoinBlock,
    LoopBackedge,
    PostDominatorBlock,
    UnreachableOrUnclassified,
    MergePhiConsumer,
    OrdinaryDataConsumer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CrossBlockConsumerProvenance {
    pub(crate) relation: CrossBlockConsumerRelation,
    pub(crate) consumer_opcode: Option<PcodeOpcode>,
    pub(crate) consumer_is_multiequal: bool,
    pub(crate) immediate_successor: bool,
    pub(crate) consumer_is_join: bool,
    pub(crate) redefined_before_consumer: bool,
    pub(crate) def_successor_count: usize,
    pub(crate) consumer_predecessor_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CrossBlockReplacementProof {
    pub(crate) relation: CrossBlockConsumerRelation,
    pub(crate) dominates_consumer: bool,
    pub(crate) rhs_low_cost: bool,
    pub(crate) preserve_materialization: bool,
    pub(crate) no_redefinition_before_consumer: bool,
    pub(crate) merge_phi: bool,
    pub(crate) def_successor_count: usize,
    pub(crate) consumer_predecessor_count: usize,
    pub(crate) narrow_candidate: bool,
    pub(crate) consumer_opcode: Option<PcodeOpcode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CrossBlockRedefinitionRelation {
    RedefinedInDefBlockAfterDef,
    RedefinedOnEdge,
    RedefinedInConsumerBlockBeforeUse,
    RedefinedInSiblingPredecessor,
    PhiRedefinition,
    LoopCarriedRedefinition,
    UnknownRedefinition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CrossBlockRedefinitionDetail {
    pub(crate) relation: CrossBlockRedefinitionRelation,
    pub(crate) redef_block_addr: u64,
    pub(crate) redef_op_idx: usize,
    pub(crate) redef_op_seq: u32,
    pub(crate) redef_opcode: PcodeOpcode,
    pub(crate) redef_rhs_kind: SameBlockOverwriteRhsKind,
    pub(crate) overwrite_shape: SameBlockOverwriteShapeKind,
    pub(crate) def_to_redef_gap: usize,
    pub(crate) redef_to_terminator_gap: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CopyOverwriteRestartProof {
    pub(crate) consumer_relation: CrossBlockConsumerRelation,
    pub(crate) redef_op_seq: u32,
    pub(crate) redef_rhs: String,
    pub(crate) same_value: bool,
    pub(crate) redef_dominates_consumer: bool,
    pub(crate) old_def_has_pre_redef_use: bool,
    pub(crate) consumer_block_addr: u64,
    pub(crate) consumer_op_seq: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PredicateOverwriteRefreshProof {
    pub(crate) consumer_relation: CrossBlockConsumerRelation,
    pub(crate) redef_op_seq: u32,
    pub(crate) redef_rhs: String,
    pub(crate) predicate_consumer_block_addr: u64,
    pub(crate) predicate_consumer_op_seq: u32,
    pub(crate) predicate_rhs: String,
    pub(crate) same_guard_family: bool,
    pub(crate) old_def_has_pre_redef_use: bool,
    pub(crate) redef_dominates_predicate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoopCarriedValueKind {
    BooleanFlag,
    CounterIncrement,
    PointerAdvance,
    Accumulator,
    UnknownLoopCarried,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoopBooleanGuardFamily {
    DirectFlag,
    NegatedFlag,
    EqZero,
    NeZero,
    NonPredicate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoopBoundaryBindingFamily {
    BoolNegate,
    IntNotEqual,
    OtherBooleanFlag,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoopCarriedOverwriteProvenance {
    pub(crate) loop_header: u64,
    pub(crate) backedge_block: u64,
    pub(crate) consumer_block: u64,
    pub(crate) consumer_op_seq: u32,
    pub(crate) redef_op_seq: u32,
    pub(crate) redef_rhs: String,
    pub(crate) has_multiequal: bool,
    pub(crate) phi_input_count: usize,
    pub(crate) induction_like: bool,
    pub(crate) carried_value_kind: LoopCarriedValueKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoopBooleanFlagProof {
    pub(crate) consumer_opcode: PcodeOpcode,
    pub(crate) exit_edge: Option<u64>,
    pub(crate) backedge_edge: Option<u64>,
    pub(crate) guard_family: LoopBooleanGuardFamily,
    pub(crate) same_guard_as_exit: bool,
    pub(crate) old_def_has_pre_redef_use: bool,
    pub(crate) redef_dominates_backedge: bool,
    pub(crate) consumer_is_loop_header_predicate: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LoopGuardRefreshDominanceReason {
    ProvedBySingleBackedge,
    RedefAfterBackedgeBranch,
    RedefInNonBackedgeBlock,
    MultipleBackedgeBlocks,
    HeaderPredicateUsesIntermediate,
    MissingBackedgeTerminator,
    UnknownDominance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoopGuardRefreshDominanceProof {
    pub(crate) redef_block: u64,
    pub(crate) backedge_block: u64,
    pub(crate) redef_before_backedge_branch: bool,
    pub(crate) all_backedge_paths_covered: bool,
    pub(crate) header_predicate_uses_redef: bool,
    pub(crate) reason: LoopGuardRefreshDominanceReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoopBoundaryBindingCorrelation {
    pub(crate) loop_header: u64,
    pub(crate) family: LoopBoundaryBindingFamily,
    pub(crate) missing_merge_binding: bool,
    pub(crate) stable_representative_required: bool,
    pub(crate) merge_block: Option<u64>,
    pub(crate) candidate_binding: String,
    pub(crate) existing_binding: Option<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(in crate::nir::builder) struct MaterializeOwnerRepartition {
    pub(crate) alias_unsafe_hazard_kind: BTreeMap<String, usize>,
    pub(crate) disallowed_single_consumer_reason: BTreeMap<String, usize>,
    pub(crate) disallowed_single_consumer_consumer_kind: BTreeMap<String, usize>,
    pub(crate) disallowed_single_consumer_rhs_kind: BTreeMap<String, usize>,
    pub(crate) single_consumer_call_rhs_family: BTreeMap<String, usize>,
    pub(crate) single_consumer_call_rhs_effect_source: BTreeMap<String, usize>,
    pub(crate) single_consumer_call_rhs_consumer_kind: BTreeMap<String, usize>,
    pub(crate) single_consumer_call_rhs_downstream_opcode: BTreeMap<String, usize>,
    pub(crate) carry_intrinsic_predicate_family: BTreeMap<String, usize>,
    pub(crate) carry_intrinsic_boolor_downstream_use: BTreeMap<String, usize>,
    pub(crate) carry_intrinsic_final_predicate_context: BTreeMap<String, usize>,
    pub(crate) intrinsic_compare_only_family: BTreeMap<String, usize>,
    pub(crate) intrinsic_compare_only_final_predicate_context: BTreeMap<String, usize>,
    pub(crate) single_consumer_load_rhs_family: BTreeMap<String, usize>,
    pub(crate) single_consumer_load_rhs_alias_class: BTreeMap<String, usize>,
    pub(crate) missing_merge_binding_relation: BTreeMap<String, usize>,
    pub(crate) join_merge_missing_reason: BTreeMap<String, usize>,
    pub(crate) merge_binding_candidate_result: BTreeMap<String, usize>,
    pub(crate) merge_binding_candidate_incoming_kind: BTreeMap<String, usize>,
    pub(crate) explicit_merge_binding_trial_reason: BTreeMap<String, usize>,
    pub(crate) missing_incoming_pred_kind: BTreeMap<String, usize>,
    pub(crate) missing_incoming_semantics_result: BTreeMap<String, usize>,
    pub(crate) missing_incoming_missing_pred_kind: BTreeMap<String, usize>,
    pub(crate) missing_no_prior_def_reason: BTreeMap<String, usize>,
    pub(crate) temp_only_representative_reason: BTreeMap<String, usize>,
    pub(crate) stable_representative_owner_reason: BTreeMap<String, usize>,
    pub(crate) stable_representative_consumer_kind: BTreeMap<String, usize>,
    pub(crate) stable_representative_downstream_opcode: BTreeMap<String, usize>,
    pub(crate) alias_stable_required_family: BTreeMap<String, usize>,
    pub(crate) alias_stable_required_consumer_kind: BTreeMap<String, usize>,
    pub(crate) alias_stable_required_downstream_opcode: BTreeMap<String, usize>,
    pub(crate) address_stable_required_family: BTreeMap<String, usize>,
    pub(crate) address_stable_required_base_kind: BTreeMap<String, usize>,
    pub(crate) address_stable_required_expr_kind: BTreeMap<String, usize>,
    pub(crate) stack_address_stability_reason: BTreeMap<String, usize>,
    pub(crate) stack_address_base_reg: BTreeMap<String, usize>,
    pub(crate) stack_address_frame_relative_candidate: BTreeMap<String, usize>,
    pub(crate) stack_address_frame_stable_trial_reason: BTreeMap<String, usize>,
    pub(crate) dominating_prior_def_proof_result: BTreeMap<String, usize>,
    pub(crate) unknown_missing_merge_attribution_reason: BTreeMap<String, usize>,
    pub(crate) unknown_missing_merge_consumer_kind: BTreeMap<String, usize>,
    pub(crate) unknown_missing_merge_rhs_kind: BTreeMap<String, usize>,
    pub(crate) synthetic_root_merge_attribution_reason: BTreeMap<String, usize>,
    pub(crate) forward_join_not_selected_rejected_reason: BTreeMap<String, usize>,
    pub(crate) ambiguous_join_pred_reason: BTreeMap<String, usize>,
    pub(crate) unknown_consumer_kind_reason: BTreeMap<String, usize>,
    pub(crate) unknown_consumer_kind_opcode: BTreeMap<String, usize>,
    pub(crate) popcount_consumer_result_use: BTreeMap<String, usize>,
    pub(crate) popcount_consumer_downstream_opcode: BTreeMap<String, usize>,
    pub(crate) popcount_intand_mask_kind: BTreeMap<String, usize>,
    pub(crate) popcount_intand_downstream_use: BTreeMap<String, usize>,
    pub(crate) parity_chain_regression_role: BTreeMap<String, usize>,
    pub(crate) parity_chain_regression_before_event: BTreeMap<String, usize>,
    pub(crate) parity_chain_regression_consumer_context: BTreeMap<String, usize>,
    pub(crate) single_consumer_predicate_family: BTreeMap<String, usize>,
    pub(crate) single_consumer_predicate_guard_family: BTreeMap<String, usize>,
    pub(crate) single_consumer_predicate_same_guard: BTreeMap<String, usize>,
    pub(crate) single_consumer_predicate_requires_stable: BTreeMap<String, usize>,
    pub(crate) arithmetic_predicate_shape: BTreeMap<String, usize>,
    pub(crate) arithmetic_predicate_consumer_guard: BTreeMap<String, usize>,
    pub(crate) arithmetic_predicate_boolean_width: BTreeMap<String, usize>,
    pub(crate) arithmetic_predicate_stable_reason: BTreeMap<String, usize>,
    pub(crate) low_bit_mask_predicate_family: BTreeMap<String, usize>,
    pub(crate) low_bit_mask_input_origin_kind: BTreeMap<String, usize>,
    pub(crate) low_bit_mask_feeds_only_predicate: BTreeMap<String, usize>,
    pub(crate) low_bit_mask_input_is_boolean_like: BTreeMap<String, usize>,
    pub(crate) materialization_rejection_reason: BTreeMap<String, usize>,
    pub(crate) malformed_def_use_window_relation: BTreeMap<String, usize>,
    pub(crate) cross_block_consumer_relation: BTreeMap<String, usize>,
    pub(crate) cross_block_redefinition_relation: BTreeMap<String, usize>,
    pub(crate) same_block_overwrite_shape_kind: BTreeMap<String, usize>,
    pub(crate) loop_carried_value_kind: BTreeMap<String, usize>,
    pub(crate) loop_boolean_guard_family: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SameBlockOverwriteShapeKind {
    OverwriteBeforeBranch,
    OverwriteAtPredicateProducer,
    OverwriteAtLoopUpdate,
    OverwriteAtCallResult,
    OverwriteAtLoadResult,
    OverwriteAtCopy,
    OverwriteUnknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SameBlockOverwriteRhsKind {
    CopyLike,
    Predicate,
    Arithmetic,
    Load,
    Call,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AliasUnsafeHazard {
    pub(crate) kind: AliasUnsafeHazardKind,
    pub(crate) use_stmt_idx: Option<usize>,
    pub(crate) hazard_stmt_idx: Option<usize>,
    pub(crate) hazard_opcode: Option<PcodeOpcode>,
}

impl AliasUnsafeHazard {
    pub(crate) fn new(
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
pub(crate) enum ReplacementCompleteness {
    Complete,
    Incomplete(MaterializationRejectionReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReplacementValuePlan {
    pub(crate) dominant_read: ReplacementReadClass,
    pub(crate) completeness: ReplacementCompleteness,
}

impl ReplacementValuePlan {
    pub(crate) fn complete(dominant_read: ReplacementReadClass) -> Self {
        Self {
            dominant_read,
            completeness: ReplacementCompleteness::Complete,
        }
    }

    pub(crate) fn incomplete(
        dominant_read: ReplacementReadClass,
        reason: MaterializationRejectionReason,
    ) -> Self {
        Self {
            dominant_read,
            completeness: ReplacementCompleteness::Incomplete(reason),
        }
    }

    pub(crate) fn is_complete(self) -> bool {
        matches!(self.completeness, ReplacementCompleteness::Complete)
    }

    pub(crate) fn rejection_reason(self) -> Option<MaterializationRejectionReason> {
        match self.completeness {
            ReplacementCompleteness::Complete => None,
            ReplacementCompleteness::Incomplete(reason) => Some(reason),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoConsumerMaterializationProfile {
    pub(crate) same_block_consumers: usize,
    pub(crate) cross_block_consumers: usize,
    pub(crate) has_later_block_use: bool,
    pub(crate) has_phi_merge_use: bool,
    pub(crate) has_debug_use: bool,
    pub(crate) rhs_side_effectful: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoConsumerMaterializationDecision {
    Suppress,
    SuppressAlways,
    Keep(NoConsumerMaterializationKeepReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoConsumerMaterializationKeepReason {
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
pub(crate) enum NoConsumerSuppressionRhsKind {
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
    FieldAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoConsumerSuppressionBlockPosition {
    Local,
    PreBranch,
    PredicateAdjacent,
    ReturnAdjacent,
    MergeAdjacent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoConsumerSuppressionOutputKind {
    TempOnly,
    RegisterVisible,
    MemoryDerived,
}
