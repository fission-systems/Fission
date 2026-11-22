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
    /// Also capture/render PreHIR alongside NIR/HIR (`decomp --prehir`).
    pub prehir: bool,
}

pub(crate) struct CliRustDecompileRecord {
    pub func: FunctionInfo,
    pub outcome: CliRustOutcome,
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
    WorkerInternalError {
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
