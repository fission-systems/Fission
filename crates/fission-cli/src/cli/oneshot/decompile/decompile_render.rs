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
    pub(super) code: Result<RenderedCode, fission_core::FissionError>,
    pub(super) decomp_sec: f64,
    pub(super) postprocess_sec: f64,
    pub(super) last_timing_json: Option<String>,
}

pub(super) struct RenderedCode {
    pub(super) code: String,
    pub(super) postprocess_sec: f64,
    pub(super) engine_used: &'static str,
    pub(super) fell_back: bool,
    pub(super) fallback_reason: Option<String>,
    pub(super) preview_build_stats: Option<NirBuildStats>,
    pub(super) preview_hint_stats: Option<NirHintStats>,
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
    let postprocessor = PostProcessor::new()
        .with_inferred_types(merged_types)
        .with_dwarf_info(fact_store.dwarf_function(address).cloned())
        .with_string_map(Some(binary.inner().string_map.clone()));
    let postprocess_start = std::time::Instant::now();
    let code = postprocessor.process(&result.code);
    let postprocess_sec = postprocess_start.elapsed().as_secs_f64();
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
        postprocess_sec,
        engine_used: NirEngineMode::Legacy.as_str(),
        fell_back: false,
        fallback_reason: None,
        preview_build_stats: None,
        preview_hint_stats: None,
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
    let mut fact_store = FactStore::from_binary(binary);
    let preview_mode = match engine_mode {
        EngineMode::Legacy => NirEngineMode::Legacy,
        EngineMode::Nir => NirEngineMode::Nir,
        EngineMode::Auto => NirEngineMode::Auto,
        EngineMode::RustSleigh => NirEngineMode::Nir,
    };
    let preview = select_nir_output_with_facts(
        decomp,
        binary,
        &fact_store,
        address,
        name,
        preview_mode,
        timeout_ms,
    )
    .map_err(FissionError::decompiler)?;

    if let Some(code) = preview.nir_code {
        return Ok(RenderedCode {
            code,
            postprocess_sec: 0.0,
            engine_used: NirEngineMode::Nir.as_str(),
            fell_back: false,
            fallback_reason: None,
            preview_build_stats: preview.build_stats,
            preview_hint_stats: preview.hint_stats,
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
                    decomp,
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
                        return Ok(RenderedCode {
                            code,
                            postprocess_sec: 0.0,
                            engine_used: NirEngineMode::Nir.as_str(),
                            fell_back: true,
                            fallback_reason: selection.fallback_reason,
                            preview_build_stats: selection.build_stats,
                            preview_hint_stats: selection.hint_stats,
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
    Ok(rendered)
}
