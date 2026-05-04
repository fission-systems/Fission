use crate::engine::NirEngineMode;

#[derive(Debug, Clone)]
pub struct RustSleighDecompileConfig {
    pub decode_max_bytes_cap: usize,
    pub default_decode_bytes: usize,
    pub instruction_budget_cap: usize,
    pub instruction_budget_default: usize,
    pub continue_past_indirect_branch: bool,
    pub retry_on_decode_error: bool,
    pub use_next_function_distance_if_unknown: bool,
    pub enable_wrapper_contraction_probe: bool,
    pub wrapper_probe_max_bytes: usize,
    pub wrapper_probe_instruction_limit: usize,
    pub nir_mode: NirEngineMode,
    pub nir_timeout_ms: Option<u64>,
    pub pe_x64_only: bool,
    pub conservative_irreducible_fallback: bool,
}

impl RustSleighDecompileConfig {
    /// Default Rust-Sleigh + NIR pipeline configuration.
    ///
    /// Used by both [`crate::rust_sleigh::decompile_with_rust_sleigh`] call sites (CLI and desktop)
    /// so lift/decode limits match for the same binary and address.
    pub fn cli_defaults() -> Self {
        Self {
            decode_max_bytes_cap: 0x10000,
            default_decode_bytes: 0x4000,
            instruction_budget_cap: 4096,
            instruction_budget_default: 512,
            continue_past_indirect_branch: true,
            retry_on_decode_error: true,
            use_next_function_distance_if_unknown: true,
            enable_wrapper_contraction_probe: true,
            wrapper_probe_max_bytes: 64,
            wrapper_probe_instruction_limit: 16,
            nir_mode: NirEngineMode::Nir,
            nir_timeout_ms: None,
            pe_x64_only: false,
            conservative_irreducible_fallback: true,
        }
    }
}

impl Default for RustSleighDecompileConfig {
    fn default() -> Self {
        Self::cli_defaults()
    }
}
