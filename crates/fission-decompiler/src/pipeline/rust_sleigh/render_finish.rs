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

/// Attempt to resolve the `.cspec` prototype for `binary` and inject
/// register-based ABI overrides into `options`.
///
/// Reads the SLA register map (Ghidra `ELEM_VARNODE_SYM`) and the `.cspec` XML
/// for the binary's `(language_id, compiler_spec_id)` pair. On success, populates
/// `cspec_param_offsets` and `cspec_stack_arg_base` so that `AbiState` uses
/// dynamic cspec-based resolution instead of hardcoded ABI tables.
///
/// This is a best-effort, zero-regression helper: if the SLA or cspec is not
/// available (e.g., no Ghidra install), the options are returned unchanged.
fn apply_cspec_overrides(binary: &LoadedBinary, options: &mut NirRenderOptions) {
    let Some(load_spec) = binary.load_spec() else {
        return;
    };

    // Step 1: Get register name → (offset, size) from SLA ELEM_VARNODE_SYM.
    let Some(reg_map) = fission_sleigh::runtime::register_map_for_load_spec(load_spec) else {
        return;
    };

    // Step 2: Find the .cspec file in the utils/sleigh-specs/languages tree.
    let compiler_spec_id = load_spec.pair.compiler_spec_id.as_str();
    let language_id = load_spec.pair.language_id.as_str();

    // Derive the processor subdir from the language_id, e.g. "x86:LE:64:default" → "x86"
    let processor = language_id.split(':').next().unwrap_or("");
    if processor.is_empty() {
        return;
    }

    let languages_root = fission_sleigh::compiler::sleigh_languages_root();
    let language_dir = fission_pcode::nir::cspec::loader::find_language_dir(
        &languages_root,
        processor,
    );
    let Some(language_dir) = language_dir else {
        return;
    };

    // Build candidate cspec stems in preference order:
    //   1. "<proc>-<bits>-<compiler_spec_id>"  e.g. "x86-64-gcc", "x86-64-windows"
    //   2. "<proc>-<bits>-<alias>"             e.g. "x86-64-win" (alias for "windows")
    //   3. "<compiler_spec_id>"                 e.g. "gcc"
    //   4. "default"
    let bits = if binary.is_64bit { "64" } else { "32" };
    let proc_lower = processor.to_ascii_lowercase();
    let specific_stem = format!("{proc_lower}-{bits}-{compiler_spec_id}");
    // Build short alias: "windows" → "win", "gcc" stays "gcc", etc.
    let alias = match compiler_spec_id {
        "windows" => Some("win"),
        "macosx" => Some("osx"),
        _ => None,
    };
    let alias_stem = alias.map(|a| format!("{proc_lower}-{bits}-{a}"));
    let mut preferred_stems: Vec<&str> = vec![specific_stem.as_str(), compiler_spec_id];
    if let Some(ref a) = alias_stem {
        preferred_stems.push(a.as_str());
    }
    preferred_stems.push("default");

    let Some(resolved) = fission_pcode::nir::cspec::loader::load_default_cspec_resolved(
        &language_dir,
        &preferred_stems,
        &reg_map,
    ) else {
        return;
    };

    if let Some(proto) = &resolved.default_proto {
        if !proto.int_param_offsets.is_empty() {
            options.cspec_param_offsets = Some(proto.int_param_offsets.clone());
        }
        if let Some(base) = proto.stack_arg_base {
            options.cspec_stack_arg_base = Some(base);
        }
    }
}

pub(crate) fn finish_rust_sleigh_render(
    binary: &LoadedBinary,
    entry_address: u64,
    name: &str,
    config: &RustSleighDecompileConfig,
    pcode: &PcodeFunction,
    userops: std::collections::HashMap<u32, String>,
    evidence: &mut RustSleighPipelineEvidence,
) -> Result<RustSleighDecompileResult, String> {
    let mut options = NirRenderOptions::from_loaded_binary(binary);
    options.pe_x64_only = config.pe_x64_only;
    options.conservative_irreducible_fallback = config.conservative_irreducible_fallback;
    options.userops = userops;
    apply_cspec_overrides(binary, &mut options);

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
