use fission_decompiler::{NirBuildStats, NirHintStats};
use fission_loader::loader::FunctionInfo;

#[derive(Clone, Copy)]
pub(crate) struct RenderConfig {
    pub benchmark: bool,
    pub ghidra_compat: bool,
    pub effective_no_warnings: bool,
    pub debug_decomp: bool,
    pub debug_decomp_bundle: bool,
    pub requested_address: Option<u64>,
    pub timeout_ms: Option<u64>,
    /// Deterministic work limits for each function's render.
    pub resource_limits: fission_decompiler::midend_core::ir::ResourceLimits,
    /// Also capture/render PreHIR alongside NIR/HIR (`decomp --prehir`).
    pub prehir: bool,
}

pub(crate) struct CliRustDecompileRecord {
    pub func: FunctionInfo,
    pub outcome: CliRustOutcome,
}

/// A batch job/infrastructure failure: the batch runner did not give the
/// function a complete render attempt (or could not deliver its result).
/// Contrast [`AbortedRenderKind::AnalysisPanicked`] and
/// [`AbortedRenderKind::AnalysisTimedOut`], where the attempt ran and the
/// analysis itself failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InfrastructureFailureKind {
    WorkerInternalError,
    WorkerUnavailable,
    NotStarted,
    Cancelled,
}

impl InfrastructureFailureKind {
    pub(crate) fn fallback_reason(self) -> &'static str {
        match self {
            Self::WorkerInternalError => "rust_sleigh:worker_internal_error",
            Self::WorkerUnavailable => "rust_sleigh:worker_unavailable",
            Self::NotStarted => "rust_sleigh:worker_not_started",
            Self::Cancelled => "rust_sleigh:worker_cancelled",
        }
    }

    pub(crate) fn job_status(self) -> &'static str {
        match self {
            Self::WorkerInternalError => "infrastructure_failed",
            Self::WorkerUnavailable => "worker_unavailable",
            Self::NotStarted => "not_started",
            Self::Cancelled => "cancelled",
        }
    }
}

/// Why a render produced no analysis output of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AbortedRenderKind {
    /// The render ran on this function and panicked: a decompiler defect on
    /// this input, not a fault of the batch runner.
    AnalysisPanicked,
    /// The render ran on this function past its wall-clock budget.
    AnalysisTimedOut,
    /// The batch runner failed; the function's analysis is not to blame.
    Infrastructure(InfrastructureFailureKind),
}

impl AbortedRenderKind {
    pub(crate) fn fallback_reason(self) -> &'static str {
        match self {
            // The value every aborted render carried before the analysis /
            // infrastructure split; `failure_domain` now tells them apart.
            Self::AnalysisPanicked | Self::AnalysisTimedOut => "rust_sleigh:worker_internal_error",
            Self::Infrastructure(kind) => kind.fallback_reason(),
        }
    }
}

pub(crate) enum CliRustOutcome {
    Success {
        /// The single output surface (HIR presentation of the structured tree).
        code: String,
        /// PreHIR text, only present when `RenderConfig::prehir` was set.
        code_prehir: Option<String>,
        fell_back: bool,
        fallback_reason: Option<String>,
        build_stats: Option<NirBuildStats>,
        hint_stats: Option<NirHintStats>,
        decomp_sec: f64,
        /// Recovered variables, so a consumer does not have to parse them
        /// back out of the printed declarations.
        variables: Vec<fission_decompiler::RecoveredVariable>,
        line_mappings: Vec<fission_decompiler::RecoveredLineMapping>,
        /// The file-scope objects the code declares.
        globals: Vec<DecompiledGlobal>,
        /// Stack slots whose offset from the canonical frame address is proven.
        frame: Vec<FrameSlot>,
        /// Callees the code calls through a declared, not recovered, prototype.
        declared_callees: Vec<String>,
    },
    AssemblyFallback {
        fallback_code: String,
        original_error: String,
        decomp_sec: f64,
    },
    HardError {
        error_text: String,
        decomp_sec: f64,
    },
    /// The render did not produce an analysis result: it panicked, timed
    /// out, or the batch runner failed around it (see `kind`).
    WorkerInternalError {
        kind: AbortedRenderKind,
        message: String,
        assembly_fallback_code: Option<String>,
    },
}

/// A global the decompiled code declares, as data.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct DecompiledGlobal {
    pub name: String,
    /// The C type of its declaration.
    #[serde(rename = "type")]
    pub type_name: String,
    /// `None` when several symbols share the name, so the text does not say
    /// which one is meant.
    pub address: Option<String>,
}

/// A stack slot and its offset from the canonical frame address (the stack
/// pointer before the call that entered the function).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub(crate) struct FrameSlot {
    pub name: String,
    pub cfa_offset: i64,
}
