//! Cross-pass structural IR invariant validation and typed violation diagnostics.
//!
//! # Contract & Scope
//!
//! Structural validation verifies machine-checkable graph, SSA, scoping, label,
//! control-exit, and representation invariants close to the pass that produced
//! them. **Structural well-formedness is necessary but never sufficient for
//! semantic or effect preservation**: passing every check in this module never
//! proves behavioral equivalence or that side effects are preserved.

use super::{
    CallSummary, HirExpr, HirFunction, HirLValue, HirStmt, NirType, ScalarSsaShapeError,
    SsaHighVariableId, SsaOpSite, SsaStorageKey, SsaUseSite, SsaValueId,
};
use crate::labels::SWITCH_FALLTHROUGH_SENTINEL;
use indexmap::IndexMap;
use std::collections::BTreeSet;
use std::fmt;

/// Maximum UTF-8 byte length of a serialized reproduction snippet stored inside
/// [`StructuralViolation::repro_summary`].
pub const MAX_REPRO_SUMMARY_BYTES: usize = 512;

/// Deterministically truncate `raw` on a UTF-8 char boundary to fit within
/// [`MAX_REPRO_SUMMARY_BYTES`].
#[must_use]
pub fn bound_repro_summary(raw: &str) -> String {
    if raw.len() <= MAX_REPRO_SUMMARY_BYTES {
        return raw.to_string();
    }
    const SUFFIX: &str = "...";
    let limit = MAX_REPRO_SUMMARY_BYTES.saturating_sub(SUFFIX.len());
    let mut end = 0;
    for (idx, ch) in raw.char_indices() {
        let next = idx + ch.len_utf8();
        if next > limit {
            break;
        }
        end = next;
    }
    let mut out = String::with_capacity(end + SUFFIX.len());
    out.push_str(&raw[..end]);
    out.push_str(SUFFIX);
    out
}

/// Canonical IR stage at which structural validation runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IrStage {
    Pcode,
    ScalarSsa,
    SsaEmission,
    CfgStructuring,
    PreHir,
    Hir,
}

impl fmt::Display for IrStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Pcode => "pcode",
            Self::ScalarSsa => "scalar_ssa",
            Self::SsaEmission => "ssa_emission",
            Self::CfgStructuring => "cfg_structuring",
            Self::PreHir => "prehir",
            Self::Hir => "hir",
        };
        f.write_str(name)
    }
}

/// Individual structural invariant families checked across midend stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IrInvariant {
    CfgEdgeSymmetry,
    SsaShapeAndDominance,
    PhiEdgeCoverage,
    StorageAccessPieces,
    ParallelCopySchedule,
    BindingScope,
    LabelGotoResolution,
    StructuredControlExits,
    EffectRepresentation,
}

impl IrInvariant {
    pub const ALL: [Self; 9] = [
        Self::CfgEdgeSymmetry,
        Self::SsaShapeAndDominance,
        Self::PhiEdgeCoverage,
        Self::StorageAccessPieces,
        Self::ParallelCopySchedule,
        Self::BindingScope,
        Self::LabelGotoResolution,
        Self::StructuredControlExits,
        Self::EffectRepresentation,
    ];

    const fn bit(self) -> u32 {
        match self {
            Self::CfgEdgeSymmetry => 1 << 0,
            Self::SsaShapeAndDominance => 1 << 1,
            Self::PhiEdgeCoverage => 1 << 2,
            Self::StorageAccessPieces => 1 << 3,
            Self::ParallelCopySchedule => 1 << 4,
            Self::BindingScope => 1 << 5,
            Self::LabelGotoResolution => 1 << 6,
            Self::StructuredControlExits => 1 << 7,
            Self::EffectRepresentation => 1 << 8,
        }
    }
}

impl fmt::Display for IrInvariant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::CfgEdgeSymmetry => "cfg_edge_symmetry",
            Self::SsaShapeAndDominance => "ssa_shape_and_dominance",
            Self::PhiEdgeCoverage => "phi_edge_coverage",
            Self::StorageAccessPieces => "storage_access_pieces",
            Self::ParallelCopySchedule => "parallel_copy_schedule",
            Self::BindingScope => "binding_scope",
            Self::LabelGotoResolution => "label_goto_resolution",
            Self::StructuredControlExits => "structured_control_exits",
            Self::EffectRepresentation => "effect_representation",
        };
        f.write_str(name)
    }
}

/// Bitset of [`IrInvariant`] flags usable in `const` pass-contract declarations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct IrInvariantSet(u32);

impl IrInvariantSet {
    pub const EMPTY: Self = Self(0);

    pub const SCALAR_SSA_STAGE: Self = Self(
        IrInvariant::CfgEdgeSymmetry.bit()
            | IrInvariant::SsaShapeAndDominance.bit()
            | IrInvariant::PhiEdgeCoverage.bit()
            | IrInvariant::StorageAccessPieces.bit()
            | IrInvariant::EffectRepresentation.bit(),
    );

    pub const STRUCTURING_CFG: Self = Self(IrInvariant::CfgEdgeSymmetry.bit());

    pub const STRUCTURING_OUTPUT: Self = Self(
        IrInvariant::CfgEdgeSymmetry.bit()
            | IrInvariant::LabelGotoResolution.bit()
            | IrInvariant::StructuredControlExits.bit()
            | IrInvariant::EffectRepresentation.bit(),
    );

    pub const PREHIR_STAGE: Self = Self(
        IrInvariant::BindingScope.bit()
            | IrInvariant::LabelGotoResolution.bit()
            | IrInvariant::StructuredControlExits.bit()
            | IrInvariant::EffectRepresentation.bit(),
    );

    pub const HIR_STAGE: Self = Self(
        IrInvariant::BindingScope.bit()
            | IrInvariant::LabelGotoResolution.bit()
            | IrInvariant::StructuredControlExits.bit()
            | IrInvariant::EffectRepresentation.bit(),
    );

    #[must_use]
    pub const fn single(inv: IrInvariant) -> Self {
        Self(inv.bit())
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    #[must_use]
    pub const fn contains(self, inv: IrInvariant) -> bool {
        (self.0 & inv.bit()) != 0
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub const fn is_superset(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn iter(self) -> impl Iterator<Item = IrInvariant> {
        IrInvariant::ALL
            .into_iter()
            .filter(move |inv| self.contains(*inv))
    }
}

/// Pre/post structural contract declared by a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PassContract {
    pub required: IrInvariantSet,
    pub guaranteed: IrInvariantSet,
    /// When `true`, running the pass must not invalidate cached CFG analyses.
    pub preserves_cfg_analyses: bool,
}

impl PassContract {
    #[must_use]
    pub const fn new(
        required: IrInvariantSet,
        guaranteed: IrInvariantSet,
        preserves_cfg_analyses: bool,
    ) -> Self {
        Self {
            required,
            guaranteed,
            preserves_cfg_analyses,
        }
    }

    #[must_use]
    pub const fn default_prehir() -> Self {
        Self::new(
            IrInvariantSet::PREHIR_STAGE,
            IrInvariantSet::PREHIR_STAGE,
            true,
        )
    }

    #[must_use]
    pub const fn default_cfg_preserving() -> Self {
        Self::new(
            IrInvariantSet::STRUCTURING_CFG,
            IrInvariantSet::STRUCTURING_CFG,
            true,
        )
    }

    #[must_use]
    pub const fn default_cfg_mutating() -> Self {
        Self::new(
            IrInvariantSet::STRUCTURING_CFG,
            IrInvariantSet::STRUCTURING_CFG,
            false,
        )
    }
}

/// Controls how aggressively cross-pass validation runs.
///
/// Configured via `FISSION_VERIFY_IR`:
/// - unset / `"boundary"` / `"stage"`: fast stage-boundary checks only
/// - `"each"` / `"all"` / `"1"` / `"true"`: verify every pass pre/post contract
/// - `"off"` / `"0"` / `"false"`: skip structural verification
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValidationMode {
    Off,
    StageBoundary,
    VerifyEach,
}

impl ValidationMode {
    #[must_use]
    pub fn from_env() -> Self {
        match std::env::var("FISSION_VERIFY_IR") {
            Ok(raw) => Self::parse(&raw),
            Err(_) => Self::StageBoundary,
        }
    }

    #[must_use]
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "off" | "0" | "false" | "none" => Self::Off,
            "each" | "all" | "1" | "true" | "verify-each" | "verify_each" => Self::VerifyEach,
            _ => Self::StageBoundary,
        }
    }
}

