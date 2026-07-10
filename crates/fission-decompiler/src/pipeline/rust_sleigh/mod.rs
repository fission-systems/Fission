//! Rust-Sleigh decode + NIR rendering pipeline (`decompile_with_rust_sleigh`).

pub mod bounds;
mod config;
mod decode;
mod evidence;

pub use config::RustSleighDecompileConfig;
pub use evidence::RustSleighPipelineEvidence;

use crate::{NirBuildStats, NirHintStats};

#[derive(Debug, Clone)]
pub struct RustSleighDecompileResult {
    /// Primary code (NIR-faithful by default for oracle / backward compat).
    pub code: String,
    /// Dual-layer surfaces when available (same IR build).
    pub code_nir: Option<String>,
    pub code_hir: Option<String>,
    pub fell_back: bool,
    pub fallback_reason: Option<String>,
    pub build_stats: Option<NirBuildStats>,
    pub hint_stats: Option<NirHintStats>,
    pub evidence: RustSleighPipelineEvidence,
}

mod pipeline;
mod probe;
mod render_finish;

pub use pipeline::decompile_with_rust_sleigh;
pub(crate) use render_finish::apply_spec_overrides;
pub use render_finish::select_nir_output_from_prebuilt_pcode;

use fission_loader::loader::LoadedBinary;

/// Facts/preview helpers call sites expect `Result<PcodeFunction, String>` without decode telemetry.
pub(crate) fn decode_rust_sleigh_pcode(
    binary: &LoadedBinary,
    name: &str,
    entry_address: u64,
    max_bytes: usize,
    instruction_limit: usize,
    continue_past_indirect_branch: bool,
    retry_on_decode_error: bool,
) -> Result<crate::PcodeFunction, String> {
    decode::decode_rust_sleigh_pcode(
        binary,
        name,
        entry_address,
        max_bytes,
        instruction_limit,
        continue_past_indirect_branch,
        retry_on_decode_error,
    )
    .map(|(p, _, _)| p)
    .map_err(|f| f.message)
}
