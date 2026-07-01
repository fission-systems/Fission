use crate::recovery::{
    is_type_failure_for_nir_rescue, try_structuring_recovery, try_structuring_recovery_from_pcode,
};
use crate::render::{
    build_nir_type_context_from_facts, render_nir_from_json_with_type_context,
    render_nir_from_pcode_with_type_context_and_options,
};
use crate::taxonomy::classify_native_failure_kind;
use crate::types::{
    NirEngineMode, NirRoutingDecision, NirRoutingResolver, NirSelection, NirSource,
};
use crate::{NirAdmissionFacts, NirRenderOptions, NirTypeContext, PcodeFunction, TargetProfile};
use fission_loader::loader::LoadedBinary;
use fission_static::analysis::decomp::facts::FactStore;
use std::time::Instant;

/// Admission-only gate for preview/NIR auto mode.
///
/// This is intentionally a raw pcode-shape gate, not a semantic success/failure
/// classifier. It should never be used as a substitute for canonical
/// `NirBuildStats`-based ownership decisions.
pub fn auto_nir_admission_eligible(binary: &LoadedBinary, pcode: &PcodeFunction) -> bool {
    let profile = TargetProfile::from_binary(binary, true);
    profile.auto_admission_eligible(NirAdmissionFacts::from_pcode(pcode))
}

pub fn auto_nir_eligible(binary: &LoadedBinary, pcode: &PcodeFunction) -> bool {
    auto_nir_admission_eligible(binary, pcode)
}

pub fn native_failure_routing_decision(error: &str) -> NirRoutingDecision {
    let _ = classify_native_failure_kind(error);
    NirRoutingResolver::native_failure(error)
}

/// Render a NIR selection from raw pcode JSON with a pre-built type context.
///
/// Phase 1 refactor: `type_context` is now built by the caller once and passed in.
/// Previously this function called `build_nir_type_context_from_facts` internally,
/// which forced a redundant clone for the structuring-recovery fallback path.
///
/// # Clone accounting
/// - Happy path:   `type_context` is moved into `render_nir_from_json_with_type_context`.
/// - Recovery path: `type_context` is moved into `try_structuring_recovery`.
/// Since both paths are mutually exclusive, no clone is required here.
fn render_selection_from_json(
    pcode_json: &str,
    binary: &LoadedBinary,
    type_context: NirTypeContext,
    address: u64,
    name: &str,
    prefer_preview_surface: bool,
    timeout_ms: Option<u64>,
) -> Result<NirSelection, String> {
    match render_nir_from_json_with_type_context(
        pcode_json,
        binary,
        address,
        name,
        prefer_preview_surface,
        timeout_ms,
        type_context,
        false,
        false,
    ) {
        Ok(Some((code, build_stats, hint_stats))) => Ok(NirRoutingResolver::nir_success(
            code,
            build_stats,
            hint_stats,
            false,
            None,
        )),
        Ok(None) => Ok(NirRoutingResolver::nir_fallback(
            "nir skipped: function not supported by Fission NIR builder",
        )),
        Err(err) => {
            if err.contains("not a function (orphan block detected)") {
                return Ok(NirRoutingResolver::nir_skipped(&err));
            }
            // On error the caller-supplied type_context has been consumed; we cannot
            // reuse it. try_structuring_recovery rebuilds a fresh context internally.
            // (Phase 3 will eliminate this by passing &mut DecompContext through.)
            if let Some(selection) = try_structuring_recovery_with_facts_rebuild(
                pcode_json,
                binary,
                address,
                name,
                timeout_ms,
                &err,
            )? {
                Ok(selection)
            } else {
                Ok(NirRoutingResolver::nir_fallback(&err))
            }
        }
    }
}

/// Structuring recovery that rebuilds type context from facts.
///
/// Called only when the main render path fails. In Phase 3 this will accept a
/// `&mut DecompContext` instead of rebuilding from scratch.
fn try_structuring_recovery_with_facts_rebuild(
    pcode_json: &str,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    timeout_ms: Option<u64>,
    err: &str,
) -> Result<Option<NirSelection>, String> {
    // Rebuild a fresh FactStore for the recovery path.
    // This is a temporary overhead that Phase 2 will eliminate by threading
    // &mut DecompContext through the entire pipeline.
    let fact_store = FactStore::from_binary(binary);
    let type_context = build_nir_type_context_from_facts(binary, &fact_store, address);
    try_structuring_recovery(pcode_json, binary, address, name, timeout_ms, type_context, err)
}