/// Canonical site locator reusing existing midend IR identifiers.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValidationSite {
    Function { name: String },
    Block { block: u32 },
    Edge { from: u32, to: u32 },
    SsaOp(SsaOpSite),
    SsaUse(SsaUseSite),
    SsaValue(SsaValueId),
    SsaStorage(SsaStorageKey),
    SsaHighVariable(SsaHighVariableId),
    StmtPath { path: Vec<u32>, kind: &'static str },
    Binding { name: String },
    Label { name: String },
    CalleeSummary { symbol: String },
}

impl fmt::Display for ValidationSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Function { name } => write!(f, "fn:{name}"),
            Self::Block { block } => write!(f, "block:{block}"),
            Self::Edge { from, to } => write!(f, "edge:{from}->{to}"),
            Self::SsaOp(site) => write!(f, "op:{}:{}", site.block, site.op),
            Self::SsaUse(site) => write!(f, "use:{}:{}:{}", site.block, site.op, site.input),
            Self::SsaValue(id) => write!(f, "val:v{}", id.0),
            Self::SsaStorage(key) => {
                write!(
                    f,
                    "storage:{}:0x{:x}:{}",
                    key.space_id, key.offset, key.size
                )
            }
            Self::SsaHighVariable(id) => write!(f, "high_var:hv{}", id.0),
            Self::StmtPath { path, kind } => {
                write!(f, "stmt:{kind}@")?;
                for (i, idx) in path.iter().enumerate() {
                    if i > 0 {
                        f.write_str(".")?;
                    }
                    write!(f, "{idx}")?;
                }
                Ok(())
            }
            Self::Binding { name } => write!(f, "binding:{name}"),
            Self::Label { name } => write!(f, "label:{name}"),
            Self::CalleeSummary { symbol } => write!(f, "callee:{symbol}"),
        }
    }
}

/// One typed structural invariant failure anchored to pass, stage, revision,
/// and site, with a bounded deterministic repro snippet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralViolation {
    pub stage: IrStage,
    pub pass: String,
    pub revision: u64,
    pub invariant: IrInvariant,
    pub site: ValidationSite,
    pub message: String,
    pub repro_summary: String,
}

impl StructuralViolation {
    #[must_use]
    pub fn new(
        stage: IrStage,
        pass: impl Into<String>,
        revision: u64,
        invariant: IrInvariant,
        site: ValidationSite,
        message: impl Into<String>,
        repro_raw: &str,
    ) -> Self {
        Self {
            stage,
            pass: pass.into(),
            revision,
            invariant,
            site,
            message: message.into(),
            repro_summary: bound_repro_summary(repro_raw),
        }
    }
}

impl fmt::Display for StructuralViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[stage={} pass={} rev={} inv={} site={}] {}",
            self.stage, self.pass, self.revision, self.invariant, self.site, self.message
        )?;
        if !self.repro_summary.is_empty() {
            write!(f, " | repro={}", self.repro_summary)?;
        }
        Ok(())
    }
}

/// Result of running structural validation at a pass or stage boundary.
///
/// Structural well-formedness is necessary but never sufficient for semantic
/// preservation: [`Self::proves_semantic_equivalence`] is unconditionally
/// `false`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralValidationReport {
    pub stage: IrStage,
    pub pass: String,
    pub revision: u64,
    pub checked_invariants: IrInvariantSet,
    pub violations: Vec<StructuralViolation>,
    proves_semantic_equivalence: bool,
}

impl StructuralValidationReport {
    #[must_use]
    pub fn new(
        stage: IrStage,
        pass: impl Into<String>,
        revision: u64,
        checked_invariants: IrInvariantSet,
        violations: Vec<StructuralViolation>,
    ) -> Self {
        Self {
            stage,
            pass: pass.into(),
            revision,
            checked_invariants,
            violations,
            proves_semantic_equivalence: false,
        }
    }

    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.violations.is_empty()
    }

    /// Always `false`: structural checks never prove behavioral or effect
    /// preservation.
    #[must_use]
    pub const fn proves_semantic_equivalence(&self) -> bool {
        self.proves_semantic_equivalence
    }

    #[must_use]
    pub fn into_result(self) -> Result<(), Self> {
        if self.violations.is_empty() {
            Ok(())
        } else {
            Err(self)
        }
    }
}

impl fmt::Display for StructuralValidationReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(first) = self.violations.first() {
            write!(
                f,
                "{} ({} violation(s); semantic_equivalence_proven={})",
                first,
                self.violations.len(),
                self.proves_semantic_equivalence
            )
        } else {
            write!(
                f,
                "ok [stage={} pass={} rev={} semantic_equivalence_proven={}]",
                self.stage, self.pass, self.revision, self.proves_semantic_equivalence
            )
        }
    }
}

impl std::error::Error for StructuralValidationReport {}

