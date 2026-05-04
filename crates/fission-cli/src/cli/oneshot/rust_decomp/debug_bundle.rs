use crate::cli::oneshot::debug_decomp::debug_decomp_bundle_json;
use crate::cli::oneshot::rust_decomp::record::RenderConfig;
use fission_loader::loader::{FunctionInfo, LoadedBinary};

pub(crate) fn debug_bundle_for_record(
    binary: &LoadedBinary,
    func: &FunctionInfo,
    config: RenderConfig,
    build_stats: Option<&fission_pcode::NirBuildStats>,
    hint_stats: Option<&fission_pcode::NirHintStats>,
    nir_hard_fail: bool,
    nir_fallback_without_stats: bool,
) -> Option<serde_json::Value> {
    (config.debug_decomp || config.debug_decomp_bundle).then(|| {
        debug_decomp_bundle_json(
            binary,
            config.requested_address,
            func,
            build_stats,
            hint_stats,
            None,
            nir_hard_fail,
            nir_fallback_without_stats,
        )
    })
}
