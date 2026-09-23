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
    /// Typed artifacts from the same in-process NIR render. This is the
    /// canonical source for snapshots and recovered variables; compatibility
    /// TLS accessors remain available for older callers only.
    pub render_output: Option<crate::NirDecompileOutput>,
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

/// Decode a direct callee and report whether the bounded decode covered a
/// trustworthy whole-function byte/instruction window. Callers still need to
/// validate the p-code control-flow shape before treating derived effects as
/// exact.
pub(crate) fn decode_rust_sleigh_pcode_with_completion(
    binary: &LoadedBinary,
    name: &str,
    entry_address: u64,
    max_bytes: usize,
    instruction_limit: usize,
    function_size: u64,
    continue_past_indirect_branch: bool,
    retry_on_decode_error: bool,
) -> Result<(crate::PcodeFunction, bool), String> {
    let (pcode, diag, _) = decode::decode_rust_sleigh_pcode(
        binary,
        name,
        entry_address,
        max_bytes,
        instruction_limit,
        continue_past_indirect_branch,
        retry_on_decode_error,
        None,
    )
    .map_err(|failure| failure.message)?;

    let exact_byte_window = usize::try_from(function_size)
        .ok()
        .is_some_and(|size| size > 0 && size == max_bytes && size <= instruction_limit);
    let completed_without_recovery = matches!(
        diag.stop_reason.as_str(),
        "success_first_lift"
            | "success_thumb_preferred_entry_hint"
            | "success_after_forced_low_bit_code_mode_retry"
            | "success_cached_fid_decode"
    ) || diag
        .stop_reason
        .starts_with("success_after_sibling_language_retry:");

    Ok((pcode, exact_byte_window && completed_without_recovery))
}
