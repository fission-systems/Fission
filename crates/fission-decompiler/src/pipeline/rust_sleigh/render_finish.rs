use crate::engine::NirEngineMode;
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

/// Attempt to resolve the `.cspec` and `.pspec` specs for `binary` and inject
/// Ghidra-style overrides into `options`.
///
/// Uses the Ghidra-style `.ldefs` index to resolve the exact `.cspec` and `.pspec`
/// filenames for the binary's `(language_id, compiler_spec_id)` pair — covering all
/// architectures in `utils/sleigh-specs/languages/`.
///
/// ## `.cspec` populates:
/// - `cspec_param_offsets` — integer parameter register REGISTER-space offsets
/// - `cspec_stack_arg_base` — stack argument base offset
/// - `sla_register_map` — inverted SLA `(offset, size)` → name for all registers
///
/// ## `.pspec` populates:
/// - `pspec_programcounter` — authoritative PC register name (`RIP`, `pc`, …)
/// - `pspec_tracked_context` — register constants (e.g. `DF=0` on x86-64)
/// - `pspec_hidden_registers` — internal SLEIGH context variables to suppress
///
/// Best-effort: if any spec is unavailable, options are returned unchanged.
pub(crate) fn apply_spec_overrides(binary: &LoadedBinary, options: &mut NirRenderOptions) {
    let Some(load_spec) = binary.load_spec() else {
        return;
    };

    let language_id = load_spec.pair.language_id.as_str();
    let compiler_spec_id = load_spec.pair.compiler_spec_id.as_str();

    // Step 1: register name → (offset, size) table used for cspec name resolution.
    //
    // Sourced from the checked-in `.slaspec` register model under
    // `utils/sleigh-specs/languages`. We deliberately do NOT read Ghidra's packaged `.sla`
    // (`vendor/`): `vendor/` is a reference-only corpus and must not be a production
    // dependency. `crate::seed_nir_render_options` via `seed_nir_render_options` seeds `sla_register_map`
    // from the same model via `ensure_sla_register_map()`; we keep that map when present.
    let Some(model) = fission_pcode::midend::cspec::register_model_for_language(language_id) else {
        return;
    };
    let reg_map = model.to_sla_register_map();
    if options.sla_register_map.is_none() {
        options.sla_register_map = Some(model.to_offset_map());
    }

    let languages_root = fission_sleigh::compiler::sleigh_languages_root();

    // Step 3: Ghidra-style exact .cspec lookup via .ldefs index.
    if let Some(resolved) = fission_pcode::midend::cspec::loader::load_cspec_for_pair(
        &languages_root,
        language_id,
        compiler_spec_id,
        &reg_map,
    ) && let Some(proto) = resolved.default_proto.as_ref()
    {
        fission_pcode::midend::cspec::apply::apply_resolved_proto_to_options(options, proto);
    }

    // Step 4: Ghidra-style .pspec lookup via the same .ldefs index.
    //
    // `.pspec` is language-level (not compiler-variant-level), but the `.ldefs` index
    // stores `processorspec="..."` per language, so any compiler_spec_id that maps to
    // the same language_id yields the same pspec.  We use the first hit.
    if let Some(pspec) = fission_pcode::midend::cspec::pspec::load_pspec_for_pair(
        &languages_root,
        language_id,
        compiler_spec_id,
    ) {
        if let Some(pc) = pspec.programcounter {
            options.pspec_programcounter = Some(pc);
        }
        if !pspec.tracked_set.is_empty() {
            options.pspec_tracked_context = pspec.tracked_set;
        }
        if !pspec.hidden_registers.is_empty() {
            options.pspec_hidden_registers = pspec.hidden_registers;
        }
    }
}

pub(crate) fn finish_rust_sleigh_render(
    binary: &LoadedBinary,
    fact_store: &FactStore,
    entry_address: u64,
    name: &str,
    config: &RustSleighDecompileConfig,
    pcode: &PcodeFunction,
    userops: std::collections::HashMap<u32, String>,
    evidence: &mut RustSleighPipelineEvidence,
) -> Result<RustSleighDecompileResult, String> {
    let mut options = crate::seed_nir_render_options(binary);
    options.pe_x64_only = config.pe_x64_only;
    options.conservative_irreducible_fallback = config.conservative_irreducible_fallback;
    options.userops = userops;
    apply_spec_overrides(binary, &mut options);

    let selection = select_nir_output_from_prebuilt_pcode_with_facts(
        pcode,
        binary,
        fact_store,
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
        let layered = fission_pcode::take_last_layered_pseudocode();
        return Ok(RustSleighDecompileResult {
            code,
            code_nir: layered.as_ref().map(|l| l.nir.clone()),
            code_hir: layered.as_ref().map(|l| l.hir.clone()),
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
            code_nir: None,
            code_hir: None,
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
    select_nir_output_from_prebuilt_pcode_with_facts(
        pcode,
        binary,
        &fact_store,
        address,
        name,
        mode,
        timeout_ms,
        options,
    )
}

pub fn select_nir_output_from_prebuilt_pcode_with_facts(
    pcode: &PcodeFunction,
    binary: &LoadedBinary,
    fact_store: &FactStore,
    address: u64,
    name: &str,
    mode: NirEngineMode,
    timeout_ms: Option<u64>,
    options: NirRenderOptions,
) -> Result<NirSelection, String> {
    crate::routing::select_nir_output_from_pcode_with_facts(
        pcode,
        binary,
        &fact_store,
        address,
        name,
        mode,
        timeout_ms,
        options,
    )
}