/// Convert a [`ScalarSsaShapeError`] into the common [`StructuralViolation`] contract.
#[must_use]
pub fn scalar_ssa_shape_error_to_violation(
    err: &ScalarSsaShapeError,
    pass: &str,
    revision: u64,
) -> StructuralViolation {
    let (invariant, site) = match err {
        ScalarSsaShapeError::NonDenseValue { value, .. } => (
            IrInvariant::SsaShapeAndDominance,
            ValidationSite::SsaValue(*value),
        ),
        ScalarSsaShapeError::UnknownValueReference(id) => (
            IrInvariant::SsaShapeAndDominance,
            ValidationSite::SsaValue(*id),
        ),
        ScalarSsaShapeError::MissingAddressSpaceGuard { storage }
        | ScalarSsaShapeError::StorageOutsideGuard { storage }
        | ScalarSsaShapeError::InputDefinitionMismatch { storage, .. } => (
            IrInvariant::StorageAccessPieces,
            ValidationSite::SsaStorage(*storage),
        ),
        ScalarSsaShapeError::OperationDefinitionMismatch { site, .. }
        | ScalarSsaShapeError::MalformedOperationOutputPieces { site } => (
            IrInvariant::StorageAccessPieces,
            ValidationSite::SsaOp(*site),
        ),
        ScalarSsaShapeError::MalformedOperationInputPieces { site } => (
            IrInvariant::StorageAccessPieces,
            ValidationSite::SsaUse(*site),
        ),
        ScalarSsaShapeError::PhiDefinitionMismatch { block, .. }
        | ScalarSsaShapeError::PhiOutputStorageMismatch { block, .. }
        | ScalarSsaShapeError::UnsortedPhiStorage { block }
        | ScalarSsaShapeError::UnsortedPhiPredecessors { block, .. } => (
            IrInvariant::PhiEdgeCoverage,
            ValidationSite::Block { block: *block },
        ),
        ScalarSsaShapeError::MalformedDynamicGuard { site } => (
            IrInvariant::EffectRepresentation,
            ValidationSite::SsaOp(*site),
        ),
        ScalarSsaShapeError::NonDenseMemoryValue { .. }
        | ScalarSsaShapeError::UnknownMemoryValueReference(_)
        | ScalarSsaShapeError::MalformedMemorySsa => (
            IrInvariant::EffectRepresentation,
            ValidationSite::Function {
                name: pass.to_string(),
            },
        ),
        ScalarSsaShapeError::UnsortedOutOfSsaCopies => (
            IrInvariant::ParallelCopySchedule,
            ValidationSite::Function {
                name: pass.to_string(),
            },
        ),
        ScalarSsaShapeError::InvalidOutOfSsaCopy { copy } => (
            IrInvariant::ParallelCopySchedule,
            ValidationSite::Edge {
                from: copy.predecessor,
                to: copy.successor,
            },
        ),
        ScalarSsaShapeError::HighVariableMapLength { .. } => (
            IrInvariant::SsaShapeAndDominance,
            ValidationSite::Function {
                name: pass.to_string(),
            },
        ),
        ScalarSsaShapeError::NonDenseHighVariable { high, .. }
        | ScalarSsaShapeError::MalformedHighVariable { high } => (
            IrInvariant::SsaShapeAndDominance,
            ValidationSite::SsaHighVariable(*high),
        ),
        ScalarSsaShapeError::HighVariableMembershipMismatch { value, .. }
        | ScalarSsaShapeError::MissingHighVariableMember { value } => (
            IrInvariant::SsaShapeAndDominance,
            ValidationSite::SsaValue(*value),
        ),
    };
    let repro = format!("{err:?}");
    StructuralViolation::new(
        IrStage::ScalarSsa,
        pass,
        revision,
        invariant,
        site,
        format!("scalar SSA shape error: {err:?}"),
        &repro,
    )
}

/// Validate CFG vector lengths, out-of-bounds edges, and successor/predecessor
/// symmetry.
#[must_use]
pub fn validate_cfg_edges(
    block_count: usize,
    successors: &[Vec<usize>],
    predecessors: &[Vec<usize>],
    stage: IrStage,
    pass: &str,
    revision: u64,
) -> Vec<StructuralViolation> {
    let mut violations = Vec::new();
    if successors.len() != block_count {
        violations.push(StructuralViolation::new(
            stage,
            pass,
            revision,
            IrInvariant::CfgEdgeSymmetry,
            ValidationSite::Function {
                name: pass.to_string(),
            },
            format!(
                "successor vector length {} does not match block count {}",
                successors.len(),
                block_count
            ),
            &format!("block_count={block_count} succ_len={}", successors.len()),
        ));
    }
    if predecessors.len() != block_count {
        violations.push(StructuralViolation::new(
            stage,
            pass,
            revision,
            IrInvariant::CfgEdgeSymmetry,
            ValidationSite::Function {
                name: pass.to_string(),
            },
            format!(
                "predecessor vector length {} does not match block count {}",
                predecessors.len(),
                block_count
            ),
            &format!("block_count={block_count} pred_len={}", predecessors.len()),
        ));
    }
    if !violations.is_empty() {
        return violations;
    }

    for (from, succs) in successors.iter().enumerate() {
        for &to in succs {
            if to >= block_count {
                violations.push(StructuralViolation::new(
                    stage,
                    pass,
                    revision,
                    IrInvariant::CfgEdgeSymmetry,
                    ValidationSite::Edge {
                        from: from as u32,
                        to: to as u32,
                    },
                    format!("successor {to} out of bounds for block_count {block_count}"),
                    &format!("edge {from}->{to} out of bounds"),
                ));
            } else if !predecessors[to].contains(&from) {
                violations.push(StructuralViolation::new(
                    stage,
                    pass,
                    revision,
                    IrInvariant::CfgEdgeSymmetry,
                    ValidationSite::Edge {
                        from: from as u32,
                        to: to as u32,
                    },
                    format!(
                        "missing inverse predecessor edge: {from}->{to} absent from predecessors[{to}]"
                    ),
                    &format!("succs[{from}]={succs:?} preds[{to}]={:?}", predecessors[to]),
                ));
            }
        }
    }

    for (to, preds) in predecessors.iter().enumerate() {
        for &from in preds {
            if from >= block_count {
                violations.push(StructuralViolation::new(
                    stage,
                    pass,
                    revision,
                    IrInvariant::CfgEdgeSymmetry,
                    ValidationSite::Edge {
                        from: from as u32,
                        to: to as u32,
                    },
                    format!("predecessor {from} out of bounds for block_count {block_count}"),
                    &format!("pred {from}->{to} out of bounds"),
                ));
            } else if !successors[from].contains(&to) {
                violations.push(StructuralViolation::new(
                    stage,
                    pass,
                    revision,
                    IrInvariant::CfgEdgeSymmetry,
                    ValidationSite::Edge {
                        from: from as u32,
                        to: to as u32,
                    },
                    format!(
                        "missing inverse successor edge: {from}->{to} absent from successors[{from}]"
                    ),
                    &format!("preds[{to}]={preds:?} succs[{from}]={:?}", successors[from]),
                ));
            }
        }
    }

    violations
}

/// Validate a [`NirType`] structurally (non-zero integer bitwidths, recognized
/// float bitwidths, recursive pointee/aggregate field types).
pub fn validate_nir_type(
    ty: &NirType,
    stage: IrStage,
    pass: &str,
    revision: u64,
    site: &ValidationSite,
    violations: &mut Vec<StructuralViolation>,
) {
    match ty {
        NirType::Unknown | NirType::Bool => {}
        NirType::Int { bits, .. } => {
            if *bits == 0 {
                violations.push(StructuralViolation::new(
                    stage,
                    pass,
                    revision,
                    IrInvariant::EffectRepresentation,
                    site.clone(),
                    "integer type has zero bitwidth",
                    &format!("{ty:?}"),
                ));
            }
        }
        NirType::Float { bits } => {
            if !matches!(*bits, 16 | 32 | 64 | 80 | 128) {
                violations.push(StructuralViolation::new(
                    stage,
                    pass,
                    revision,
                    IrInvariant::EffectRepresentation,
                    site.clone(),
                    format!("float type has unsupported bitwidth {bits}"),
                    &format!("{ty:?}"),
                ));
            }
        }
        NirType::Ptr(inner) => validate_nir_type(inner, stage, pass, revision, site, violations),
        NirType::Aggregate { fields, .. } => {
            for field in fields {
                validate_nir_type(&field.ty, stage, pass, revision, site, violations);
            }
        }
    }
}

