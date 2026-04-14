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
use fission_loader::loader::LoadedBinary;
use fission_pcode::{NirAdmissionFacts, NirRenderOptions, PcodeFunction, TargetProfile};
use fission_static::analysis::decomp::facts::FactStore;
use std::time::Instant;

/// Admission-only heuristic for preview/NIR auto mode.
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
        NirEngineMode::Nir => {
            let pcode_start = Instant::now();
            if diag {
                eprintln!("[NIR-DIAG] get_pcode start: fn=0x{address:x} mode=nir");
            }
            let pcode_json = source.get_pcode_json(address).map_err(|e| e.to_string())?;
            if diag {
                eprintln!(
                    "[NIR-DIAG] get_pcode done: fn=0x{address:x} mode=nir elapsed_ms={:.1}",
                    pcode_start.elapsed().as_secs_f64() * 1000.0
                );
            }
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
                    if let Some(selection) = try_structuring_recovery(
                        &pcode_json,
                        binary,
                        address,
                        name,
                        timeout_ms,
                        build_nir_type_context_from_facts(binary, fact_store, address),
                        &err,
                    )? {
                        Ok(selection)
                    } else {
                        Ok(NirRoutingResolver::nir_fallback(&err))
                    }
                }
            }
        }
        NirEngineMode::Auto => {
            let pcode_start = Instant::now();
            if diag {
                eprintln!("[PREVIEW-DIAG] get_pcode start: fn=0x{address:x} mode=auto");
            }
            let pcode_json = source.get_pcode_json(address).map_err(|e| e.to_string())?;
            if diag {
                eprintln!(
                    "[PREVIEW-DIAG] get_pcode done: fn=0x{address:x} mode=auto elapsed_ms={:.1}",
                    pcode_start.elapsed().as_secs_f64() * 1000.0
                );
            }
            let type_context = build_nir_type_context_from_facts(binary, fact_store, address);
            match render_nir_from_json_with_type_context(
                &pcode_json,
                binary,
                address,
                name,
                true,
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
                    if let Some(selection) = try_structuring_recovery(
                        &pcode_json,
                        binary,
                        address,
                        name,
                        timeout_ms,
                        build_nir_type_context_from_facts(binary, fact_store, address),
                        &err,
                    )? {
                        Ok(selection)
                    } else {
                        Ok(NirRoutingResolver::nir_fallback(&err))
                    }
                }
            }
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
        NirEngineMode::Nir => {
            let type_context = build_nir_type_context_from_facts(binary, fact_store, address);
            match render_nir_from_pcode_with_type_context_and_options(
                pcode,
                binary,
                address,
                name,
                false,
                timeout_ms,
                type_context,
                options.clone(),
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
                    if let Some(selection) = try_structuring_recovery_from_pcode(
                        pcode,
                        binary,
                        address,
                        name,
                        timeout_ms,
                        build_nir_type_context_from_facts(binary, fact_store, address),
                        options,
                        &err,
                    )? {
                        Ok(selection)
                    } else {
                        Ok(NirRoutingResolver::nir_fallback(&err))
                    }
                }
            }
        }
        NirEngineMode::Auto => {
            let type_context = build_nir_type_context_from_facts(binary, fact_store, address);
            match render_nir_from_pcode_with_type_context_and_options(
                pcode,
                binary,
                address,
                name,
                true,
                timeout_ms,
                type_context,
                options.clone(),
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
                    if let Some(selection) = try_structuring_recovery_from_pcode(
                        pcode,
                        binary,
                        address,
                        name,
                        timeout_ms,
                        build_nir_type_context_from_facts(binary, fact_store, address),
                        options,
                        &err,
                    )? {
                        Ok(selection)
                    } else {
                        Ok(NirRoutingResolver::nir_fallback(&err))
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::auto_nir_admission_eligible;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};
    use fission_pcode::{PcodeBasicBlock, PcodeFunction};

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
