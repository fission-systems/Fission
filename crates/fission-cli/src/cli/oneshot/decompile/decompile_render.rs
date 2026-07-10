use super::*;

pub(super) fn strip_warnings(code: &str) -> String {
    code.lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with("WARNING:")
                && !trimmed.starts_with("NOTICE:")
                && !trimmed.starts_with("/* WARNING")
                && !trimmed.starts_with("// WARNING")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn strip_inferred_structs(code: &str) -> String {
    let mut result = String::new();
    let mut in_struct_block = false;
    for line in code.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("typedef struct") || trimmed.starts_with("// Inferred Structure") {
            if trimmed.ends_with(';') {
                continue;
            }
            in_struct_block = true;
            continue;
        }
        if in_struct_block {
            if trimmed.starts_with('}') && trimmed.ends_with(';') {
                in_struct_block = false;
                continue;
            }
            continue;
        }
        result.push_str(line);
        result.push('\n');
    }
    result
}

fn should_use_assembly_fallback(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("duplicate variablepiece")
        || lower.contains("control flow analysis error")
        || lower.contains("followflow")
        || lower.contains("preview_timeout")
        || lower.contains("could not find op at target address")
        || lower.contains("ghidra lowlevelerror")
        || lower.contains("unsupported architecture in mlil-preview")
        || lower.contains("decoded") && lower.contains("zero semantic ops")
}

pub(super) fn make_assembly_fallback(
    binary: &LoadedBinary,
    binary_data: &[u8],
    func: &FunctionInfo,
    error: &str,
) -> Option<String> {
    if !should_use_assembly_fallback(error) {
        return None;
    }
    let error_class = classify_native_failure_kind(error);
    let asm = render_function_disassembly_text(binary, binary_data, func.address).ok()?;
    Some(format!(
        "// Assembly fallback: {}\n// Function: {} @ 0x{:x}\n// Error class: {}\n\n{}",
        error, func.name, func.address, error_class, asm
    ))
}

pub(super) fn attach_native_timing(entry: &mut serde_json::Value, decomp: &DecompilerNative) {
    let Ok(raw_timing) = decomp.get_last_timing_json() else {
        return;
    };
    if raw_timing.trim().is_empty() || raw_timing.trim() == "{}" {
        return;
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw_timing) {
        entry["native_timing"] = value;
    }
}

pub(super) struct DecompEntry {
    pub(super) address: u64,
    pub(super) name: String,
    pub(super) size: u64,
    pub(super) code: Result<RenderedCode, fission_core::FissionError>,
    pub(super) decomp_sec: f64,
    pub(super) postprocess_sec: f64,
    pub(super) last_timing_json: Option<String>,
}

pub(super) struct RenderedCode {
    pub(super) code: String,
    /// NIR-faithful surface (when dual render is available).
    pub(super) code_nir: Option<String>,
    /// HIR-readable surface (when dual render is available).
    pub(super) code_hir: Option<String>,
    pub(super) postprocess_sec: f64,
    pub(super) engine_used: &'static str,
    pub(super) fell_back: bool,
    pub(super) fallback_reason: Option<String>,
    pub(super) preview_build_stats: Option<NirBuildStats>,
    pub(super) preview_hint_stats: Option<NirHintStats>,
    pub(super) rust_sleigh_evidence: Option<fission_decompiler::RustSleighPipelineEvidence>,
}

/// Select primary text from dual layers (`code` remains NIR-faithful fallback).
pub(super) fn select_layer_text(
    rendered: &RenderedCode,
    layer: fission_pcode::PseudocodeLayer,
) -> String {
    let nir = rendered
        .code_nir
        .clone()
        .unwrap_or_else(|| rendered.code.clone());
    let hir = rendered
        .code_hir
        .clone()
        .unwrap_or_else(|| rendered.code.clone());
    let layered = fission_pcode::LayeredPseudocode { nir, hir };
    layered.format_text(layer, true)
}

pub(super) fn parse_layer_arg(raw: Option<&str>) -> fission_pcode::PseudocodeLayer {
    raw.and_then(fission_pcode::PseudocodeLayer::parse)
        .unwrap_or(fission_pcode::PseudocodeLayer::Nir)
}