/// Validate mechanically checkable invariants on per-callee effect & prototype
/// summaries without claiming effect preservation.
pub fn validate_callee_summaries(
    summaries: &IndexMap<String, CallSummary>,
    stage: IrStage,
    pass: &str,
    revision: u64,
    violations: &mut Vec<StructuralViolation>,
) {
    for (key, summary) in summaries {
        let site = ValidationSite::CalleeSummary {
            symbol: key.clone(),
        };
        if key.is_empty() || summary.target.symbol.is_empty() {
            violations.push(StructuralViolation::new(
                stage,
                pass,
                revision,
                IrInvariant::EffectRepresentation,
                site.clone(),
                "callee summary has empty key or target symbol",
                &format!("key={key:?} sym={:?}", summary.target.symbol),
            ));
        }
        let proto = &summary.prototype;
        if proto.min_arity > proto.max_arity {
            violations.push(StructuralViolation::new(
                stage,
                pass,
                revision,
                IrInvariant::EffectRepresentation,
                site.clone(),
                format!(
                    "callee prototype min_arity ({}) exceeds max_arity ({})",
                    proto.min_arity, proto.max_arity
                ),
                &format!(
                    "callee={key} min={} max={}",
                    proto.min_arity, proto.max_arity
                ),
            ));
        }
        if let Some(exact) = proto.locked_exact_arity
            && (exact < proto.min_arity || exact > proto.max_arity)
        {
            violations.push(StructuralViolation::new(
                stage,
                pass,
                revision,
                IrInvariant::EffectRepresentation,
                site.clone(),
                format!(
                    "callee prototype locked_exact_arity ({exact}) outside [{}, {}]",
                    proto.min_arity, proto.max_arity
                ),
                &format!(
                    "callee={key} exact={exact} min={} max={}",
                    proto.min_arity, proto.max_arity
                ),
            ));
        }
        if let Some(variadic) = proto.variadic_fixed_arity
            && variadic > proto.max_arity
        {
            violations.push(StructuralViolation::new(
                stage,
                pass,
                revision,
                IrInvariant::EffectRepresentation,
                site.clone(),
                format!(
                    "callee prototype variadic_fixed_arity ({variadic}) exceeds max_arity ({})",
                    proto.max_arity
                ),
                &format!("callee={key} variadic={variadic} max={}", proto.max_arity),
            ));
        }
        validate_nir_type(
            &proto.return_lattice,
            stage,
            pass,
            revision,
            &site,
            violations,
        );
        for param_ty in &proto.param_lattices {
            validate_nir_type(param_ty, stage, pass, revision, &site, violations);
        }
    }
}

/// Check mechanically verifiable structural invariants on a [`HirFunction`].
pub fn validate_hir_function(
    func: &HirFunction,
    invariants: IrInvariantSet,
    pass: &str,
    revision: u64,
) -> Result<(), StructuralValidationReport> {
    let mut violations = Vec::new();
    let mut declared = BTreeSet::new();

    if invariants.contains(IrInvariant::BindingScope) {
        let mut seen_params = BTreeSet::new();
        for param in &func.params {
            if param.name.is_empty() {
                violations.push(StructuralViolation::new(
                    IrStage::Hir,
                    pass,
                    revision,
                    IrInvariant::BindingScope,
                    ValidationSite::Function {
                        name: func.name.clone(),
                    },
                    "parameter binding has empty name",
                    &format!("fn={} params={}", func.name, func.params.len()),
                ));
            } else {
                if !seen_params.insert(param.name.as_str()) {
                    violations.push(StructuralViolation::new(
                        IrStage::Hir,
                        pass,
                        revision,
                        IrInvariant::BindingScope,
                        ValidationSite::Binding {
                            name: param.name.clone(),
                        },
                        format!("duplicate parameter binding '{}'", param.name),
                        &format!("fn={} duplicate param {}", func.name, param.name),
                    ));
                }
                declared.insert(param.name.as_str());
            }
        }
        let mut seen_locals = BTreeSet::new();
        for local in &func.locals {
            if local.name.is_empty() {
                violations.push(StructuralViolation::new(
                    IrStage::Hir,
                    pass,
                    revision,
                    IrInvariant::BindingScope,
                    ValidationSite::Function {
                        name: func.name.clone(),
                    },
                    "local binding has empty name",
                    &format!("fn={} locals={}", func.name, func.locals.len()),
                ));
            } else {
                if seen_params.contains(local.name.as_str()) {
                    violations.push(StructuralViolation::new(
                        IrStage::Hir,
                        pass,
                        revision,
                        IrInvariant::BindingScope,
                        ValidationSite::Binding {
                            name: local.name.clone(),
                        },
                        format!("local binding '{}' shadows parameter", local.name),
                        &format!("fn={} shadow {}", func.name, local.name),
                    ));
                }
                if !seen_locals.insert(local.name.as_str()) {
                    violations.push(StructuralViolation::new(
                        IrStage::Hir,
                        pass,
                        revision,
                        IrInvariant::BindingScope,
                        ValidationSite::Binding {
                            name: local.name.clone(),
                        },
                        format!("duplicate local binding '{}'", local.name),
                        &format!("fn={} duplicate local {}", func.name, local.name),
                    ));
                }
                declared.insert(local.name.as_str());
            }
        }
    }

    if invariants.contains(IrInvariant::EffectRepresentation) {
        let fn_site = ValidationSite::Function {
            name: func.name.clone(),
        };
        validate_nir_type(
            &func.return_type,
            IrStage::Hir,
            pass,
            revision,
            &fn_site,
            &mut violations,
        );
        for param in &func.params {
            let site = ValidationSite::Binding {
                name: param.name.clone(),
            };
            validate_nir_type(
                &param.ty,
                IrStage::Hir,
                pass,
                revision,
                &site,
                &mut violations,
            );
        }
        for local in &func.locals {
            let site = ValidationSite::Binding {
                name: local.name.clone(),
            };
            validate_nir_type(
                &local.ty,
                IrStage::Hir,
                pass,
                revision,
                &site,
                &mut violations,
            );
        }
        validate_callee_summaries(
            &func.callee_summaries,
            IrStage::Hir,
            pass,
            revision,
            &mut violations,
        );
    }

    let declared_ref = invariants
        .contains(IrInvariant::BindingScope)
        .then_some(&declared);
    if let Err(mut body_report) =
        validate_hir_stmts(&func.body, invariants, declared_ref, pass, revision)
    {
        violations.append(&mut body_report.violations);
    }

    StructuralValidationReport::new(IrStage::Hir, pass, revision, invariants, violations)
        .into_result()
}

/// Check mechanically verifiable structural invariants on a slice of [`HirStmt`].
pub fn validate_hir_stmts(
    stmts: &[HirStmt],
    invariants: IrInvariantSet,
    declared_bindings: Option<&BTreeSet<&str>>,
    pass: &str,
    revision: u64,
) -> Result<(), StructuralValidationReport> {
    let mut violations = Vec::new();
    let mut defined_labels = BTreeSet::new();
    if invariants.contains(IrInvariant::LabelGotoResolution) {
        let mut path = Vec::new();
        collect_hir_labels(
            stmts,
            &mut path,
            &mut defined_labels,
            pass,
            revision,
            &mut violations,
        );
    }

    let mut path = Vec::new();
    validate_hir_stmt_list(
        stmts,
        &mut path,
        0,
        0,
        invariants,
        declared_bindings,
        &defined_labels,
        pass,
        revision,
        &mut violations,
    );

    StructuralValidationReport::new(IrStage::Hir, pass, revision, invariants, violations)
        .into_result()
}

