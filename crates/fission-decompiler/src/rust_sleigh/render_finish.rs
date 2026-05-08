use crate::engine::NirEngineMode;
use crate::request::{DecompileRequest, decompile_prebuilt_pcode};
use crate::rust_sleigh::decode::render_pcode_text;
use crate::rust_sleigh::{
    RustSleighDecompileConfig, RustSleighDecompileResult, RustSleighPipelineEvidence,
};
use crate::types::NirSelection;
use crate::{NirRenderOptions, PcodeFunction};
use fission_loader::loader::LoadedBinary;
use fission_static::analysis::decomp::facts::FactStore;

pub(crate) fn should_retry_with_strict_indirect_stop(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("unsupported pcode pattern")
        || lower.contains("nir_unsupported")
        || lower.contains("unsupported opcode")
}

pub(crate) fn finish_rust_sleigh_render(
    binary: &LoadedBinary,
    entry_address: u64,
    name: &str,
    config: &RustSleighDecompileConfig,
    pcode: &PcodeFunction,
    evidence: &mut RustSleighPipelineEvidence,
) -> Result<RustSleighDecompileResult, String> {
    let mut options = NirRenderOptions::from_loaded_binary(binary);
    options.pe_x64_only = config.pe_x64_only;
    options.conservative_irreducible_fallback = config.conservative_irreducible_fallback;

    let selection = select_nir_output_from_prebuilt_pcode(
        pcode,
        binary,
        entry_address,
        name,
        config.nir_mode,
        config.nir_timeout_ms,
        options,
    )
    .map_err(|e| format!("rust_sleigh routing failed: {e}"))?;

    evidence.nir_fallback_kind = selection.fallback_kind.map(|s| s.to_string());
    evidence.nir_fallback_kind_refined = selection.fallback_kind_refined.map(|s| s.to_string());
    evidence.nir_fallback_reason_summary = selection.fallback_reason.clone();

    if let Some(code) = selection.nir_code {
        evidence
            .pipeline_stage_status
            .insert("nir_render".into(), "success".into());
        return Ok(RustSleighDecompileResult {
            code,
            fell_back: selection.fell_back,
            fallback_reason: selection.fallback_reason,
            build_stats: selection.build_stats,
            hint_stats: selection.hint_stats,
            evidence: evidence.clone(),
        });
    }

    let fallback_reason = selection.fallback_reason.unwrap_or_else(|| {
        "nir skipped: function not supported by Fission NIR builder".to_string()
    });
    let lower = fallback_reason.to_ascii_lowercase();
    let is_unsupported_arch = lower.contains("unsupported architecture in mlil-preview")
        || matches!(
            selection.fallback_kind_refined,
            Some("preview_architecture_unsupported")
        );
    if is_unsupported_arch {
        evidence
            .pipeline_stage_status
            .insert("nir_render".into(), "pcode_dump_arch_unsupported".into());
        return Ok(RustSleighDecompileResult {
            code: render_pcode_text(name, pcode),
            fell_back: true,
            fallback_reason: Some("nir_unsupported_arch:pcode_dump".to_string()),
            build_stats: None,
            hint_stats: None,
            evidence: evidence.clone(),
        });
    }

    evidence
        .pipeline_stage_status
        .insert("nir_render".into(), "failed".into());
    Err(format!("rust_sleigh render failed: {fallback_reason}"))
}

pub fn select_nir_output_from_prebuilt_pcode(
    pcode: &PcodeFunction,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    mode: NirEngineMode,
    timeout_ms: Option<u64>,
    options: NirRenderOptions,
) -> Result<NirSelection, String> {
    let fact_store = FactStore::from_binary(binary);
    let request = DecompileRequest {
        binary,
        fact_store: Some(&fact_store),
        function_address: address,
        function_name: Some(name),
        engine_mode: mode,
        timeout_ms,
        render_options: Some(options),
    };
    decompile_prebuilt_pcode(pcode, &request).map(|result| result.selection)
}