pub(super) fn apply_code_filters(code: &str, no_warnings: bool, ghidra_compat: bool) -> String {
    let mut filtered = code.to_string();
    if no_warnings {
        filtered = strip_warnings(&filtered);
    }
    if ghidra_compat {
        filtered = strip_inferred_structs(&filtered);
    }
    filtered
}

/// Text body for the selected layer after warning/compat filters.
pub(super) fn filtered_layer_text(
    rendered: &RenderedCode,
    layer: fission_pcode::PseudocodeLayer,
    no_warnings: bool,
    ghidra_compat: bool,
) -> String {
    apply_code_filters(
        &select_layer_text(rendered, layer),
        no_warnings,
        ghidra_compat,
    )
}

/// Primary JSON `code` field: stays NIR-faithful unless the user selects HIR-only.
pub(super) fn filtered_primary_json_code(
    rendered: &RenderedCode,
    layer: fission_pcode::PseudocodeLayer,
    no_warnings: bool,
    ghidra_compat: bool,
) -> String {
    let raw = match layer {
        fission_pcode::PseudocodeLayer::Hir => rendered
            .code_hir
            .as_deref()
            .unwrap_or(rendered.code.as_str()),
        _ => rendered.code.as_str(),
    };
    apply_code_filters(raw, no_warnings, ghidra_compat)
}

/// Attach `layer` / `code_nir` / `code_hir` to a per-function JSON entry.
pub(super) fn attach_dual_layer_json_fields(
    entry: &mut serde_json::Value,
    rendered: &RenderedCode,
    layer: fission_pcode::PseudocodeLayer,
    no_warnings: bool,
    ghidra_compat: bool,
) {
    entry["layer"] = serde_json::json!(layer.as_str());
    entry["code_nir"] = match &rendered.code_nir {
        Some(nir) => serde_json::json!(apply_code_filters(nir, no_warnings, ghidra_compat)),
        None => serde_json::Value::Null,
    };
    entry["code_hir"] = match &rendered.code_hir {
        Some(hir) => serde_json::json!(apply_code_filters(hir, no_warnings, ghidra_compat)),
        None => serde_json::Value::Null,
    };
}

pub(super) fn write_output_bytes(cli: &OneShotArgs, body: &str) -> io::Result<()> {
    if let Some(ref output_path) = cli.output {
        fs::write(output_path, body.as_bytes())?;
        if cli.verbose {
            eprintln!("[✓] Output written to: {}", output_path.display());
        }
    } else {
        let mut stdout = io::stdout().lock();
        stdout.write_all(body.as_bytes())?;
    }
    Ok(())
}

fn render_legacy_code(
    address: u64,
    binary: &LoadedBinary,
    fact_store: &mut FactStore,
    result: fission_ffi::DecompilationResult,
) -> (String, f64) {
    let function_types = result.inferred_types;
    fact_store.ingest_native_function_types(address, function_types.clone());
    let merged_types = fact_store.merged_inferred_types(address);
    log_type_diag(
        address,
        &function_types,
        fact_store.loader_type_facts(),
        &merged_types,
    );
    let code = result.code.clone();
    let postprocess_sec = 0.0;
    (code, postprocess_sec)
}

pub(super) fn legacy_rendered_code(
    address: u64,
    binary: &LoadedBinary,
    fact_store: &mut FactStore,
    result: fission_ffi::DecompilationResult,
) -> RenderedCode {
    let (code, postprocess_sec) = render_legacy_code(address, binary, fact_store, result);
    RenderedCode {
        code,
        code_nir: None,
        code_hir: None,
        postprocess_sec,
        engine_used: NirEngineMode::Legacy.as_str(),
        fell_back: false,
        fallback_reason: None,
        preview_build_stats: None,
        preview_hint_stats: None,
        rust_sleigh_evidence: None,
    }
}