/// Render a NIR selection from a pre-lifted pcode function with a pre-built type context.
///
/// Phase 1 refactor: `type_context` is now built by the caller once and passed in.
/// `options` is also passed by value (moved), eliminating the clone in the old
/// `DecompileRequest::resolved_render_options()` path.
fn render_selection_from_pcode(
    pcode: &PcodeFunction,
    binary: &LoadedBinary,
    type_context: NirTypeContext,
    address: u64,
    name: &str,
    prefer_preview_surface: bool,
    timeout_ms: Option<u64>,
    options: NirRenderOptions,
) -> Result<NirSelection, String> {
    match render_nir_from_pcode_with_type_context_and_options(
        pcode,
        binary,
        address,
        name,
        prefer_preview_surface,
        timeout_ms,
        type_context,
        options,
        false,
        false,
    ) {
        Ok(Some((code, build_stats, hint_stats))) => Ok(NirRoutingResolver::nir_success(
            code,
            build_stats,
            hint_stats,
            false,
            None,
        )),
        Ok(None) => Ok(NirRoutingResolver::nir_fallback(
            "nir skipped: function not supported by Fission NIR builder",
        )),
        Err(err) => {
            if err.contains("not a function (orphan block detected)") {
                return Ok(NirRoutingResolver::nir_skipped(&err));
            }
            // Recovery path: rebuild context from facts.
            // Phase 3 eliminates this rebuild via &mut DecompContext threading.
            let fact_store = FactStore::from_binary(binary);
            let recovery_type_context =
                build_nir_type_context_from_facts(binary, &fact_store, address);
            let recovery_options = crate::render::nir_options_with_recovery(binary, false, false);
            if let Some(selection) = try_structuring_recovery_from_pcode(
                pcode,
                binary,
                address,
                name,
                timeout_ms,
                recovery_type_context,
                recovery_options,
                &err,
            )? {
                Ok(selection)
            } else {
                Ok(NirRoutingResolver::nir_fallback(&err))
            }
        }
    }
}

pub fn select_nir_output<S: NirSource>(
    source: &mut S,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    mode: NirEngineMode,
    timeout_ms: Option<u64>,
) -> Result<NirSelection, String> {
    let fact_store = FactStore::from_binary(binary);
    select_nir_output_with_facts(source, binary, &fact_store, address, name, mode, timeout_ms)
}

pub fn select_nir_output_with_facts<S: NirSource>(
    source: &mut S,
    binary: &LoadedBinary,
    fact_store: &FactStore,
    address: u64,
    name: &str,
    mode: NirEngineMode,
    timeout_ms: Option<u64>,
) -> Result<NirSelection, String> {
    let diag = std::env::var_os("FISSION_PREVIEW_DIAG").is_some();
    match mode {
        NirEngineMode::Legacy => Ok(NirRoutingResolver::legacy_mode()),
        NirEngineMode::Nir | NirEngineMode::Auto => {
            let prefer_preview_surface = matches!(mode, NirEngineMode::Auto);
            let pcode_start = Instant::now();
            if diag {
                let mode_label = if prefer_preview_surface { "auto" } else { "nir" };
                eprintln!("[NIR-DIAG] get_pcode start: fn=0x{address:x} mode={mode_label}");
            }
            let pcode_json = source.get_pcode_json(address).map_err(|e| e.to_string())?;
            if diag {
                let mode_label = if prefer_preview_surface { "auto" } else { "nir" };
                eprintln!(
                    "[NIR-DIAG] get_pcode done: fn=0x{address:x} mode={mode_label} elapsed_ms={:.1}",
                    pcode_start.elapsed().as_secs_f64() * 1000.0
                );
            }
            // Phase 1: build type_context once here; pass it into the render function
            // so that `render_selection_from_json` no longer needs to build it internally.
            let type_context = build_nir_type_context_from_facts(binary, fact_store, address);
            render_selection_from_json(
                &pcode_json,
                binary,
                type_context,
                address,
                name,
                prefer_preview_surface,
                timeout_ms,
            )
        }
    }
}

