use fission_loader::loader::LoadedBinary;
use indexmap::IndexMap;
use std::collections::HashMap;
use thiserror::Error;

use super::support::CallingConvention;
use crate::pcode::{PcodeFunction, PcodeOpcode};

pub type NirValueId = u32;
pub type StackSlotId = u32;

/// A single field in a recovered struct layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructField {
    /// Byte offset from the start of the aggregate.
    pub offset: u32,
    /// Inferred type of the field (best-effort; may be Unknown).
    pub ty: NirType,
    /// Generated or surface name (e.g. "field_8", or from win_types lookup).
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageClass {
    Param,
    StackLocal,
    Aggregate,
    GlobalLike,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapperClass {
    None,
    TailForwarder,
    ImportThunk,
    PureAdapter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcedureControlEffect {
    Returns,
    TailJumps,
    NonReturning,
    IndirectOnly,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcedureMemoryEffect {
    Pure,
    StackLocal,
    ReadsGlobal,
    WritesGlobal,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcedureStackEffect {
    Neutral,
    FrameSetupOnly,
    AffineDelta,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcedureReturnShape {
    None,
    ForwardedCallResult(CallTargetRef),
    ForwardedTailTarget(CallTargetRef),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcedureCallShape {
    Leaf,
    SingleTailWrapper,
    WrapperChain,
    General,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgForwardingRelation {
    pub forwarded_param_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapperContractionProof {
    pub wrapper_class: WrapperClass,
    pub target: CallTargetRef,
    pub arg_forwarding: ArgForwardingRelation,
    pub confidence: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcedureSummary {
    pub control_effect: ProcedureControlEffect,
    pub memory_effect: ProcedureMemoryEffect,
    pub stack_effect: ProcedureStackEffect,
    pub return_shape: ProcedureReturnShape,
    pub arg_forwarding: ArgForwardingRelation,
    pub call_shape: ProcedureCallShape,
    pub wrapper_contraction: Option<WrapperContractionProof>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SummarySoundness {
    Pessimistic,
    Optimistic,
}

pub type ObjectRootId = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactEvidenceSource {
    Partition,
    ExplicitType,
    ImportSignature,
    AbiCarrier,
    CallsiteRole,
    LoaderSymbol,
    StructuralInference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactEvidenceKind {
    ObjectRoot,
    TypedShape,
    SurfaceBinding,
    PrototypeSummary,
    CallEffect,
    IndirectTarget,
    DispatcherShape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactEvidence {
    pub source: FactEvidenceSource,
    pub confidence: u8,
    pub kind: FactEvidenceKind,
    pub subject: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectRegion {
    pub root: ObjectRootId,
    pub storage_class: StorageClass,
    pub escaped: bool,
    pub interval: (i64, i64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedObjectShape {
    pub fields: Vec<StructField>,
    pub array_runs: Vec<(u32, u32)>,
    pub opaque_ranges: Vec<(u32, u32)>,
    pub confidence: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectFact {
    pub root: ObjectRootId,
    pub storage_class: StorageClass,
    pub escaped: bool,
    pub interval_set: Vec<(u32, u32)>,
    pub type_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceBinding {
    pub object_id: ObjectRootId,
    pub role: StorageClass,
    pub preferred_name: String,
    pub preferred_type: NirType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceFact {
    pub binding: String,
    pub preferred_name: String,
    pub preferred_type: NirType,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TypedFactStore {
    pub evidences: Vec<FactEvidence>,
    pub object_facts: IndexMap<String, ObjectFact>,
    pub surface_facts: IndexMap<String, SurfaceFact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NirType {
    Unknown,
    Bool,
    Int {
        bits: u32,
        signed: bool,
    },
    Ptr(Box<NirType>),
    /// An opaque aggregate (struct/array-like) region.
    ///
    /// `size` is the total byte size.  `fields` is populated by
    /// `aggregate_fields.rs` after pointer-arithmetic recovery; it is empty
    /// until that pass runs.
    Aggregate {
        size: u32,
        fields: Vec<StructField>,
    },
    Float {
        bits: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirBinding {
    pub name: String,
    pub ty: NirType,
    pub surface_type_name: Option<String>,
    pub origin: Option<NirBindingOrigin>,
    pub initializer: Option<HirExpr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NirBindingOrigin {
    ParamIndex(usize),
    StackOffset(i64),
    HomeSlot(i64),
    OutgoingArgSlot(i64),
    VaRegion,
    ReturnScaffold,
    DerivedFromStackOffset(i64),
    Temp,
    TempPreserved,
}

impl NirBindingOrigin {
    pub fn is_temp_like(self) -> bool {
        matches!(self, Self::Temp | Self::TempPreserved)
    }

    pub fn preserves_materialization(self) -> bool {
        matches!(self, Self::TempPreserved)
    }
}

impl NirBinding {
    pub fn is_temp_like(&self) -> bool {
        self.origin.is_some_and(NirBindingOrigin::is_temp_like)
    }

    pub fn preserves_materialization(&self) -> bool {
        self.origin
            .is_some_and(NirBindingOrigin::preserves_materialization)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CarrierClass {
    Gpr,
    Fpr,
    Vec,
    StackArg,
    HomeSlot,
    LocalSlot,
    ReturnSlot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CallTargetProvenance {
    Direct,
    Import,
    Fact,
    Global,
    Intrinsic,
    Reference,
    IndirectCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CallEdgeKind {
    Direct,
    Import,
    Reference,
    IndirectCandidate,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CallTargetRef {
    pub address: Option<u64>,
    pub symbol: String,
    pub provenance: CallTargetProvenance,
    pub edge_kind: CallEdgeKind,
    pub confidence: u8,
}

impl CallTargetRef {
    pub fn canonical_key(&self) -> String {
        self.address
            .map(|address| format!("addr:0x{address:x}"))
            .unwrap_or_else(|| format!("sym:{}", self.symbol))
    }

    pub fn is_import_locked(&self) -> bool {
        matches!(
            self.provenance,
            CallTargetProvenance::Import | CallTargetProvenance::Intrinsic
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSummary {
    pub target: CallTargetRef,
    pub prototype: PrototypeSummary,
    pub effect_summary: CallEffectSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrototypeSummary {
    pub min_arity: usize,
    pub max_arity: usize,
    pub locked_exact_arity: Option<usize>,
    pub return_lattice: NirType,
    pub param_lattices: Vec<NirType>,
    pub soundness: SummarySoundness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryEffectRegion {
    Stack,
    Aggregate,
    HeapLike,
    GlobalLike,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CallEffectSummarySource {
    CallTargetRef,
    ImportSignature,
    FactStore,
    PreviewCalleeAnalysis,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirCallEffectSummary {
    pub reads_memory: Option<bool>,
    pub writes_memory: Option<bool>,
    pub escapes_args: Option<bool>,
    pub may_call_unknown: Option<bool>,
    pub may_exit: Option<bool>,
    pub source: Option<CallEffectSummarySource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallEffectSummary {
    pub reads_memory: Option<bool>,
    pub writes_memory: Option<bool>,
    pub escapes_args: Option<bool>,
    pub regions: Vec<MemoryEffectRegion>,
    pub wrapper_class: WrapperClass,
    pub wrapper_of: Option<CallTargetRef>,
    pub confidence: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallsiteState {
    pub arg_bindings: Vec<Option<String>>,
    pub stack_consumption: Option<u32>,
    pub variadic_state: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum IndirectControlSurface {
    BranchInd,
    CallInd,
    SwitchLike,
    DispatcherLike,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum UnsupportedControlFamily {
    MissingTargets,
    AmbiguousTargets,
    NonStructuralDispatcher,
    ExternalTarget,
    CallRegion,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct UnsupportedControlEvidence {
    pub opcode: String,
    pub source_block: Option<u64>,
    pub target_expr: Option<String>,
    pub successor_targets: Vec<u64>,
    pub failure_family: UnsupportedControlFamily,
    pub surface: IndirectControlSurface,
    pub confidence: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndirectTargetSet {
    pub definite: Vec<CallTargetRef>,
    pub possible: Vec<CallTargetRef>,
    pub confidence: u8,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatcherProofKind {
    SuccessorBounded,
    JumpTable,
    ConstantStrideIndex,
    TailForwarder,
    BoundedTargetSet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatcherShape {
    pub selector: String,
    pub target_map: Vec<(i64, CallTargetRef)>,
    pub default_target: Option<CallTargetRef>,
    pub proof_kind: DispatcherProofKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProofFailureFamily {
    MissingBounds,
    MissingOrdinalCoverage,
    MixedSelectorFamily,
    AmbiguousTargetMap,
    MissingFollow,
    SharedTailConflict,
    NonSideEffectFreeSelector,
    WidthOrSpaceMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatcherCaseMapSource {
    SuccessorOnly,
    JumpTableRecovered,
    CompareChainRecovered,
    Merged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatcherProofScope {
    TerminatorLocal,
    OuterDispatch,
    NestedDispatch,
    HelperTail,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatcherProofUnit {
    pub selector_expr: String,
    pub rendered_selector_expr: Option<String>,
    pub candidate_targets: Vec<u64>,
    pub recovered_cases: Vec<(i64, u64)>,
    pub selector_cardinality: usize,
    pub target_cardinality: usize,
    pub case_map_source: DispatcherCaseMapSource,
    pub default_target: Option<u64>,
    pub guard_set: Vec<String>,
    pub follow_block: Option<u64>,
    pub normalization: Option<SelectorNormalization>,
    pub legality_witness: Option<DispatcherLegality>,
    pub proof_scope: DispatcherProofScope,
    pub proof_complete: bool,
    pub failure_family: Option<ProofFailureFamily>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectorNormalization {
    pub base_subtract: Option<i64>,
    pub mask: Option<u64>,
    pub stride: Option<u64>,
    pub width: Option<u32>,
    pub address_space: Option<u64>,
    pub guard_bounds: Vec<(Option<i64>, Option<i64>)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DispatcherLegality {
    pub follow_block: Option<u64>,
    pub postdom_ok: bool,
    pub side_effect_free_selector: bool,
    pub ordinal_domain_complete: bool,
    pub shared_tail_conflict: bool,
    pub valid: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredTargetMap {
    pub cases: Vec<(i64, CallTargetRef)>,
    pub default_target: Option<CallTargetRef>,
    pub deterministic: bool,
    pub source: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct IndirectControlClassification {
    pub has_indirect_control: bool,
    pub has_preserved_indirect_surface: bool,
    pub has_unresolved_unsupported_indirect: bool,
    pub has_dispatcher_recovery: bool,
}

impl IndirectControlClassification {
    fn stats_indicate_indirect_control(stats: &NirBuildStats) -> bool {
        stats.unsupported_indirect_control_count > 0
            || stats.unsupported_indirect_call_count > 0
            || stats.unsupported_external_target_count > 0
            || stats.indirect_surface_preserved_count > 0
            || stats.indirect_target_set_refined_count > 0
            || stats.dispatcher_shape_recovered_count > 0
    }

    #[must_use]
    pub fn from_pcode(pcode: &crate::pcode::PcodeFunction) -> Self {
        Self::from_stats_or_observation(None, crate::pcode_has_indirect_control_flow(pcode))
    }

    #[must_use]
    pub fn from_stats_only(stats: Option<&NirBuildStats>) -> Self {
        match stats {
            Some(stats) => {
                Self::from_stats(Some(stats), Self::stats_indicate_indirect_control(stats))
            }
            None => Self::default(),
        }
    }

    #[must_use]
    pub fn from_stats(stats: Option<&NirBuildStats>, has_indirect_control: bool) -> Self {
        let stats = stats.cloned().unwrap_or_default();
        Self {
            has_indirect_control,
            has_preserved_indirect_surface: stats.indirect_surface_preserved_count > 0,
            has_unresolved_unsupported_indirect: stats.unsupported_indirect_control_count > 0
                || stats.unsupported_indirect_call_count > 0
                || stats.unsupported_external_target_count > 0,
            has_dispatcher_recovery: stats.dispatcher_shape_recovered_count > 0,
        }
    }

    #[must_use]
    pub fn from_stats_or_observation(
        stats: Option<&NirBuildStats>,
        observed_has_indirect_control: bool,
    ) -> Self {
        match stats {
            Some(stats) => Self::from_stats(
                Some(stats),
                Self::stats_indicate_indirect_control(stats) || observed_has_indirect_control,
            ),
            None => Self {
                has_indirect_control: observed_has_indirect_control,
                has_preserved_indirect_surface: false,
                has_unresolved_unsupported_indirect: false,
                has_dispatcher_recovery: false,
            },
        }
    }

    #[must_use]
    pub fn from_flags(
        has_indirect_control: bool,
        has_preserved_indirect_surface: bool,
        has_unresolved_unsupported_indirect: bool,
        has_dispatcher_recovery: bool,
    ) -> Self {
        Self {
            has_indirect_control,
            has_preserved_indirect_surface,
            has_unresolved_unsupported_indirect,
            has_dispatcher_recovery,
        }
    }

    #[must_use]
    pub fn allows_heuristic_surface_candidate(&self) -> bool {
        !self.has_unresolved_unsupported_indirect
    }

    #[must_use]
    pub fn allows_strict_explicit_candidate(&self, pcode_op_count: usize) -> bool {
        !self.has_unresolved_unsupported_indirect && pcode_op_count <= 800
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirFunction {
    pub name: String,
    pub address: u64,
    pub blocks: Vec<NirBlock>,
    pub locals: Vec<NirBinding>,
    pub params: Vec<NirBinding>,
    pub return_type: NirType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirPhiNode {
    pub dest_id: u32,              // Maps to SsaVarId
    pub operands: Vec<(u32, u32)>, // Pairs of (pred_block_id, src_var_id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NirBlock {
    pub id: u32,
    pub phis: Vec<NirPhiNode>,
    pub stmts: Vec<HirStmt>,
    pub terminator: NirTerminator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NirTerminator {
    Fallthrough(Option<u32>),
    Goto(u32),
    Branch {
        cond: HirExpr,
        true_target: u32,
        false_target: Option<u32>,
    },
    Return(Option<HirExpr>),
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirFunction {
    pub name: String,
    pub params: Vec<NirBinding>,
    pub locals: Vec<NirBinding>,
    pub return_type: NirType,
    pub surface_return_type_name: Option<String>,
    pub body: Vec<HirStmt>,
    /// ABI used for `param_k` register ordering and entry promotion (mirrors preview options).
    pub calling_convention: CallingConvention,
    /// When false, x64-only normalize passes (entry param promotion, etc.) are skipped.
    pub is_64bit: bool,
    /// Per-callee symbol: maximum argument count observed at direct call sites in this function.
    /// Downstream pipelines may merge this across functions for interprocedural arity bounds.
    /// [`IndexMap`] preserves insertion order for deterministic iteration / dumps.
    pub callee_observed_max_arity: IndexMap<String, usize>,
    /// Typed summaries derived from canonical call-target identity.
    pub callee_summaries: IndexMap<String, CallSummary>,
}

impl Default for HirFunction {
    fn default() -> Self {
        Self {
            name: String::new(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: Vec::new(),
            calling_convention: CallingConvention::default(),
            is_64bit: true,
            callee_observed_max_arity: IndexMap::new(),
            callee_summaries: IndexMap::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PassAggregate {
    pub total_time_ms: f64,
    pub total_invocations: usize,
    pub changed_count: usize,
    pub stmts_reduced: isize,
    pub locals_reduced: isize,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct NirBuildStats {
    #[serde(default)]
    pub build_duration_ms: usize,
    #[serde(default)]
    pub normalize_duration_ms: usize,
    #[serde(default)]
    pub structuring_duration_ms: usize,
    #[serde(default)]
    pub render_duration_ms: usize,
    #[serde(default)]
    pub rendered_code_len: usize,
    #[serde(default)]
    pub max_structuring_scc_component_size: usize,
    /// Total clean-room Ghidra-concept stages observed in this function build.
    #[serde(default)]
    pub ghidra_action_stage_count: usize,
    /// Fission builder stage corresponding to Ghidra `Funcdata` construction.
    #[serde(default)]
    pub ghidra_action_funcdata_build_count: usize,
    /// Fission value/representative stage corresponding to Ghidra `Heritage`.
    #[serde(default)]
    pub ghidra_action_heritage_value_recovery_count: usize,
    /// Fission normalize stage corresponding to Ghidra's core action pipeline.
    #[serde(default)]
    pub ghidra_action_normalize_count: usize,
    /// Fission type/prototype stage corresponding to Ghidra `FuncProto` work.
    #[serde(default)]
    pub ghidra_action_prototype_types_count: usize,
    /// Fission structuring stage corresponding to Ghidra `FlowBlock`/`BlockGraph`.
    #[serde(default)]
    pub ghidra_action_blockgraph_structuring_count: usize,
    /// Fission final renderer stage corresponding to Ghidra `PrintC`.
    #[serde(default)]
    pub ghidra_action_printc_count: usize,
    /// Completed Rust-native pipeline count after all clean-room stage records.
    #[serde(default)]
    pub ghidra_clean_room_pipeline_complete_count: usize,
    #[serde(default)]
    pub procedure_summary_contracted_count: usize,
    #[serde(default)]
    pub procedure_summary_tail_wrapper_count: usize,
    #[serde(default)]
    pub procedure_summary_import_thunk_count: usize,
    #[serde(default)]
    pub forced_linear_structuring_count: usize,
    #[serde(default)]
    pub structuring_force_linear_explicit_count: usize,
    #[serde(default)]
    pub structuring_force_linear_irreducible_budget_count: usize,
    #[serde(default)]
    pub structuring_force_linear_extreme_budget_count: usize,
    #[serde(default)]
    pub region_linearize_structuring_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_non_structuring_failure_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_no_exit_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_failed_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count:
        usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_successor_inline_rejected_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_revisit_cycle_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_body_lowering_unsupported_terminator_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_non_advancing_count: usize,
    #[serde(default)]
    pub region_linearize_rejected_irreducible_cfg_count: usize,
    #[serde(default)]
    pub structuring_scc_component_count: usize,
    #[serde(default)]
    pub structuring_irreducible_scc_count: usize,
    #[serde(default)]
    pub rule_block_if_no_exit_count: usize,
    #[serde(default)]
    pub rule_block_if_no_exit_accepted_count: usize,
    #[serde(default)]
    pub structuring_irreducible_header_count: usize,
    #[serde(default)]
    pub loop_control_explicit_reducer_count: usize,
    #[serde(default)]
    pub loop_control_rewrite_break_count: usize,
    #[serde(default)]
    pub loop_control_rewrite_continue_count: usize,
    #[serde(default)]
    pub loop_control_rewrite_skipped_nested_scope_count: usize,
    /// How many while loops used the subgraph body lowering path (complex body with branches).
    #[serde(default)]
    pub loop_while_subgraph_lowered_count: usize,
    /// How many Break statements were emitted via multi-exit CFG path (mid-body break).
    #[serde(default)]
    pub loop_multi_exit_break_count: usize,
    /// How many for-loop patterns were recognised and emitted as HirStmt::For.
    #[serde(default)]
    pub loop_for_lowered_count: usize,
    /// Region proofs produced by the CFG/region structuring owner.
    #[serde(default)]
    pub region_proof_candidate_count: usize,
    /// Region proofs that completed with legality fully closed.
    #[serde(default)]
    pub region_proof_completed_count: usize,
    /// Region proofs that existed but failed emit-ready gating.
    #[serde(default)]
    pub region_emit_ready_failed_count: usize,
    /// Ghidra BlockGraph-style region proof candidates considered by structuring.
    #[serde(default)]
    pub blockgraph_region_candidate_count: usize,
    /// BlockGraph-style region proofs with complete legality and emit readiness.
    #[serde(default)]
    pub blockgraph_region_complete_count: usize,
    /// BlockGraph-style region proofs rejected due to missing follow/postdom witness.
    #[serde(default)]
    pub blockgraph_region_rejected_missing_follow_count: usize,
    /// BlockGraph-style region proofs rejected because the target label must remain.
    #[serde(default)]
    pub blockgraph_region_rejected_must_emit_label_count: usize,
    /// BlockGraph-style region proofs rejected by emit-ready or alias/side-entry legality.
    #[serde(default)]
    pub blockgraph_region_rejected_emit_ready_count: usize,
    /// BlockGraph-style region proofs rejected due to irreducible SCC evidence.
    #[serde(default)]
    pub blockgraph_region_rejected_irreducible_count: usize,
    /// Conditional region candidates considered by the structuring owner.
    #[serde(default)]
    pub conditional_region_candidate_count: usize,
    /// Conditional regions selected into the structured overlay.
    #[serde(default)]
    pub conditional_region_promoted_count: usize,
    #[serde(default)]
    pub guarded_tail_candidate_count: usize,
    #[serde(default)]
    pub guarded_tail_promoted_count: usize,
    #[serde(default)]
    pub promotion_candidate_count: usize,
    #[serde(default)]
    pub promoted_region_count: usize,
    #[serde(default)]
    pub promotion_rejected_by_shape_count: usize,
    #[serde(default)]
    pub promotion_rejected_by_shape_missing_terminal_join_target_count: usize,
    #[serde(default)]
    pub promotion_rejected_by_shape_empty_nonterminal_tail_count: usize,
    #[serde(default)]
    pub promotion_rejected_by_gate_count: usize,
    #[serde(default)]
    pub discovery_seen_guarded_tail_like_shape_count: usize,
    #[serde(default)]
    pub guarded_tail_rejected_missing_terminal_join_count: usize,
    #[serde(default)]
    pub guarded_tail_rejected_side_entry_conflict_count: usize,
    #[serde(default)]
    pub guarded_tail_rejected_alias_interleave_conflict_count: usize,
    #[serde(default)]
    pub guarded_tail_rejected_ambiguous_follow_count: usize,
    #[serde(default)]
    pub guarded_tail_rejected_side_effectful_callee_count: usize,
    #[serde(default)]
    pub guarded_tail_replacement_plan_candidate_count: usize,
    #[serde(default)]
    pub guarded_tail_replacement_plan_completed_count: usize,
    #[serde(default)]
    pub guarded_tail_replacement_plan_merge_created_count: usize,
    #[serde(default)]
    pub guarded_tail_replacement_plan_rejected_missing_merge_count: usize,
    #[serde(default)]
    pub guarded_tail_replacement_plan_rejected_unstable_read_count: usize,
    #[serde(default)]
    pub guarded_tail_exported_binding_count: usize,
    #[serde(default)]
    pub guarded_tail_replacement_read_count: usize,
    #[serde(default)]
    pub guarded_tail_replacement_read_rewritten_count: usize,
    #[serde(default)]
    pub guarded_tail_replacement_read_rejected_nondominated_count: usize,
    #[serde(default)]
    pub guarded_tail_replacement_read_rejected_nonremovable_op_count: usize,
    #[serde(default)]
    pub discovery_rejected_noncanonical_layout_count: usize,
    #[serde(default)]
    pub canonicalized_guarded_tail_shape_count: usize,
    #[serde(default)]
    pub canonicalization_failed_multiple_payload_entries: usize,
    #[serde(default)]
    pub canonicalization_failed_interleaved_join_uses: usize,
    #[serde(default)]
    pub canonicalization_failed_interleaved_join_uses_no_next_label_count: usize,
    #[serde(default)]
    pub canonicalization_failed_interleaved_join_uses_nontrivial_segment_count: usize,
    #[serde(default)]
    pub canonicalization_failed_nonterminal_join_label: usize,
    #[serde(default)]
    pub canonicalization_failed_nested_tail_escape: usize,
    #[serde(default)]
    pub canonicalized_interleaved_join_use_count: usize,
    #[serde(default)]
    pub canonicalized_local_nonfallthrough_alias_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_not_fallthrough_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_not_fallthrough_top_level_after_label_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_not_fallthrough_nested_after_label_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_has_multiple_internal_predecessors_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_has_nonlocal_ref_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_has_nonlocal_ref_external_before_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_has_nonlocal_ref_nested_before_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count: usize,
    #[serde(default)]
    pub canonicalization_failed_alias_body_not_trivial_count: usize,
    #[serde(default)]
    pub canonicalization_failed_join_has_external_ref_count: usize,
    #[serde(default)]
    pub canonicalization_failed_payload_crosses_join_count: usize,
    #[serde(default)]
    pub rejected_must_emit_label: usize,
    #[serde(default)]
    pub rejected_must_emit_label_surviving_middle_ref: usize,
    #[serde(default)]
    pub rejected_must_emit_label_surviving_external_ref: usize,
    #[serde(default)]
    pub rejected_must_emit_label_owner_conflict: usize,
    #[serde(default)]
    pub rejected_not_single_pred_succ: usize,
    #[serde(default)]
    pub rejected_external_entry: usize,
    #[serde(default)]
    pub rejected_loop_or_switch_target: usize,
    #[serde(default)]
    pub condition_fold_and_count: usize,
    #[serde(default)]
    pub condition_fold_or_count: usize,
    #[serde(default)]
    pub condition_fold_rejected_side_effect: usize,
    /// Spill temps renamed to `param_k` in the entry linear prefix (HIR normalize).
    #[serde(default)]
    pub entry_param_promotion_spill_rename_count: usize,
    /// Conservative folds / annotations for variadic tail stack-arg regions (ABI lattice).
    #[serde(default)]
    pub variadic_stack_region_fold_count: usize,
    /// ABI-classified stack / carrier slots recovered from entry/call usage.
    #[serde(default)]
    pub abi_slot_recovered_count: usize,
    /// Home / shadow slots promoted out of generic `local_*` naming.
    #[serde(default)]
    pub home_slot_promoted_count: usize,
    /// Semantic `va_start` statements recovered from ABI carrier setup.
    #[serde(default)]
    pub va_start_recovered_count: usize,
    /// Call targets/signatures tightened beyond anonymous `sub_/FUN_` defaults.
    #[serde(default)]
    pub call_signature_refined_count: usize,
    /// Security-cookie setup/check pairs folded into semantic form.
    #[serde(default)]
    pub security_cookie_fold_count: usize,
    /// Non-semantic call/return scaffolding stores removed after proof.
    #[serde(default)]
    pub call_artifact_removed_count: usize,
    /// Typed object shapes recovered from partitioned memory regions.
    #[serde(default)]
    pub object_shape_recovered_count: usize,
    /// Root object regions recovered before field/array surfacing.
    #[serde(default)]
    pub object_root_recovered_count: usize,
    /// Typed fact evidences collected before canonical object/surface promotion.
    #[serde(default)]
    pub typed_fact_evidence_count: usize,
    /// Conflicting typed facts withheld from canonical promotion.
    #[serde(default)]
    pub typed_fact_conflict_count: usize,
    /// Root-object facts promoted into canonical object inventory.
    #[serde(default)]
    pub object_root_fact_promotion_count: usize,
    /// Existing object shapes refined with field/array/opaque range evidence.
    #[serde(default)]
    pub typed_object_shape_refined_count: usize,
    /// New surface bindings/slot aliases promoted from raw partition evidence.
    #[serde(default)]
    pub surface_binding_promoted_count: usize,
    /// New canonical surface facts promoted before naming/polish.
    #[serde(default)]
    pub surface_fact_promotion_count: usize,
    /// Call prototype summaries refined beyond raw call-site arity bounds.
    #[serde(default)]
    pub prototype_summary_refined_count: usize,
    /// Bounded propagation rounds for canonical prototype summaries.
    #[serde(default)]
    pub prototype_summary_round_count: usize,
    /// Call/effect summaries refined beyond plain arity/name recovery.
    #[serde(default)]
    pub call_effect_summary_refined_count: usize,
    /// Forwarding/wrapper summaries folded into canonical call-target metadata.
    #[serde(default)]
    pub wrapper_summary_fold_count: usize,
    /// Cleanup families skipped due to explicit budget/readiness guards.
    #[serde(default)]
    pub cleanup_budget_skip_count: usize,
    /// Binding-initializer cleanup family invocations.
    #[serde(default)]
    pub cleanup_family_binding_init_count: usize,
    /// Statement canonical cleanup family invocations.
    #[serde(default)]
    pub cleanup_family_stmt_canonical_count: usize,
    /// Statement-fold canonical cleanup subfamily invocations.
    #[serde(default)]
    pub cleanup_stmt_fold_count: usize,
    /// Boundary-label cleanup subfamily invocations.
    #[serde(default)]
    pub cleanup_boundary_label_count: usize,
    /// Loopish rewrite cleanup subfamily invocations.
    #[serde(default)]
    pub cleanup_loopish_rewrite_count: usize,
    /// Dead-binding cleanup family invocations.
    #[serde(default)]
    pub cleanup_family_dead_binding_count: usize,
    /// Rounds of interprocedural signature constraint propagation (call-site arity meet/join).
    #[serde(default)]
    pub interproc_signature_constraint_rounds: usize,
    /// Unsupported indirect-control terminators observed before explicit surfacing.
    #[serde(default)]
    pub unsupported_indirect_control_count: usize,
    /// Unsupported indirect-call targets observed before explicit surfacing.
    #[serde(default)]
    pub unsupported_indirect_call_count: usize,
    /// Unsupported external branch/call targets that resolved outside the canonical CFG slice.
    #[serde(default)]
    pub unsupported_external_target_count: usize,
    /// Unsupported indirect/control sites preserved as explicit pseudo-surface instead of marker calls.
    #[serde(default)]
    pub indirect_surface_preserved_count: usize,
    /// Indirect target sets refined by bounded target proof.
    #[serde(default)]
    pub indirect_target_set_refined_count: usize,
    /// Dispatcher-like control shapes recovered from structural proof.
    #[serde(default)]
    pub dispatcher_shape_recovered_count: usize,
    /// Nontrivial repeated pure expressions stabilized into explicit temporaries.
    #[serde(default)]
    pub materialization_stabilized_count: usize,
    /// Materialization/rewrite candidates considered by the replacement planner.
    #[serde(default)]
    pub replacement_plan_candidate_count: usize,
    /// Candidates whose replacement plan completed without needing a preserved representative.
    #[serde(default)]
    pub replacement_plan_completed_count: usize,
    /// Synthetic merge bindings introduced by the replacement planner.
    #[serde(default)]
    pub replacement_plan_merge_binding_count: usize,
    /// Replacement plans rejected because no alias-safe rewrite existed.
    #[serde(default)]
    pub replacement_plan_rejected_alias_unsafe_count: usize,
    /// Replacement plans rejected because a merge/read bridge was required but unavailable.
    #[serde(default)]
    pub replacement_plan_rejected_missing_merge_count: usize,
    /// Replacement plans rejected because the nonlocal owner was root/entry representative attribution.
    #[serde(default)]
    pub replacement_plan_rejected_representative_root_attribution_count: usize,
    /// Replacement plans rejected because the nonlocal owner was temp-only representative lifecycle.
    #[serde(default)]
    pub replacement_plan_rejected_temp_only_representative_lifecycle_count: usize,
    /// Replacement plans rejected because the nonlocal owner was a dead temp representative.
    #[serde(default)]
    pub replacement_plan_rejected_dead_temp_representative_count: usize,
    /// Legacy inline candidates intentionally kept materialized by the replacement planner.
    #[serde(default)]
    pub materialization_inline_suppressed_count: usize,
    /// Representative selections that fell back to a weaker form instead of a preserved alias/temp.
    #[serde(default)]
    pub representative_downgrade_count: usize,
    /// Representative downgrades caused by the absence of an alias-safe source expression.
    #[serde(default)]
    pub representative_downgrade_no_aliassafe_source_count: usize,
    /// Representative downgrades caused by join-shape conflicts.
    #[serde(default)]
    pub representative_downgrade_join_conflict_count: usize,
    /// Preserved representatives that cleanup/prune intentionally refused to erase.
    #[serde(default)]
    pub preserved_temp_prune_blocked_count: usize,
    /// Preserved representatives intentionally excluded from copy propagation.
    #[serde(default)]
    pub preserved_temp_copyprop_skip_count: usize,
    /// Join-hoisted shared representatives emitted as preserved temps.
    #[serde(default)]
    pub gvn_join_preserved_count: usize,
    /// Switch/dispatcher surfaces emitted directly from proof payloads.
    #[serde(default)]
    pub proof_payload_direct_emit_count: usize,
    /// Heavy reruns skipped because the preserving pass made no structural change.
    #[serde(default)]
    pub pass_rerun_skipped_by_preservation_count: usize,
    /// Dispatcher proof units discovered before emission gating.
    #[serde(default)]
    pub dispatcher_proof_unit_count: usize,
    /// Dispatcher proof units that completed with a legal target map.
    #[serde(default)]
    pub dispatcher_proof_completed_count: usize,
    /// Dispatcher proof units rejected after proof analysis.
    #[serde(default)]
    pub dispatcher_proof_failed_count: usize,
    /// Switch-like regions that had proof/candidate evidence but failed emit-ready gating.
    #[serde(default)]
    pub switch_emit_ready_failed_count: usize,
    /// Compare-chain dispatcher proofs discovered from conditional ladders.
    #[serde(default)]
    pub compare_chain_dispatcher_count: usize,
    /// Candidate-scoped jump resolver admissions that reached the solver.
    #[serde(default)]
    pub candidate_scoped_jump_resolver_count: usize,
    /// SCCP passes skipped because admission analysis found no useful control-flow seeds.
    #[serde(default)]
    pub sccp_skipped_by_admission_count: usize,
    /// Wide dead-assignment reruns admitted after a successful first pass.
    #[serde(default)]
    pub wide_dead_assignment_rerun_admitted_count: usize,
    /// Wide dead-assignment reruns skipped by explicit admission guards.
    #[serde(default)]
    pub wide_dead_assignment_rerun_skipped_by_admission_count: usize,
    /// Preview/render gating disagreed with the canonical target profile.
    #[serde(default)]
    pub pe_admission_profile_mismatch_count: usize,
    /// Memory normalization passes skipped because typed-fact prefilter found no object roots.
    #[serde(default)]
    pub memory_fact_prefilter_skip_count: usize,
    /// Aggregate-field recovery skipped by explicit admission guards.
    #[serde(default)]
    pub aggregate_fields_skipped_by_admission_count: usize,
    /// Memory slot surfacing exited through the cheap path because no alias-safe roots were present.
    #[serde(default)]
    pub memory_slot_cheap_exit_count: usize,
    /// Canonical family totals derived from structuring failures/recovery in pcode.
    #[serde(default)]
    pub structuring_reason_region_legality_count: usize,
    #[serde(default)]
    pub structuring_reason_follow_failure_count: usize,
    #[serde(default)]
    pub structuring_reason_irreducible_count: usize,
    #[serde(default)]
    pub structuring_reason_loop_exit_count: usize,
    #[serde(default)]
    pub structuring_reason_switch_shape_count: usize,
    #[serde(default)]
    pub structuring_reason_budget_count: usize,
    #[serde(default)]
    pub pass_metrics: std::collections::BTreeMap<String, PassAggregate>,
}

impl NirBuildStats {
    pub fn merge_assign(&mut self, other: &Self) {
        self.build_duration_ms += other.build_duration_ms;
        self.normalize_duration_ms += other.normalize_duration_ms;
        self.structuring_duration_ms += other.structuring_duration_ms;
        self.render_duration_ms += other.render_duration_ms;
        self.rendered_code_len += other.rendered_code_len;
        self.max_structuring_scc_component_size = self
            .max_structuring_scc_component_size
            .max(other.max_structuring_scc_component_size);
        self.ghidra_action_stage_count += other.ghidra_action_stage_count;
        self.ghidra_action_funcdata_build_count += other.ghidra_action_funcdata_build_count;
        self.ghidra_action_heritage_value_recovery_count +=
            other.ghidra_action_heritage_value_recovery_count;
        self.ghidra_action_normalize_count += other.ghidra_action_normalize_count;
        self.ghidra_action_prototype_types_count += other.ghidra_action_prototype_types_count;
        self.ghidra_action_blockgraph_structuring_count +=
            other.ghidra_action_blockgraph_structuring_count;
        self.ghidra_action_printc_count += other.ghidra_action_printc_count;
        self.ghidra_clean_room_pipeline_complete_count +=
            other.ghidra_clean_room_pipeline_complete_count;
        self.procedure_summary_contracted_count += other.procedure_summary_contracted_count;
        self.procedure_summary_tail_wrapper_count += other.procedure_summary_tail_wrapper_count;
        self.procedure_summary_import_thunk_count += other.procedure_summary_import_thunk_count;
        self.forced_linear_structuring_count += other.forced_linear_structuring_count;
        self.structuring_force_linear_explicit_count +=
            other.structuring_force_linear_explicit_count;
        self.structuring_force_linear_irreducible_budget_count +=
            other.structuring_force_linear_irreducible_budget_count;
        self.structuring_force_linear_extreme_budget_count +=
            other.structuring_force_linear_extreme_budget_count;
        self.region_linearize_structuring_count += other.region_linearize_structuring_count;
        self.region_linearize_rejected_non_structuring_failure_count +=
            other.region_linearize_rejected_non_structuring_failure_count;
        self.region_linearize_rejected_no_exit_count +=
            other.region_linearize_rejected_no_exit_count;
        self.region_linearize_rejected_body_lowering_failed_count +=
            other.region_linearize_rejected_body_lowering_failed_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count +=
            other.region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count += other
            .region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count +=
            other
                .region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count +=
            other.region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count +=
            other.region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count += other
            .region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count += other
            .region_linearize_rejected_body_lowering_conditional_tail_arm_body_lowering_failed_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count += other
            .region_linearize_rejected_body_lowering_conditional_tail_one_arm_body_lowering_failed_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count += other
            .region_linearize_rejected_body_lowering_conditional_tail_both_arms_body_lowering_failed_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count += other
            .region_linearize_rejected_body_lowering_conditional_tail_follow_tail_lowering_failed_count;
        self.region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count += other
            .region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count;
        self.region_linearize_rejected_body_lowering_successor_inline_rejected_count +=
            other.region_linearize_rejected_body_lowering_successor_inline_rejected_count;
        self.region_linearize_rejected_body_lowering_revisit_cycle_count +=
            other.region_linearize_rejected_body_lowering_revisit_cycle_count;
        self.region_linearize_rejected_body_lowering_unsupported_terminator_count +=
            other.region_linearize_rejected_body_lowering_unsupported_terminator_count;
        self.region_linearize_rejected_non_advancing_count +=
            other.region_linearize_rejected_non_advancing_count;
        self.region_linearize_rejected_irreducible_cfg_count +=
            other.region_linearize_rejected_irreducible_cfg_count;
        self.structuring_scc_component_count += other.structuring_scc_component_count;
        self.structuring_irreducible_scc_count += other.structuring_irreducible_scc_count;
        self.rule_block_if_no_exit_count += other.rule_block_if_no_exit_count;
        self.rule_block_if_no_exit_accepted_count += other.rule_block_if_no_exit_accepted_count;
        self.structuring_irreducible_header_count += other.structuring_irreducible_header_count;
        self.loop_control_explicit_reducer_count += other.loop_control_explicit_reducer_count;
        self.loop_control_rewrite_break_count += other.loop_control_rewrite_break_count;
        self.loop_control_rewrite_continue_count += other.loop_control_rewrite_continue_count;
        self.loop_control_rewrite_skipped_nested_scope_count +=
            other.loop_control_rewrite_skipped_nested_scope_count;
        self.loop_while_subgraph_lowered_count += other.loop_while_subgraph_lowered_count;
        self.loop_multi_exit_break_count += other.loop_multi_exit_break_count;
        self.loop_for_lowered_count += other.loop_for_lowered_count;
        self.region_proof_candidate_count += other.region_proof_candidate_count;
        self.region_proof_completed_count += other.region_proof_completed_count;
        self.region_emit_ready_failed_count += other.region_emit_ready_failed_count;
        self.blockgraph_region_candidate_count += other.blockgraph_region_candidate_count;
        self.blockgraph_region_complete_count += other.blockgraph_region_complete_count;
        self.blockgraph_region_rejected_missing_follow_count +=
            other.blockgraph_region_rejected_missing_follow_count;
        self.blockgraph_region_rejected_must_emit_label_count +=
            other.blockgraph_region_rejected_must_emit_label_count;
        self.blockgraph_region_rejected_emit_ready_count +=
            other.blockgraph_region_rejected_emit_ready_count;
        self.blockgraph_region_rejected_irreducible_count +=
            other.blockgraph_region_rejected_irreducible_count;
        self.conditional_region_candidate_count += other.conditional_region_candidate_count;
        self.conditional_region_promoted_count += other.conditional_region_promoted_count;
        self.guarded_tail_candidate_count += other.guarded_tail_candidate_count;
        self.guarded_tail_promoted_count += other.guarded_tail_promoted_count;
        self.promotion_candidate_count += other.promotion_candidate_count;
        self.promoted_region_count += other.promoted_region_count;
        self.promotion_rejected_by_shape_count += other.promotion_rejected_by_shape_count;
        self.promotion_rejected_by_shape_missing_terminal_join_target_count +=
            other.promotion_rejected_by_shape_missing_terminal_join_target_count;
        self.promotion_rejected_by_shape_empty_nonterminal_tail_count +=
            other.promotion_rejected_by_shape_empty_nonterminal_tail_count;
        self.promotion_rejected_by_gate_count += other.promotion_rejected_by_gate_count;
        self.discovery_seen_guarded_tail_like_shape_count +=
            other.discovery_seen_guarded_tail_like_shape_count;
        self.guarded_tail_rejected_missing_terminal_join_count +=
            other.guarded_tail_rejected_missing_terminal_join_count;
        self.guarded_tail_rejected_side_entry_conflict_count +=
            other.guarded_tail_rejected_side_entry_conflict_count;
        self.guarded_tail_rejected_alias_interleave_conflict_count +=
            other.guarded_tail_rejected_alias_interleave_conflict_count;
        self.guarded_tail_rejected_ambiguous_follow_count +=
            other.guarded_tail_rejected_ambiguous_follow_count;
        self.guarded_tail_rejected_side_effectful_callee_count +=
            other.guarded_tail_rejected_side_effectful_callee_count;
        self.guarded_tail_replacement_plan_candidate_count +=
            other.guarded_tail_replacement_plan_candidate_count;
        self.guarded_tail_replacement_plan_completed_count +=
            other.guarded_tail_replacement_plan_completed_count;
        self.guarded_tail_replacement_plan_merge_created_count +=
            other.guarded_tail_replacement_plan_merge_created_count;
        self.guarded_tail_replacement_plan_rejected_missing_merge_count +=
            other.guarded_tail_replacement_plan_rejected_missing_merge_count;
        self.guarded_tail_replacement_plan_rejected_unstable_read_count +=
            other.guarded_tail_replacement_plan_rejected_unstable_read_count;
        self.guarded_tail_exported_binding_count += other.guarded_tail_exported_binding_count;
        self.guarded_tail_replacement_read_count += other.guarded_tail_replacement_read_count;
        self.guarded_tail_replacement_read_rewritten_count +=
            other.guarded_tail_replacement_read_rewritten_count;
        self.guarded_tail_replacement_read_rejected_nondominated_count +=
            other.guarded_tail_replacement_read_rejected_nondominated_count;
        self.guarded_tail_replacement_read_rejected_nonremovable_op_count +=
            other.guarded_tail_replacement_read_rejected_nonremovable_op_count;
        self.discovery_rejected_noncanonical_layout_count +=
            other.discovery_rejected_noncanonical_layout_count;
        self.canonicalized_guarded_tail_shape_count += other.canonicalized_guarded_tail_shape_count;
        self.canonicalization_failed_multiple_payload_entries +=
            other.canonicalization_failed_multiple_payload_entries;
        self.canonicalization_failed_interleaved_join_uses +=
            other.canonicalization_failed_interleaved_join_uses;
        self.canonicalization_failed_interleaved_join_uses_no_next_label_count +=
            other.canonicalization_failed_interleaved_join_uses_no_next_label_count;
        self.canonicalization_failed_interleaved_join_uses_nontrivial_segment_count +=
            other.canonicalization_failed_interleaved_join_uses_nontrivial_segment_count;
        self.canonicalization_failed_nonterminal_join_label +=
            other.canonicalization_failed_nonterminal_join_label;
        self.canonicalization_failed_nested_tail_escape +=
            other.canonicalization_failed_nested_tail_escape;
        self.canonicalized_interleaved_join_use_count +=
            other.canonicalized_interleaved_join_use_count;
        self.canonicalized_local_nonfallthrough_alias_count +=
            other.canonicalized_local_nonfallthrough_alias_count;
        self.canonicalization_failed_alias_not_fallthrough_count +=
            other.canonicalization_failed_alias_not_fallthrough_count;
        self.canonicalization_failed_alias_not_fallthrough_top_level_after_label_count +=
            other.canonicalization_failed_alias_not_fallthrough_top_level_after_label_count;
        self.canonicalization_failed_alias_not_fallthrough_nested_after_label_count +=
            other.canonicalization_failed_alias_not_fallthrough_nested_after_label_count;
        self.canonicalization_failed_alias_has_multiple_internal_predecessors_count +=
            other.canonicalization_failed_alias_has_multiple_internal_predecessors_count;
        self.canonicalization_failed_alias_has_nonlocal_ref_count +=
            other.canonicalization_failed_alias_has_nonlocal_ref_count;
        self.canonicalization_failed_alias_has_nonlocal_ref_external_before_count +=
            other.canonicalization_failed_alias_has_nonlocal_ref_external_before_count;
        self.canonicalization_failed_alias_has_nonlocal_ref_nested_before_count +=
            other.canonicalization_failed_alias_has_nonlocal_ref_nested_before_count;
        self.canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count +=
            other.canonicalization_failed_alias_has_nonlocal_ref_post_segment_ref_count;
        self.canonicalization_failed_alias_body_not_trivial_count +=
            other.canonicalization_failed_alias_body_not_trivial_count;
        self.canonicalization_failed_join_has_external_ref_count +=
            other.canonicalization_failed_join_has_external_ref_count;
        self.canonicalization_failed_payload_crosses_join_count +=
            other.canonicalization_failed_payload_crosses_join_count;
        self.rejected_must_emit_label += other.rejected_must_emit_label;
        self.rejected_must_emit_label_surviving_middle_ref +=
            other.rejected_must_emit_label_surviving_middle_ref;
        self.rejected_must_emit_label_surviving_external_ref +=
            other.rejected_must_emit_label_surviving_external_ref;
        self.rejected_must_emit_label_owner_conflict +=
            other.rejected_must_emit_label_owner_conflict;
        self.rejected_not_single_pred_succ += other.rejected_not_single_pred_succ;
        self.rejected_external_entry += other.rejected_external_entry;
        self.rejected_loop_or_switch_target += other.rejected_loop_or_switch_target;
        self.condition_fold_and_count += other.condition_fold_and_count;
        self.condition_fold_or_count += other.condition_fold_or_count;
        self.condition_fold_rejected_side_effect += other.condition_fold_rejected_side_effect;
        self.entry_param_promotion_spill_rename_count +=
            other.entry_param_promotion_spill_rename_count;
        self.variadic_stack_region_fold_count += other.variadic_stack_region_fold_count;
        self.abi_slot_recovered_count += other.abi_slot_recovered_count;
        self.home_slot_promoted_count += other.home_slot_promoted_count;
        self.va_start_recovered_count += other.va_start_recovered_count;
        self.call_signature_refined_count += other.call_signature_refined_count;
        self.security_cookie_fold_count += other.security_cookie_fold_count;
        self.call_artifact_removed_count += other.call_artifact_removed_count;
        self.object_shape_recovered_count += other.object_shape_recovered_count;
        self.object_root_recovered_count += other.object_root_recovered_count;
        self.typed_fact_evidence_count += other.typed_fact_evidence_count;
        self.typed_fact_conflict_count += other.typed_fact_conflict_count;
        self.object_root_fact_promotion_count += other.object_root_fact_promotion_count;
        self.typed_object_shape_refined_count += other.typed_object_shape_refined_count;
        self.surface_binding_promoted_count += other.surface_binding_promoted_count;
        self.surface_fact_promotion_count += other.surface_fact_promotion_count;
        self.prototype_summary_refined_count += other.prototype_summary_refined_count;
        self.prototype_summary_round_count += other.prototype_summary_round_count;
        self.call_effect_summary_refined_count += other.call_effect_summary_refined_count;
        self.wrapper_summary_fold_count += other.wrapper_summary_fold_count;
        self.cleanup_budget_skip_count += other.cleanup_budget_skip_count;
        self.cleanup_family_binding_init_count += other.cleanup_family_binding_init_count;
        self.cleanup_family_stmt_canonical_count += other.cleanup_family_stmt_canonical_count;
        self.cleanup_stmt_fold_count += other.cleanup_stmt_fold_count;
        self.cleanup_boundary_label_count += other.cleanup_boundary_label_count;
        self.cleanup_loopish_rewrite_count += other.cleanup_loopish_rewrite_count;
        self.cleanup_family_dead_binding_count += other.cleanup_family_dead_binding_count;
        self.interproc_signature_constraint_rounds += other.interproc_signature_constraint_rounds;
        self.unsupported_indirect_control_count += other.unsupported_indirect_control_count;
        self.unsupported_indirect_call_count += other.unsupported_indirect_call_count;
        self.unsupported_external_target_count += other.unsupported_external_target_count;
        self.indirect_surface_preserved_count += other.indirect_surface_preserved_count;
        self.indirect_target_set_refined_count += other.indirect_target_set_refined_count;
        self.dispatcher_shape_recovered_count += other.dispatcher_shape_recovered_count;
        self.materialization_stabilized_count += other.materialization_stabilized_count;
        self.replacement_plan_candidate_count += other.replacement_plan_candidate_count;
        self.replacement_plan_completed_count += other.replacement_plan_completed_count;
        self.replacement_plan_merge_binding_count += other.replacement_plan_merge_binding_count;
        self.replacement_plan_rejected_alias_unsafe_count +=
            other.replacement_plan_rejected_alias_unsafe_count;
        self.replacement_plan_rejected_missing_merge_count +=
            other.replacement_plan_rejected_missing_merge_count;
        self.replacement_plan_rejected_representative_root_attribution_count +=
            other.replacement_plan_rejected_representative_root_attribution_count;
        self.replacement_plan_rejected_temp_only_representative_lifecycle_count +=
            other.replacement_plan_rejected_temp_only_representative_lifecycle_count;
        self.replacement_plan_rejected_dead_temp_representative_count +=
            other.replacement_plan_rejected_dead_temp_representative_count;
        self.materialization_inline_suppressed_count +=
            other.materialization_inline_suppressed_count;
        self.representative_downgrade_count += other.representative_downgrade_count;
        self.representative_downgrade_no_aliassafe_source_count +=
            other.representative_downgrade_no_aliassafe_source_count;
        self.representative_downgrade_join_conflict_count +=
            other.representative_downgrade_join_conflict_count;
        self.preserved_temp_prune_blocked_count += other.preserved_temp_prune_blocked_count;
        self.preserved_temp_copyprop_skip_count += other.preserved_temp_copyprop_skip_count;
        self.gvn_join_preserved_count += other.gvn_join_preserved_count;
        self.proof_payload_direct_emit_count += other.proof_payload_direct_emit_count;
        self.pass_rerun_skipped_by_preservation_count +=
            other.pass_rerun_skipped_by_preservation_count;
        self.dispatcher_proof_unit_count += other.dispatcher_proof_unit_count;
        self.dispatcher_proof_completed_count += other.dispatcher_proof_completed_count;
        self.dispatcher_proof_failed_count += other.dispatcher_proof_failed_count;
        self.switch_emit_ready_failed_count += other.switch_emit_ready_failed_count;
        self.compare_chain_dispatcher_count += other.compare_chain_dispatcher_count;
        self.candidate_scoped_jump_resolver_count += other.candidate_scoped_jump_resolver_count;
        self.sccp_skipped_by_admission_count += other.sccp_skipped_by_admission_count;
        self.wide_dead_assignment_rerun_admitted_count +=
            other.wide_dead_assignment_rerun_admitted_count;
        self.wide_dead_assignment_rerun_skipped_by_admission_count +=
            other.wide_dead_assignment_rerun_skipped_by_admission_count;
        self.pe_admission_profile_mismatch_count += other.pe_admission_profile_mismatch_count;
        self.memory_fact_prefilter_skip_count += other.memory_fact_prefilter_skip_count;
        self.aggregate_fields_skipped_by_admission_count +=
            other.aggregate_fields_skipped_by_admission_count;
        self.memory_slot_cheap_exit_count += other.memory_slot_cheap_exit_count;
        self.structuring_reason_region_legality_count +=
            other.structuring_reason_region_legality_count;
        self.structuring_reason_follow_failure_count +=
            other.structuring_reason_follow_failure_count;
        self.structuring_reason_irreducible_count += other.structuring_reason_irreducible_count;
        self.structuring_reason_loop_exit_count += other.structuring_reason_loop_exit_count;
        self.structuring_reason_switch_shape_count += other.structuring_reason_switch_shape_count;
        self.structuring_reason_budget_count += other.structuring_reason_budget_count;

        for (name, agg) in &other.pass_metrics {
            let current = self.pass_metrics.entry(name.clone()).or_default();
            current.total_time_ms += agg.total_time_ms;
            current.total_invocations += agg.total_invocations;
            current.changed_count += agg.changed_count;
            current.stmts_reduced += agg.stmts_reduced;
            current.locals_reduced += agg.locals_reduced;
        }
    }

    pub fn refresh_structuring_reason_families(&mut self) {
        self.structuring_reason_region_legality_count = self
            .region_linearize_rejected_non_structuring_failure_count
            + self.region_linearize_rejected_no_exit_count
            + self.region_linearize_rejected_body_lowering_unsupported_terminator_count
            + self.region_emit_ready_failed_count
            + self.guarded_tail_rejected_side_effectful_callee_count
            + self.guarded_tail_rejected_missing_terminal_join_count
            + self.guarded_tail_rejected_alias_interleave_conflict_count
            + self.rejected_external_entry
            + self.rejected_not_single_pred_succ;
        self.structuring_reason_follow_failure_count =
            self.region_linearize_rejected_body_lowering_conditional_tail_exit_mismatch_count
                + self.region_linearize_rejected_body_lowering_conditional_tail_no_common_follow_in_window_count
                + self.region_linearize_rejected_body_lowering_conditional_tail_follow_beyond_window_count
                + self.region_linearize_rejected_body_lowering_conditional_tail_ambiguous_multiple_follows_count
                + self.guarded_tail_rejected_ambiguous_follow_count
                + self.region_linearize_rejected_body_lowering_successor_inline_rejected_count;
        self.structuring_reason_irreducible_count = self
            .region_linearize_rejected_irreducible_cfg_count
            + self.structuring_irreducible_scc_count
            + self.structuring_irreducible_header_count;
        self.structuring_reason_loop_exit_count = self
            .region_linearize_rejected_body_lowering_conditional_tail_side_entry_or_exit_count
            + self.guarded_tail_rejected_side_entry_conflict_count
            + self.loop_control_rewrite_skipped_nested_scope_count
            + self.rejected_loop_or_switch_target;
        self.structuring_reason_switch_shape_count = self
            .region_linearize_rejected_body_lowering_conditional_tail_complex_arm_shape_count
            + self.switch_emit_ready_failed_count;
        self.structuring_reason_budget_count =
            self.region_linearize_rejected_body_lowering_conditional_tail_depth_or_budget_exhausted_count
                + self.region_linearize_rejected_non_advancing_count
                + self.region_linearize_rejected_body_lowering_revisit_cycle_count;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirStmt {
    Assign {
        lhs: HirLValue,
        rhs: HirExpr,
    },
    Expr(HirExpr),
    VaStart {
        va_list: HirExpr,
        last_named_param: String,
    },
    Block(Vec<HirStmt>),
    Switch {
        expr: HirExpr,
        cases: Vec<HirSwitchCase>,
        default: Vec<HirStmt>,
    },
    If {
        cond: HirExpr,
        then_body: Vec<HirStmt>,
        else_body: Vec<HirStmt>,
    },
    While {
        cond: HirExpr,
        body: Vec<HirStmt>,
    },
    DoWhile {
        body: Vec<HirStmt>,
        cond: HirExpr,
    },
    For {
        init: Option<Box<HirStmt>>,
        cond: Option<HirExpr>,
        update: Option<Box<HirStmt>>,
        body: Vec<HirStmt>,
    },
    Label(String),
    Goto(String),
    Return(Option<HirExpr>),
    Break,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HirSwitchCase {
    pub values: Vec<i64>,
    pub body: Vec<HirStmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirLValue {
    Var(String),
    Deref {
        ptr: Box<HirExpr>,
        ty: NirType,
    },
    Index {
        base: Box<HirExpr>,
        index: Box<HirExpr>,
        elem_ty: NirType,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HirExpr {
    Var(String),
    Const(i64, NirType),
    Cast {
        ty: NirType,
        expr: Box<HirExpr>,
    },
    Unary {
        op: HirUnaryOp,
        expr: Box<HirExpr>,
        ty: NirType,
    },
    Binary {
        op: HirBinaryOp,
        lhs: Box<HirExpr>,
        rhs: Box<HirExpr>,
        ty: NirType,
    },
    Call {
        target: String,
        args: Vec<HirExpr>,
        ty: NirType,
    },
    Load {
        ptr: Box<HirExpr>,
        ty: NirType,
    },
    PtrOffset {
        base: Box<HirExpr>,
        offset: i64,
    },
    Index {
        base: Box<HirExpr>,
        index: Box<HirExpr>,
        elem_ty: NirType,
    },
    AggregateCopy {
        src: Box<HirExpr>,
        size: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirUnaryOp {
    Neg,
    Not,
    BitNot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    LogicalAnd,
    LogicalOr,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    Sar,
    Eq,
    Ne,
    Lt,
    Le,
    SLt,
    SLe,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirRenderOptions {
    pub pe_x64_only: bool,
    pub is_64bit: bool,
    pub pointer_size: u32,
    pub format: String,
    pub image_base: u64,
    pub sections: Vec<(u64, u64)>,
    pub region_linearize_structuring: bool,
    pub force_linear_structuring: bool,
    #[serde(default)]
    pub structuring_engine: StructuringEngineKind,
    #[serde(default)]
    pub conservative_irreducible_fallback: bool,
    /// Address → symbol name for IAT slots and global data symbols.
    /// Used to replace `DAT_<addr>` with the actual symbol name in decompiled output.
    #[serde(default)]
    pub global_names: HashMap<u64, String>,
    /// Calling convention used to identify parameter registers.
    /// Auto-detected from binary format in `from_loaded_binary`; can be overridden.
    #[serde(default)]
    pub calling_convention: CallingConvention,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FormatFamily {
    Pe,
    Elf,
    MachO,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum AdmissionClass {
    PreviewUnsupported,
    PeX86PreviewOnly,
    PeX64Auto,
    GenericPreviewOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StructuringBudgetClass {
    None,
    PeX86Conditional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum StructuringEngineKind {
    LegacyScored,
    #[default]
    GraphCollapseV1,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirAdmissionFacts {
    pub block_count: usize,
    pub op_count: usize,
    pub max_multiequal_fanin: usize,
}

impl NirAdmissionFacts {
    pub fn from_pcode(pcode: &PcodeFunction) -> Self {
        Self {
            block_count: pcode.blocks.len(),
            op_count: pcode.blocks.iter().map(|block| block.ops.len()).sum(),
            max_multiequal_fanin: pcode
                .blocks
                .iter()
                .flat_map(|block| block.ops.iter())
                .filter(|op| op.opcode == PcodeOpcode::MultiEqual)
                .map(|op| op.inputs.len())
                .max()
                .unwrap_or(0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TargetProfile {
    pub format_family: FormatFamily,
    pub pointer_width: u32,
    pub admission_class: AdmissionClass,
    pub structuring_budget_class: StructuringBudgetClass,
    pub worker_eligible: bool,
    pub preview_eligible: bool,
}

impl TargetProfile {
    pub fn from_binary(binary: &LoadedBinary, pe_format_gate_enabled: bool) -> Self {
        Self::from_format(
            &binary.format,
            if binary.is_64bit { 64 } else { 32 },
            pe_format_gate_enabled,
        )
    }

    pub fn from_options(options: &NirRenderOptions) -> Self {
        Self::from_format(
            &options.format,
            options.pointer_size.saturating_mul(8),
            options.pe_x64_only,
        )
    }

    pub fn from_format(format: &str, pointer_width: u32, pe_format_gate_enabled: bool) -> Self {
        let format_upper = format.to_ascii_uppercase();
        let format_family = if format_upper.starts_with("PE") {
            FormatFamily::Pe
        } else if format_upper.starts_with("ELF") {
            FormatFamily::Elf
        } else if format_upper.starts_with("MACHO") || format_upper.starts_with("MACH-O") {
            FormatFamily::MachO
        } else {
            FormatFamily::Other
        };

        let preview_eligible = !pe_format_gate_enabled || format_family == FormatFamily::Pe;
        let worker_eligible =
            preview_eligible && format_family == FormatFamily::Pe && pointer_width == 64;
        let structuring_budget_class =
            if preview_eligible && format_family == FormatFamily::Pe && pointer_width == 32 {
                StructuringBudgetClass::PeX86Conditional
            } else {
                StructuringBudgetClass::None
            };
        let admission_class = match (preview_eligible, format_family, pointer_width) {
            (false, _, _) => AdmissionClass::PreviewUnsupported,
            (true, FormatFamily::Pe, 64) => AdmissionClass::PeX64Auto,
            (true, FormatFamily::Pe, 32) => AdmissionClass::PeX86PreviewOnly,
            (true, _, _) => AdmissionClass::GenericPreviewOnly,
        };

        Self {
            format_family,
            pointer_width,
            admission_class,
            structuring_budget_class,
            worker_eligible,
            preview_eligible,
        }
    }

    pub fn auto_admission_eligible(self, facts: NirAdmissionFacts) -> bool {
        self.worker_eligible
            && facts.block_count <= 12
            && facts.op_count <= 600
            && facts.max_multiequal_fanin <= 4
    }

    pub fn if_lowering_budget_enabled(self) -> bool {
        matches!(
            self.structuring_budget_class,
            StructuringBudgetClass::PeX86Conditional
        )
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirTypeContext {
    pub call_targets: HashMap<u64, String>,
    pub call_target_refs: HashMap<u64, CallTargetRef>,
    pub call_effect_summaries: HashMap<String, NirCallEffectSummary>,
    pub call_param_rules: Vec<NirCallParamRule>,
    pub function_hints: Option<NirFunctionHints>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirHintStats {
    pub explicit_param_name_hits: usize,
    pub explicit_local_name_hits: usize,
    pub explicit_param_type_hits: usize,
    pub explicit_local_type_hits: usize,
    pub explicit_return_type_hit: usize,
    pub pointer_alias_hits: usize,
    pub local_surface_hits: usize,
    pub derived_origin_type_hits: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirCallParamRule {
    pub callee_address: Option<u64>,
    pub callee_name: String,
    pub arg_index: usize,
    pub pointer_alias: String,
    pub pointee_alias: String,
    pub pointer_size: u32,
    pub pointee_sizes: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirFunctionHints {
    pub param_names: Vec<String>,
    pub param_type_names: HashMap<usize, String>,
    pub stack_local_names: HashMap<i64, String>,
    pub stack_local_type_names: HashMap<i64, String>,
    pub return_type_name: Option<String>,
}

impl NirRenderOptions {
    pub fn from_loaded_binary(binary: &LoadedBinary) -> Self {
        let inner = binary.inner();
        let sections = inner
            .sections
            .iter()
            .map(|section| {
                (
                    section.virtual_address,
                    section.virtual_address + section.virtual_size as u64,
                )
            })
            .collect();

        let mut global_names = inner.iat_symbols.clone();
        for (addr, name) in &inner.global_symbols {
            global_names.entry(*addr).or_insert_with(|| name.clone());
        }

        // Detect calling convention from binary format.
        // PE (Windows) uses Windows x64 fastcall; ELF and Mach-O use System V AMD64.
        let fmt_upper = binary.format.to_ascii_uppercase();
        let calling_convention = if fmt_upper.starts_with("ELF") || fmt_upper.starts_with("MACHO") {
            CallingConvention::SystemVAmd64
        } else {
            CallingConvention::WindowsX64
        };

        Self {
            pe_x64_only: true,
            is_64bit: binary.is_64bit,
            pointer_size: if binary.is_64bit { 8 } else { 4 },
            format: binary.format.clone(),
            image_base: inner.image_base,
            sections,
            region_linearize_structuring: false,
            force_linear_structuring: false,
            structuring_engine: StructuringEngineKind::GraphCollapseV1,
            conservative_irreducible_fallback: false,
            global_names,
            calling_convention,
        }
    }

    pub fn target_profile(&self) -> TargetProfile {
        TargetProfile::from_options(self)
    }

    pub fn effective_structuring_engine(&self) -> StructuringEngineKind {
        match std::env::var("FISSION_STRUCTURING_ENGINE")
            .ok()
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("graph")
            | Some("graphcollapsev1")
            | Some("graph_collapse_v1")
            | Some("graph-collapse-v1") => StructuringEngineKind::GraphCollapseV1,
            Some("legacy")
            | Some("legacyscored")
            | Some("legacy_scored")
            | Some("legacy-scored") => StructuringEngineKind::GraphCollapseV1,
            _ => self.structuring_engine,
        }
    }

    pub(super) fn is_mapped_global(&self, address: u64) -> bool {
        self.sections
            .iter()
            .any(|(start, end)| address >= *start && address < *end)
    }
}

pub type PreviewBuildStats = NirBuildStats;
pub type MlilPreviewOptions = NirRenderOptions;
pub type PreviewTypeContext = NirTypeContext;
pub type PreviewHintStats = NirHintStats;
pub type PreviewCallParamRule = NirCallParamRule;
pub type PreviewFunctionHints = NirFunctionHints;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructuringFailureKind {
    RegionShape,
    PhiJoin,
    IndirectCallRegion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RecoveryMode {
    Structured,
    RegionLinearized,
    ForcedLinear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StructuringReasonFamily {
    RegionLegality,
    FollowFailure,
    Irreducible,
    LoopExit,
    SwitchShape,
    Budget,
}

impl StructuringReasonFamily {
    pub const fn as_str(self) -> &'static str {
        match self {
            StructuringReasonFamily::RegionLegality => "region_legality",
            StructuringReasonFamily::FollowFailure => "follow_failure",
            StructuringReasonFamily::Irreducible => "irreducible",
            StructuringReasonFamily::LoopExit => "loop_exit",
            StructuringReasonFamily::SwitchShape => "switch_shape",
            StructuringReasonFamily::Budget => "budget",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StructuringOutcome {
    pub mode: RecoveryMode,
    pub reason_family: StructuringReasonFamily,
    pub retryable: bool,
    pub confidence: u8,
}

pub fn parse_call_target_address(target: &str) -> Option<u64> {
    for prefix in ["sub_", "FUN_0x", "FUN_", "DAT_0x", "DAT_"] {
        if let Some(rest) = target.strip_prefix(prefix) {
            return u64::from_str_radix(rest.trim_start_matches("0x"), 16).ok();
        }
    }
    None
}

pub fn structuring_outcome_for_signature(signature: &str) -> Option<StructuringOutcome> {
    let family = match signature {
        "unsupported_cfg_region_shape" | "unsupported_cfg_phi_join" => {
            StructuringReasonFamily::RegionLegality
        }
        "unsupported_cfg_indirect_call_region" => StructuringReasonFamily::FollowFailure,
        _ => return None,
    };
    Some(StructuringOutcome {
        mode: RecoveryMode::RegionLinearized,
        reason_family: family,
        retryable: true,
        confidence: 224,
    })
}

impl StructuringFailureKind {
    pub const fn preview_block_signature(self) -> &'static str {
        match self {
            StructuringFailureKind::RegionShape => "unsupported_cfg_region_shape",
            StructuringFailureKind::PhiJoin => "unsupported_cfg_phi_join",
            StructuringFailureKind::IndirectCallRegion => "unsupported_cfg_indirect_call_region",
        }
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum MlilPreviewError {
    #[error("mlil-preview currently supports PE x64 only")]
    UnsupportedArchitecture,
    #[error("unsupported architecture in mlil-preview")]
    UnsupportedArchitectureDetailed,
    #[error("unsupported control flow in mlil-preview")]
    UnsupportedControlFlow,
    #[error("unsupported branch target in mlil-preview")]
    UnsupportedCfgBranchTarget,
    #[error("unsupported region shape in mlil-preview")]
    UnsupportedCfgRegionShape,
    #[error("unsupported phi join in mlil-preview")]
    UnsupportedCfgPhiJoin,
    #[error("unsupported indirect call region in mlil-preview")]
    UnsupportedCfgIndirectCallRegion,
    #[error("unsupported pcode pattern: {0}")]
    UnsupportedPattern(&'static str),
    #[error("value lowering failed")]
    LoweringFailed,
    #[error("value lowering failed on multiequal")]
    UnsupportedExprMultiequal,
    #[error("value lowering failed on varnode")]
    UnsupportedExprVarnodeLowering,
    #[error("value lowering failed on varnode: unsupported address materialization")]
    UnsupportedExprAddressMaterialization,
    #[error("value lowering failed on varnode: unsupported indirect value source")]
    UnsupportedExprIndirectValueSource,
    #[error("value lowering failed on varnode: unsupported piece/subpiece shape")]
    UnsupportedExprPieceShape,
    #[error("value lowering failed on varnode: unsupported ptr arithmetic shape")]
    UnsupportedExprPtrArithmetic,
    #[error("value lowering failed on varnode: unsupported memory-backed varnode")]
    UnsupportedExprMemoryBackedVarnode,
}

impl MlilPreviewError {
    pub const fn structuring_failure_kind(self) -> Option<StructuringFailureKind> {
        match self {
            MlilPreviewError::UnsupportedCfgRegionShape => {
                Some(StructuringFailureKind::RegionShape)
            }
            MlilPreviewError::UnsupportedCfgPhiJoin => Some(StructuringFailureKind::PhiJoin),
            MlilPreviewError::UnsupportedCfgIndirectCallRegion => {
                Some(StructuringFailureKind::IndirectCallRegion)
            }
            _ => None,
        }
    }
}