pub(super) fn decompile_code_with_profile(
    _profile: &str,
    engine_mode: EngineMode,
    decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    timeout_ms: Option<u64>,
    _verbose: bool,
) -> Result<RenderedCode, FissionError> {
    if matches!(engine_mode, EngineMode::RustSleigh) {
        let mut config = fission_decompiler::RustSleighDecompileConfig::cli_defaults();
        config.nir_timeout_ms = timeout_ms;
        let result = fission_decompiler::decompile_with_rust_sleigh(
            binary, address, name, &config, None, None,
        )
        .map_err(FissionError::decompiler)?;
        return Ok(RenderedCode {
            code: result.code,
            code_nir: result.code_nir,
            code_hir: result.code_hir,
            postprocess_sec: 0.0,
            engine_used: "rust_sleigh",
            fell_back: result.fell_back,
            fallback_reason: result.fallback_reason,
            preview_build_stats: result.build_stats,
            preview_hint_stats: result.hint_stats,
            rust_sleigh_evidence: Some(result.evidence),
        });
    }

    let mut fact_store = FactStore::from_binary(binary);
    let preview_mode = match engine_mode {
        EngineMode::Legacy => NirEngineMode::Legacy,
        EngineMode::Nir => NirEngineMode::Nir,
        EngineMode::Auto => NirEngineMode::Auto,
        EngineMode::RustSleigh => NirEngineMode::Nir,
    };
    let preview = select_nir_output_with_facts(
        &mut NativeNirSource::new(decomp),
        binary,
        &fact_store,
        address,
        name,
        preview_mode,
        timeout_ms,
    )
    .map_err(FissionError::decompiler)?;

    if let Some(code) = preview.nir_code {
        let layered = fission_pcode::take_last_layered_pseudocode();
        return Ok(RenderedCode {
            code,
            code_nir: layered.as_ref().map(|l| l.nir.clone()),
            code_hir: layered.as_ref().map(|l| l.hir.clone()),
            postprocess_sec: 0.0,
            engine_used: NirEngineMode::Nir.as_str(),
            fell_back: false,
            fallback_reason: None,
            preview_build_stats: preview.build_stats,
            preview_hint_stats: preview.hint_stats,
            rust_sleigh_evidence: None,
        });
    }

    if preview.fell_back
        && preview
            .fallback_reason
            .as_deref()
            .is_some_and(|reason| reason.to_ascii_lowercase().contains("preview_timeout"))
    {
        return Err(FissionError::decompiler(
            preview.fallback_reason.unwrap_or_else(|| {
                fallback_reason_with_kind("preview_timeout", "preview timed out")
            }),
        ));
    }

    let result = match decomp.decompile_with_metadata(address) {
        Ok(result) => result,
        Err(e) => {
            let error_text = e.to_string();
            if !matches!(engine_mode, EngineMode::Legacy) {
                if let Some(selection) = rescue_nir_output_with_facts(
                    &mut NativeNirSource::new(decomp),
                    binary,
                    &fact_store,
                    address,
                    name,
                    &error_text,
                    timeout_ms,
                )
                .map_err(FissionError::decompiler)?
                {
                    if let Some(code) = selection.nir_code {
                        let layered = fission_pcode::take_last_layered_pseudocode();
                        return Ok(RenderedCode {
                            code,
                            code_nir: layered.as_ref().map(|l| l.nir.clone()),
                            code_hir: layered.as_ref().map(|l| l.hir.clone()),
                            postprocess_sec: 0.0,
                            engine_used: NirEngineMode::Nir.as_str(),
                            fell_back: true,
                            fallback_reason: selection.fallback_reason,
                            preview_build_stats: selection.build_stats,
                            preview_hint_stats: selection.hint_stats,
                            rust_sleigh_evidence: None,
                        });
                    }
                }
            }
            return Err(e);
        }
    };
    let mut rendered = legacy_rendered_code(address, binary, &mut fact_store, result);
    rendered.fell_back = preview.fell_back;
    rendered.fallback_reason = preview.fallback_reason;
    rendered.preview_hint_stats = preview.hint_stats;
    rendered.rust_sleigh_evidence = None;
    Ok(rendered)
}