pub fn select_nir_output_from_pcode(
    pcode: &PcodeFunction,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    mode: NirEngineMode,
    timeout_ms: Option<u64>,
    options: NirRenderOptions,
) -> Result<NirSelection, String> {
    let fact_store = FactStore::from_binary(binary);
    select_nir_output_from_pcode_with_facts(
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

pub fn select_nir_output_from_pcode_with_facts(
    pcode: &PcodeFunction,
    binary: &LoadedBinary,
    fact_store: &FactStore,
    address: u64,
    name: &str,
    mode: NirEngineMode,
    timeout_ms: Option<u64>,
    options: NirRenderOptions,
) -> Result<NirSelection, String> {
    match mode {
        NirEngineMode::Legacy => Ok(NirRoutingResolver::legacy_mode()),
        NirEngineMode::Nir | NirEngineMode::Auto => {
            // Phase 1: build type_context once here; pass it into render_selection_from_pcode.
            let type_context = build_nir_type_context_from_facts(binary, fact_store, address);
            render_selection_from_pcode(
                pcode,
                binary,
                type_context,
                address,
                name,
                matches!(mode, NirEngineMode::Auto),
                timeout_ms,
                options,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::auto_nir_admission_eligible;
    use crate::{PcodeBasicBlock, PcodeFunction};
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};

    fn test_pcode(block_count: usize) -> PcodeFunction {
        PcodeFunction {
            blocks: (0..block_count)
                .map(|idx| PcodeBasicBlock {
                    index: idx as u32,
                    start_address: 0x401000 + (idx as u64) * 0x10,
                    successors: Vec::new(),
                    ops: Vec::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn auto_nir_admission_uses_canonical_pe_x64_profile() {
        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
            .format("PE")
            .is_64bit(true)
            .build()
            .expect("build test binary");

        assert!(auto_nir_admission_eligible(&binary, &test_pcode(4)));
    }

    #[test]
    fn auto_nir_admission_rejects_pe_x86_even_when_shape_is_small() {
        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
            .format("PE")
            .is_64bit(false)
            .build()
            .expect("build test binary");

        assert!(!auto_nir_admission_eligible(&binary, &test_pcode(4)));
    }
}

pub fn rescue_nir_output<S: NirSource>(
    source: &mut S,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    error: &str,
    timeout_ms: Option<u64>,
) -> Result<Option<NirSelection>, String> {
    let fact_store = FactStore::from_binary(binary);
    rescue_nir_output_with_facts(
        source,
        binary,
        &fact_store,
        address,
        name,
        error,
        timeout_ms,
    )
}

pub fn rescue_nir_output_with_facts<S: NirSource>(
    source: &mut S,
    binary: &LoadedBinary,
    fact_store: &FactStore,
    address: u64,
    name: &str,
    error: &str,
    timeout_ms: Option<u64>,
) -> Result<Option<NirSelection>, String> {
    if !is_type_failure_for_nir_rescue(error) {
        return Ok(None);
    }

    let diag = std::env::var_os("FISSION_PREVIEW_DIAG").is_some();
    let pcode_start = Instant::now();
    if diag {
        eprintln!("[PREVIEW-DIAG] get_pcode start: fn=0x{address:x} mode=rescue");
    }
    let pcode_json = source.get_pcode_json(address).map_err(|e| e.to_string())?;
    if diag {
        eprintln!(
            "[PREVIEW-DIAG] get_pcode done: fn=0x{address:x} mode=rescue elapsed_ms={:.1}",
            pcode_start.elapsed().as_secs_f64() * 1000.0
        );
    }
    // Phase 1: build type_context once here; no longer cloned inside the render call.
    let type_context = build_nir_type_context_from_facts(binary, fact_store, address);
    match render_nir_from_json_with_type_context(
        &pcode_json,
        binary,
        address,
        name,
        false,
        timeout_ms,
        type_context,
        false,
        false,
    ) {
        Ok(Some((code, build_stats, hint_stats))) => Ok(Some(NirRoutingResolver::nir_success(
            code,
            build_stats,
            hint_stats,
            true,
            Some(format!(
                "legacy_fallback: legacy type failure rescued by mlil-preview: {error}"
            )),
        ))),
        Ok(None) => Ok(None),
        Err(_) => Ok(None),
    }
}
