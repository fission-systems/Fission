use fission_decompiler::{NirBuildStats, NirHintStats, PseudocodeLayer};
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
    /// Pseudocode surface selection (`nir` / `hir` / `both`).
    pub layer: PseudocodeLayer,
    /// Also capture/render PreHIR alongside NIR/HIR (`decomp --prehir`).
    pub prehir: bool,
}

pub(crate) struct CliRustDecompileRecord {
    pub func: FunctionInfo,
    pub outcome: CliRustOutcome,
    /// Layer selected for this render (drives plain text + JSON primary code).
    pub layer: PseudocodeLayer,
}

pub(crate) enum CliRustOutcome {
    Success {
        /// NIR-faithful mechanical C (oracle / default `code` for non-HIR layer).
        code: String,
        /// Dual NIR surface when dual render is available.
        code_nir: Option<String>,
        /// Dual HIR surface when dual render is available.
        code_hir: Option<String>,
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