fn collect_hir_labels<'a>(
    stmts: &'a [HirStmt],
    path: &mut Vec<u32>,
    defined_labels: &mut BTreeSet<&'a str>,
    pass: &str,
    revision: u64,
    violations: &mut Vec<StructuralViolation>,
) {
    for (idx, stmt) in stmts.iter().enumerate() {
        path.push(idx as u32);
        match stmt {
            HirStmt::Label(label) => {
                if label.is_empty() {
                    violations.push(StructuralViolation::new(
                        IrStage::Hir,
                        pass,
                        revision,
                        IrInvariant::LabelGotoResolution,
                        ValidationSite::StmtPath {
                            path: path.clone(),
                            kind: "Label",
                        },
                        "empty label name",
                        "Label(\"\")",
                    ));
                } else if !defined_labels.insert(label.as_str()) {
                    violations.push(StructuralViolation::new(
                        IrStage::Hir,
                        pass,
                        revision,
                        IrInvariant::LabelGotoResolution,
                        ValidationSite::Label {
                            name: label.clone(),
                        },
                        format!("duplicate label definition '{label}'"),
                        &format!("duplicate label {label}"),
                    ));
                }
            }
            HirStmt::Block(body) => {
                collect_hir_labels(body, path, defined_labels, pass, revision, violations);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_hir_labels(
                        &case.body,
                        path,
                        defined_labels,
                        pass,
                        revision,
                        violations,
                    );
                }
                collect_hir_labels(default, path, defined_labels, pass, revision, violations);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_hir_labels(then_body, path, defined_labels, pass, revision, violations);
                collect_hir_labels(else_body, path, defined_labels, pass, revision, violations);
            }
            HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                collect_hir_labels(body, path, defined_labels, pass, revision, violations);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init) = init {
                    collect_hir_labels(
                        std::slice::from_ref(init.as_ref()),
                        path,
                        defined_labels,
                        pass,
                        revision,
                        violations,
                    );
                }
                if let Some(update) = update {
                    collect_hir_labels(
                        std::slice::from_ref(update.as_ref()),
                        path,
                        defined_labels,
                        pass,
                        revision,
                        violations,
                    );
                }
                collect_hir_labels(body, path, defined_labels, pass, revision, violations);
            }
            HirStmt::Assign { .. }
            | HirStmt::Expr(_)
            | HirStmt::VaStart { .. }
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
        path.pop();
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_hir_stmt_list(
    stmts: &[HirStmt],
    path: &mut Vec<u32>,
    loop_depth: usize,
    switch_depth: usize,
    invariants: IrInvariantSet,
    declared_bindings: Option<&BTreeSet<&str>>,
    defined_labels: &BTreeSet<&str>,
    pass: &str,
    revision: u64,
    violations: &mut Vec<StructuralViolation>,
) {
    for (idx, stmt) in stmts.iter().enumerate() {
        path.push(idx as u32);
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                let site = ValidationSite::StmtPath {
                    path: path.clone(),
                    kind: "Assign",
                };
                validate_hir_lvalue(
                    lhs,
                    invariants,
                    declared_bindings,
                    pass,
                    revision,
                    &site,
                    violations,
                );
                validate_hir_expr(
                    rhs,
                    invariants,
                    declared_bindings,
                    pass,
                    revision,
                    &site,
                    violations,
                );
            }
            HirStmt::Expr(expr) => {
                let site = ValidationSite::StmtPath {
                    path: path.clone(),
                    kind: "Expr",
                };
                validate_hir_expr(
                    expr,
                    invariants,
                    declared_bindings,
                    pass,
                    revision,
                    &site,
                    violations,
                );
            }
            HirStmt::VaStart {
                va_list,
                last_named_param,
            } => {
                let site = ValidationSite::StmtPath {
                    path: path.clone(),
                    kind: "VaStart",
                };
                if invariants.contains(IrInvariant::BindingScope) && last_named_param.is_empty() {
                    violations.push(StructuralViolation::new(
                        IrStage::Hir,
                        pass,
                        revision,
                        IrInvariant::BindingScope,
                        site.clone(),
                        "va_start has empty last_named_param",
                        "VaStart { last_named_param: \"\" }",
                    ));
                }
                validate_hir_expr(
                    va_list,
                    invariants,
                    declared_bindings,
                    pass,
                    revision,
                    &site,
                    violations,
                );
            }
            HirStmt::Block(body) => {
                validate_hir_stmt_list(
                    body,
                    path,
                    loop_depth,
                    switch_depth,
                    invariants,
                    declared_bindings,
                    defined_labels,
                    pass,
                    revision,
                    violations,
                );
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                let site = ValidationSite::StmtPath {
                    path: path.clone(),
                    kind: "Switch",
                };
                validate_hir_expr(
                    expr,
                    invariants,
                    declared_bindings,
                    pass,
                    revision,
                    &site,
                    violations,
                );
                let mut seen_case_values = BTreeSet::new();
                for (case_idx, case) in cases.iter().enumerate() {
                    if invariants.contains(IrInvariant::StructuredControlExits) {
                        if case.values.is_empty() {
                            violations.push(StructuralViolation::new(
                                IrStage::Hir,
                                pass,
                                revision,
                                IrInvariant::StructuredControlExits,
                                site.clone(),
                                format!("switch case #{case_idx} has no discriminator values"),
                                &format!("Switch case[{case_idx}].values = []"),
                            ));
                        }
                        for &val in &case.values {
                            if !seen_case_values.insert(val) {
                                violations.push(StructuralViolation::new(
                                    IrStage::Hir,
                                    pass,
                                    revision,
                                    IrInvariant::StructuredControlExits,
                                    site.clone(),
                                    format!("duplicate switch case value {val}"),
                                    &format!("Switch duplicate case {val}"),
                                ));
                            }
                        }
                    }
                    validate_hir_stmt_list(
                        &case.body,
                        path,
                        loop_depth,
                        switch_depth + 1,
                        invariants,
                        declared_bindings,
                        defined_labels,
                        pass,
                        revision,
                        violations,
                    );
                }
                validate_hir_stmt_list(
                    default,
                    path,
                    loop_depth,
                    switch_depth + 1,
                    invariants,
                    declared_bindings,
                    defined_labels,
                    pass,
                    revision,
                    violations,
                );
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let site = ValidationSite::StmtPath {
                    path: path.clone(),
                    kind: "If",
                };
                validate_hir_expr(
                    cond,
                    invariants,
                    declared_bindings,
                    pass,
                    revision,
                    &site,
                    violations,
                );
                validate_hir_stmt_list(
                    then_body,
                    path,
                    loop_depth,
                    switch_depth,
                    invariants,
                    declared_bindings,
                    defined_labels,
                    pass,
                    revision,
                    violations,
                );
                validate_hir_stmt_list(
                    else_body,
                    path,
                    loop_depth,
                    switch_depth,
                    invariants,
                    declared_bindings,
                    defined_labels,
                    pass,
                    revision,
                    violations,
                );
            }
            HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
                let site = ValidationSite::StmtPath {
                    path: path.clone(),
                    kind: "Loop",
                };
                validate_hir_expr(
                    cond,
                    invariants,
                    declared_bindings,
                    pass,
                    revision,
                    &site,
                    violations,
                );
                validate_hir_stmt_list(
                    body,
                    path,
                    loop_depth + 1,
                    switch_depth,
                    invariants,
                    declared_bindings,
                    defined_labels,
                    pass,
                    revision,
                    violations,
                );
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                let site = ValidationSite::StmtPath {
                    path: path.clone(),
                    kind: "For",
                };
                if let Some(init) = init {
                    validate_hir_stmt_list(
                        std::slice::from_ref(init.as_ref()),
                        path,
                        loop_depth,
                        switch_depth,
                        invariants,
                        declared_bindings,
                        defined_labels,
                        pass,
                        revision,
                        violations,
                    );
                }
                if let Some(cond) = cond {
                    validate_hir_expr(
                        cond,
                        invariants,
                        declared_bindings,
                        pass,
                        revision,
                        &site,
                        violations,
                    );
                }
                if let Some(update) = update {
                    validate_hir_stmt_list(
                        std::slice::from_ref(update.as_ref()),
                        path,
                        loop_depth,
                        switch_depth,
                        invariants,
                        declared_bindings,
                        defined_labels,
                        pass,
                        revision,
                        violations,
                    );
                }
                validate_hir_stmt_list(
                    body,
                    path,
                    loop_depth + 1,
                    switch_depth,
                    invariants,
                    declared_bindings,
                    defined_labels,
                    pass,
                    revision,
                    violations,
                );
            }
            HirStmt::Label(_) => {}
            HirStmt::Goto(target) => {
                if invariants.contains(IrInvariant::LabelGotoResolution) {
                    if target.is_empty() {
                        violations.push(StructuralViolation::new(
                            IrStage::Hir,
                            pass,
                            revision,
                            IrInvariant::LabelGotoResolution,
                            ValidationSite::StmtPath {
                                path: path.clone(),
                                kind: "Goto",
                            },
                            "goto has empty target label",
                            "Goto(\"\")",
                        ));
                    } else if target == SWITCH_FALLTHROUGH_SENTINEL {
                        if switch_depth == 0 {
                            violations.push(StructuralViolation::new(
                                IrStage::Hir,
                                pass,
                                revision,
                                IrInvariant::LabelGotoResolution,
                                ValidationSite::Label {
                                    name: target.clone(),
                                },
                                "switch fallthrough sentinel outside switch",
                                "Goto(__fallthrough) at switch_depth=0",
                            ));
                        }
                    } else if !defined_labels.contains(target.as_str()) {
                        violations.push(StructuralViolation::new(
                            IrStage::Hir,
                            pass,
                            revision,
                            IrInvariant::LabelGotoResolution,
                            ValidationSite::Label {
                                name: target.clone(),
                            },
                            format!("goto references undefined label '{target}'"),
                            &format!("Goto({target})"),
                        ));
                    }
                }
            }
            HirStmt::Return(expr) => {
                if let Some(expr) = expr {
                    let site = ValidationSite::StmtPath {
                        path: path.clone(),
                        kind: "Return",
                    };
                    validate_hir_expr(
                        expr,
                        invariants,
                        declared_bindings,
                        pass,
                        revision,
                        &site,
                        violations,
                    );
                }
            }
            HirStmt::Break => {
                if invariants.contains(IrInvariant::StructuredControlExits)
                    && loop_depth == 0
                    && switch_depth == 0
                {
                    violations.push(StructuralViolation::new(
                        IrStage::Hir,
                        pass,
                        revision,
                        IrInvariant::StructuredControlExits,
                        ValidationSite::StmtPath {
                            path: path.clone(),
                            kind: "Break",
                        },
                        "`break` outside loop or switch",
                        "Break at loop_depth=0, switch_depth=0",
                    ));
                }
            }
            HirStmt::Continue => {
                if invariants.contains(IrInvariant::StructuredControlExits) && loop_depth == 0 {
                    violations.push(StructuralViolation::new(
                        IrStage::Hir,
                        pass,
                        revision,
                        IrInvariant::StructuredControlExits,
                        ValidationSite::StmtPath {
                            path: path.clone(),
                            kind: "Continue",
                        },
                        "`continue` outside loop",
                        &format!("Continue at loop_depth=0, switch_depth={switch_depth}"),
                    ));
                }
            }
        }
        path.pop();
    }
}

