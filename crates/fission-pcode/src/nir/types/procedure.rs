use super::*;

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
    Export,
    ExportThunkTarget,
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
    /// `may_exit = true` was derived from Ghidra's no-return fact data
    /// (`noReturnFunctionConstraints.xml`, `*FunctionsThatDoNotReturn`, or
    /// DLL-specific `.hints` files in `utils/ghidra-data`).
    GhidraNoReturnData,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NirCallPrototypeSummary {
    pub min_arity: usize,
    pub max_arity: usize,
    pub locked_exact_arity: Option<usize>,
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
    /// ABI used for entry promotion and variadic heuristics (mirrors preview options).
    pub calling_convention: CallingConvention,
    /// Integer param register offsets from `.cspec` (mirrors preview options at HIR build).
    pub int_param_offsets: Vec<u64>,
    /// When false, x64-only normalize passes (entry param promotion, etc.) are skipped.
    pub is_64bit: bool,
    /// When true, entry-register reads should stay as hardware registers rather than ABI params.
    pub suppress_entry_register_params: bool,
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
            int_param_offsets: Vec::new(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: IndexMap::new(),
            callee_summaries: IndexMap::new(),
        }
    }
}
