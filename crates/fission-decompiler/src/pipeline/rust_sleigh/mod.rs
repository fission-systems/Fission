//! Rust-Sleigh decode + NIR rendering pipeline (`decompile_with_rust_sleigh`).

mod arm_thumb_heuristic;
pub mod bounds;
mod config;
mod decode;
mod evidence;

pub use config::RustSleighDecompileConfig;
pub use evidence::RustSleighPipelineEvidence;

use crate::{NirBuildStats, NirHintStats};
use fission_static::analysis::decomp::facts::FactStore;

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
    /// The `FactStore` this decompile ended up with, if the NIR render path
    /// ran (`None` on pcode-dump/architecture-unsupported fallbacks, which
    /// never build a `DecompContext`). Includes anything newly discovered
    /// during this call (`record_inferred_type`/`record_discovered_hints`,
    /// see `context::DecompContext`) on top of whatever `FactStore` was
    /// passed in -- callers that want that to inform *later* decompiles
    /// (e.g. a server session reusing facts across separate requests) need
    /// to persist this themselves; it isn't merged back automatically.
    pub learned_facts: Option<FactStore>,
}

mod pipeline;
mod probe;
mod render_finish;

pub use pipeline::{decompile_with_rust_sleigh, decompile_with_rust_sleigh_with_facts};
pub(crate) use render_finish::apply_spec_overrides;
pub use render_finish::{
    select_nir_output_from_prebuilt_pcode, select_nir_output_from_prebuilt_pcode_with_facts,
};

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
        None,
    )
    .map(|(p, _, _)| p)
    .map_err(|f| f.message)
}