fn validate_hir_lvalue(
    lhs: &HirLValue,
    invariants: IrInvariantSet,
    declared_bindings: Option<&BTreeSet<&str>>,
    pass: &str,
    revision: u64,
    site: &ValidationSite,
    violations: &mut Vec<StructuralViolation>,
) {
    match lhs {
        HirLValue::Var(name) => {
            if invariants.contains(IrInvariant::BindingScope) && name.is_empty() {
                violations.push(StructuralViolation::new(
                    IrStage::Hir,
                    pass,
                    revision,
                    IrInvariant::BindingScope,
                    site.clone(),
                    "lvalue variable has empty name",
                    "HirLValue::Var(\"\")",
                ));
            }
        }
        HirLValue::Deref { ptr, ty } => {
            validate_hir_expr(
                ptr,
                invariants,
                declared_bindings,
                pass,
                revision,
                site,
                violations,
            );
            if invariants.contains(IrInvariant::EffectRepresentation) {
                validate_nir_type(ty, IrStage::Hir, pass, revision, site, violations);
            }
        }
        HirLValue::Index {
            base,
            index,
            elem_ty,
        } => {
            validate_hir_expr(
                base,
                invariants,
                declared_bindings,
                pass,
                revision,
                site,
                violations,
            );
            validate_hir_expr(
                index,
                invariants,
                declared_bindings,
                pass,
                revision,
                site,
                violations,
            );
            if invariants.contains(IrInvariant::EffectRepresentation) {
                validate_nir_type(elem_ty, IrStage::Hir, pass, revision, site, violations);
            }
        }
        HirLValue::FieldAccess {
            base,
            field_name,
            ty,
            ..
        } => {
            validate_hir_expr(
                base,
                invariants,
                declared_bindings,
                pass,
                revision,
                site,
                violations,
            );
            if invariants.contains(IrInvariant::EffectRepresentation) {
                if field_name.is_empty() {
                    violations.push(StructuralViolation::new(
                        IrStage::Hir,
                        pass,
                        revision,
                        IrInvariant::EffectRepresentation,
                        site.clone(),
                        "field lvalue has empty field_name",
                        "HirLValue::FieldAccess { field_name: \"\" }",
                    ));
                }
                validate_nir_type(ty, IrStage::Hir, pass, revision, site, violations);
            }
        }
    }
}

