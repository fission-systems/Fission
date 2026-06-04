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
/// Uses the Ghidra-style `.ldefs` index to resolve the exact `.cspec` filename for
/// the binary's `(language_id, compiler_spec_id)` pair — covering all architectures
/// in `utils/sleigh-specs/languages/` (AARCH64, ARM, MIPS, PowerPC, RISC-V, Sparc,
/// LoongArch, x86/x64 with all compiler variants, etc.).
///
/// Populates:
/// - `cspec_param_offsets` — integer parameter register REGISTER-space offsets
/// - `cspec_stack_arg_base` — stack argument base offset
/// - `sla_register_map` — inverted SLA `(offset, size)` → name for all registers
///
/// Best-effort, zero-regression: if the SLA register map or `.cspec` is unavailable
/// (e.g., no Ghidra install or no `.ldefs` match), options are returned unchanged and
/// the pipeline falls back to the hardcoded `CallingConvention` tables.
fn apply_cspec_overrides(binary: &LoadedBinary, options: &mut NirRenderOptions) {
    let Some(load_spec) = binary.load_spec() else {
        return;
    };

    // Step 1: Get register name → (offset, size) from SLA ELEM_VARNODE_SYM.
    let Some(reg_map) = fission_sleigh::runtime::register_map_for_load_spec(load_spec) else {
        return;
    };

    // Step 2: Build the inverted (offset, size) → name map for the renderer.
    //
    // The SLA `reg_map` is name → (offset, size).  We invert it so the NIR renderer
    // can look up names dynamically for any architecture, replacing the hardcoded
    // `x64_ghidra_reg_name` / `aarch64_ghidra_reg_name` / … tables.
    //
    // When multiple names map to the same (offset, size) (e.g. "RAX" and "rax"),
    // prefer lowercase names as Ghidra uses lowercase for display.
    let mut inverted: std::collections::HashMap<(u64, u32), String> =
        std::collections::HashMap::new();
    for (name, (offset, size)) in &reg_map {
        let key = (*offset, *size);
        inverted
            .entry(key)
            .and_modify(|existing| {
                // Prefer shorter names (e.g. "rax" over "RAX"), then lowercase.
                if name.len() < existing.len()
                    || (name.len() == existing.len() && name.as_str() < existing.as_str())
                {
                    *existing = name.clone();
                }
            })
            .or_insert_with(|| name.clone());
    }
    options.sla_register_map = Some(inverted);

    // Step 3: Ghidra-style exact .cspec lookup via .ldefs index.
    let language_id = load_spec.pair.language_id.as_str();
    let compiler_spec_id = load_spec.pair.compiler_spec_id.as_str();
    let languages_root = fission_sleigh::compiler::sleigh_languages_root();

    let Some(resolved) = fission_pcode::nir::cspec::loader::load_cspec_for_pair(
        &languages_root,
        language_id,
        compiler_spec_id,
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