fn validate_hir_expr(
    expr: &HirExpr,
    invariants: IrInvariantSet,
    declared_bindings: Option<&BTreeSet<&str>>,
    pass: &str,
    revision: u64,
    site: &ValidationSite,
    violations: &mut Vec<StructuralViolation>,
) {
    match expr {
        HirExpr::Var(name) => {
            if invariants.contains(IrInvariant::BindingScope) && name.is_empty() {
                violations.push(StructuralViolation::new(
                    IrStage::Hir,
                    pass,
                    revision,
                    IrInvariant::BindingScope,
                    site.clone(),
                    "expression variable has empty name",
                    "HirExpr::Var(\"\")",
                ));
            }
        }
        HirExpr::AddressOfGlobal(name) => {
            if invariants.contains(IrInvariant::BindingScope) && name.is_empty() {
                violations.push(StructuralViolation::new(
                    IrStage::Hir,
                    pass,
                    revision,
                    IrInvariant::BindingScope,
                    site.clone(),
                    "address-of-global has empty name",
                    "HirExpr::AddressOfGlobal(\"\")",
                ));
            }
        }
        HirExpr::AddressOfLocal(name) => {
            if invariants.contains(IrInvariant::BindingScope) {
                if name.is_empty() {
                    violations.push(StructuralViolation::new(
                        IrStage::Hir,
                        pass,
                        revision,
                        IrInvariant::BindingScope,
                        site.clone(),
                        "address-of-local has empty name",
                        "HirExpr::AddressOfLocal(\"\")",
                    ));
                } else if let Some(declared) = declared_bindings
                    && !declared.contains(name.as_str())
                {
                    violations.push(StructuralViolation::new(
                        IrStage::Hir,
                        pass,
                        revision,
                        IrInvariant::BindingScope,
                        ValidationSite::Binding { name: name.clone() },
                        format!("address-of-local references undeclared binding '{name}'"),
                        &format!("HirExpr::AddressOfLocal({name})"),
                    ));
                }
            }
        }
        HirExpr::Const(_, ty) => {
            if invariants.contains(IrInvariant::EffectRepresentation) {
                validate_nir_type(ty, IrStage::Hir, pass, revision, site, violations);
            }
        }
        HirExpr::Cast { ty, expr } => {
            if invariants.contains(IrInvariant::EffectRepresentation) {
                validate_nir_type(ty, IrStage::Hir, pass, revision, site, violations);
            }
            validate_hir_expr(
                expr,
                invariants,
                declared_bindings,
                pass,
                revision,
                site,
                violations,
            );
        }
        HirExpr::Unary { expr, ty, .. } => {
            if invariants.contains(IrInvariant::EffectRepresentation) {
                validate_nir_type(ty, IrStage::Hir, pass, revision, site, violations);
            }
            validate_hir_expr(
                expr,
                invariants,
                declared_bindings,
                pass,
                revision,
                site,
                violations,
            );
        }
        HirExpr::Binary { lhs, rhs, ty, .. } => {
            if invariants.contains(IrInvariant::EffectRepresentation) {
                validate_nir_type(ty, IrStage::Hir, pass, revision, site, violations);
            }
            validate_hir_expr(
                lhs,
                invariants,
                declared_bindings,
                pass,
                revision,
                site,
                violations,
            );
            validate_hir_expr(
                rhs,
                invariants,
                declared_bindings,
                pass,
                revision,
                site,
                violations,
            );
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ty,
        } => {
            if invariants.contains(IrInvariant::EffectRepresentation) {
                validate_nir_type(ty, IrStage::Hir, pass, revision, site, violations);
            }
            validate_hir_expr(
                cond,
                invariants,
                declared_bindings,
                pass,
                revision,
                site,
                violations,
            );
            validate_hir_expr(
                then_expr,
                invariants,
                declared_bindings,
                pass,
                revision,
                site,
                violations,
            );
            validate_hir_expr(
                else_expr,
                invariants,
                declared_bindings,
                pass,
                revision,
                site,
                violations,
            );
        }
        HirExpr::Call {
            target, args, ty, ..
        } => {
            if invariants.contains(IrInvariant::EffectRepresentation) {
                if target.is_empty() {
                    violations.push(StructuralViolation::new(
                        IrStage::Hir,
                        pass,
                        revision,
                        IrInvariant::EffectRepresentation,
                        site.clone(),
                        "call expression has empty target",
                        "HirExpr::Call { target: \"\" }",
                    ));
                }
                validate_nir_type(ty, IrStage::Hir, pass, revision, site, violations);
            }
            for arg in args {
                validate_hir_expr(
                    arg,
                    invariants,
                    declared_bindings,
                    pass,
                    revision,
                    site,
                    violations,
                );
            }
        }
        HirExpr::Load { ptr, ty } => {
            if invariants.contains(IrInvariant::EffectRepresentation) {
                validate_nir_type(ty, IrStage::Hir, pass, revision, site, violations);
            }
            validate_hir_expr(
                ptr,
                invariants,
                declared_bindings,
                pass,
                revision,
                site,
                violations,
            );
        }
        HirExpr::PtrOffset { base, .. } => {
            validate_hir_expr(
                base,
                invariants,
                declared_bindings,
                pass,
                revision,
                site,
                violations,
            );
        }
        HirExpr::Index {
            base,
            index,
            elem_ty,
        } => {
            if invariants.contains(IrInvariant::EffectRepresentation) {
                validate_nir_type(elem_ty, IrStage::Hir, pass, revision, site, violations);
            }
            validate_hir_expr(
                base,
                invariants,
                declared_bindings,
                pass,
                revision,
                site,
                violations,
            );
            validate_hir_expr(
                index,
                invariants,
                declared_bindings,
                pass,
                revision,
                site,
                violations,
            );
        }
        HirExpr::FieldAccess {
            base,
            field_name,
            ty,
            ..
        } => {
            if invariants.contains(IrInvariant::EffectRepresentation) {
                if field_name.is_empty() {
                    violations.push(StructuralViolation::new(
                        IrStage::Hir,
                        pass,
                        revision,
                        IrInvariant::EffectRepresentation,
                        site.clone(),
                        "field access has empty field_name",
                        "HirExpr::FieldAccess { field_name: \"\" }",
                    ));
                }
                validate_nir_type(ty, IrStage::Hir, pass, revision, site, violations);
            }
            validate_hir_expr(
                base,
                invariants,
                declared_bindings,
                pass,
                revision,
                site,
                violations,
            );
        }
        HirExpr::AggregateCopy { src, size } => {
            if invariants.contains(IrInvariant::EffectRepresentation) && *size == 0 {
                violations.push(StructuralViolation::new(
                    IrStage::Hir,
                    pass,
                    revision,
                    IrInvariant::EffectRepresentation,
                    site.clone(),
                    "aggregate copy has zero byte size",
                    "HirExpr::AggregateCopy { size: 0 }",
                ));
            }
            validate_hir_expr(
                src,
                invariants,
                declared_bindings,
                pass,
                revision,
                site,
                violations,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        CallEdgeKind, CallEffectSummary, CallSummary, CallTargetProvenance, CallTargetRef,
        HirSwitchCase, NirBinding, PrototypeSummary, SummarySoundness,
    };

    fn i32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: true,
        }
    }

    fn base_hir_func(body: Vec<HirStmt>) -> HirFunction {
        HirFunction {
            name: "f".to_string(),
            params: vec![NirBinding {
                name: "p0".to_string(),
                ty: i32_ty(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            locals: vec![NirBinding {
                name: "loc0".to_string(),
                ty: i32_ty(),
                surface_type_name: None,
                origin: None,
                initializer: None,
            }],
            return_type: i32_ty(),
            body,
            ..Default::default()
        }
    }

    #[test]
    fn valid_nested_loops_switch_and_fallthrough_pass_hir_validation() {
        let func = base_hir_func(vec![
            HirStmt::While {
                cond: HirExpr::Var("p0".to_string()),
                body: vec![
                    HirStmt::Switch {
                        expr: HirExpr::AddressOfLocal("loc0".to_string()),
                        cases: vec![
                            HirSwitchCase {
                                values: vec![1],
                                body: vec![HirStmt::Goto(SWITCH_FALLTHROUGH_SENTINEL.to_string())],
                            },
                            HirSwitchCase {
                                values: vec![2],
                                body: vec![HirStmt::Continue],
                            },
                        ],
                        default: vec![HirStmt::Break],
                    },
                    HirStmt::Continue,
                ],
            },
            HirStmt::Goto("exit".to_string()),
            HirStmt::Label("exit".to_string()),
            HirStmt::Return(Some(HirExpr::Binary {
                op: super::super::HirBinaryOp::Add,
                lhs: Box::new(HirExpr::Var("p0".to_string())),
                rhs: Box::new(HirExpr::Const(1, i32_ty())),
                ty: i32_ty(),
            })),
        ]);

        let res = validate_hir_function(&func, IrInvariantSet::HIR_STAGE, "test_valid", 1);
        assert!(res.is_ok(), "unexpected error: {res:?}");
    }

    #[test]
    fn rejects_binding_scope_mutants_with_exact_site_and_non_proof_flag() {
        let mut dup_local = base_hir_func(vec![]);
        dup_local.locals.push(dup_local.locals[0].clone());
        let err = validate_hir_function(&dup_local, IrInvariantSet::HIR_STAGE, "pass_dup", 3)
            .unwrap_err();
        assert!(!err.proves_semantic_equivalence());
        assert_eq!(err.pass, "pass_dup");
        assert_eq!(err.revision, 3);
        assert_eq!(err.violations[0].invariant, IrInvariant::BindingScope);
        assert_eq!(
            err.violations[0].site,
            ValidationSite::Binding {
                name: "loc0".to_string()
            }
        );

        let undeclared_addr = base_hir_func(vec![HirStmt::Return(Some(HirExpr::AddressOfLocal(
            "ghost".to_string(),
        )))]);
        let err =
            validate_hir_function(&undeclared_addr, IrInvariantSet::HIR_STAGE, "pass_addr", 4)
                .unwrap_err();
        assert_eq!(err.violations[0].invariant, IrInvariant::BindingScope);
        assert_eq!(
            err.violations[0].site,
            ValidationSite::Binding {
                name: "ghost".to_string()
            }
        );
    }

    #[test]
    fn rejects_label_goto_and_fallthrough_mutants() {
        let missing_goto = base_hir_func(vec![HirStmt::Goto("missing".to_string())]);
        let err = validate_hir_function(&missing_goto, IrInvariantSet::HIR_STAGE, "goto_pass", 1)
            .unwrap_err();
        assert_eq!(
            err.violations[0].invariant,
            IrInvariant::LabelGotoResolution
        );

        let dup_label = base_hir_func(vec![
            HirStmt::Label("L1".to_string()),
            HirStmt::Label("L1".to_string()),
        ]);
        let err = validate_hir_function(&dup_label, IrInvariantSet::HIR_STAGE, "label_pass", 1)
            .unwrap_err();
        assert_eq!(
            err.violations[0].invariant,
            IrInvariant::LabelGotoResolution
        );

        let stray_fallthrough =
            base_hir_func(vec![HirStmt::Goto(SWITCH_FALLTHROUGH_SENTINEL.to_string())]);
        let err =
            validate_hir_function(&stray_fallthrough, IrInvariantSet::HIR_STAGE, "ft_pass", 1)
                .unwrap_err();
        assert_eq!(
            err.violations[0].invariant,
            IrInvariant::LabelGotoResolution
        );
    }

    #[test]
    fn rejects_stray_break_continue_and_duplicate_switch_cases() {
        let stray_break = base_hir_func(vec![HirStmt::Break]);
        let err =
            validate_hir_function(&stray_break, IrInvariantSet::HIR_STAGE, "brk", 1).unwrap_err();
        assert_eq!(
            err.violations[0].invariant,
            IrInvariant::StructuredControlExits
        );

        let switch_continue = base_hir_func(vec![HirStmt::Switch {
            expr: HirExpr::Var("p0".to_string()),
            cases: vec![HirSwitchCase {
                values: vec![1, 1],
                body: vec![HirStmt::Continue],
            }],
            default: vec![],
        }]);
        let err = validate_hir_function(&switch_continue, IrInvariantSet::HIR_STAGE, "sw", 2)
            .unwrap_err();
        assert!(
            err.violations
                .iter()
                .any(|v| v.invariant == IrInvariant::StructuredControlExits
                    && v.message.contains("duplicate switch case value"))
        );
        assert!(
            err.violations
                .iter()
                .any(|v| v.invariant == IrInvariant::StructuredControlExits
                    && v.message.contains("`continue` outside loop"))
        );
    }

    #[test]
    fn rejects_effect_representation_mutants_and_bounds_repro_summary() {
        let mut bad_effect = base_hir_func(vec![HirStmt::Expr(HirExpr::AggregateCopy {
            src: Box::new(HirExpr::Const(
                0,
                NirType::Int {
                    bits: 0,
                    signed: false,
                },
            )),
            size: 0,
        })]);
        bad_effect.callee_summaries = std::sync::Arc::new(IndexMap::from([(
            "callee".to_string(),
            CallSummary {
                target: CallTargetRef {
                    address: Some(0x1000),
                    symbol: "callee".to_string(),
                    provenance: CallTargetProvenance::Direct,
                    edge_kind: CallEdgeKind::Direct,
                    confidence: 100,
                },
                prototype: PrototypeSummary {
                    min_arity: 4,
                    max_arity: 2,
                    locked_exact_arity: Some(5),
                    variadic_fixed_arity: None,
                    returns_void: false,
                    return_lattice: NirType::Float { bits: 24 },
                    param_lattices: vec![],
                    param_surface_type_names: vec![],
                    param_pointer_contracts: vec![],
                    soundness: SummarySoundness::Pessimistic,
                },
                effect_summary: CallEffectSummary {
                    reads_memory: Some(true),
                    writes_memory: Some(false),
                    escapes_args: Some(false),
                    regions: vec![],
                    wrapper_class: super::super::WrapperClass::None,
                    wrapper_of: None,
                    confidence: 50,
                },
            },
        )]));

        let err =
            validate_hir_function(&bad_effect, IrInvariantSet::HIR_STAGE, "eff", 5).unwrap_err();
        assert!(err.violations.len() >= 4);
        assert!(
            err.violations
                .iter()
                .all(|v| v.invariant == IrInvariant::EffectRepresentation)
        );

        let long_repro = "é".repeat(400);
        let bounded = bound_repro_summary(&long_repro);
        assert!(bounded.len() <= MAX_REPRO_SUMMARY_BYTES);
        assert!(bounded.ends_with("..."));
    }

    #[test]
    fn cfg_edge_validator_catches_asymmetry_and_oob() {
        let violations = validate_cfg_edges(
            2,
            &[vec![1, 5], vec![]],
            &[vec![], vec![]],
            IrStage::CfgStructuring,
            "cfg_test",
            7,
        );
        assert_eq!(violations.len(), 2);
        assert!(
            violations
                .iter()
                .all(|v| v.invariant == IrInvariant::CfgEdgeSymmetry)
        );
    }
}
