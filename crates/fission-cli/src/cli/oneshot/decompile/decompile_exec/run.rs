use crate::cli::oneshot::debug_decomp::{
    attach_pcode_topology, debug_decomp_bundle_json, write_debug_decomp_bundle_file,
};
use super::super::decompile_render::{
    DecompEntry, RenderedCode, decompile_code_with_profile, make_assembly_fallback,
    strip_inferred_structs, strip_warnings,
};
use super::super::decompile_targets::collect_target_functions;
use super::super::*;
use super::output::{attach_native_timing_if_present, decompile_and_output};

#[cfg(test)]
fn is_terminal_control_flow(opcode: fission_decompiler::PcodeOpcode) -> bool {
    fission_sleigh::runtime::is_terminal_control_flow(opcode)
}

#[cfg(test)]
fn build_cfg_blocks(
    entry_address: u64,
    ops: Vec<fission_decompiler::PcodeOp>,
) -> Vec<fission_decompiler::PcodeBasicBlock> {
    fission_sleigh::runtime::build_cfg_blocks_from_ops(entry_address, ops, &std::collections::BTreeSet::new())
}

fn maybe_record_debug_decomp(
    cli: &OneShotArgs,
    effective_json: bool,
    binary: &LoadedBinary,
    func: &FunctionInfo,
    preview_build_stats: Option<&fission_decompiler::NirBuildStats>,
    preview_hint_stats: Option<&fission_decompiler::NirHintStats>,
    rust_sleigh_evidence: Option<&fission_decompiler::RustSleighPipelineEvidence>,
    native_timing: Option<serde_json::Value>,
    failed_hard: bool,
    assembly_fallback_no_stats: bool,
    entry: Option<&mut serde_json::Value>,
    sink: Option<&mut Vec<serde_json::Value>>,
) {
    if !cli.debug_decomp && cli.debug_decomp_bundle.is_none() {
        return;
    }
    let nt = native_timing.as_ref();
    let bundle = debug_decomp_bundle_json(
        binary,
        cli.address,
        func,
        preview_build_stats,
        preview_hint_stats,
        rust_sleigh_evidence,
        nt,
        failed_hard,
        assembly_fallback_no_stats,
    );
    let embed = cli.debug_decomp && effective_json;

    if embed {
        if let Some(entry) = entry {
            match sink {
                Some(sink) => {
                    entry["debug_decomp"] = bundle.clone();
                    sink.push(bundle);
                }
                None => {
                    entry["debug_decomp"] = bundle;
                }
            }
            return;
        }
    }

    if let Some(sink) = sink {
        sink.push(bundle);
    }
}

fn timing_json_from_storage(raw: Option<&String>) -> Option<serde_json::Value> {
    raw.and_then(|timing| {
        let t = timing.trim();
        if t.is_empty() || t == "{}" {
            None
        } else {
            serde_json::from_str(timing).ok()
        }
    })
}

fn attach_native_pcode_topology(
    decomp: &mut DecompilerNative,
    address: u64,
    entry: Option<&mut serde_json::Value>,
    sink: Option<&mut Vec<serde_json::Value>>,
) {
    let Ok(pcode_json) = decomp.get_pcode(address) else {
        return;
    };
    let Ok(pcode) = PcodeFunction::from_json(&pcode_json) else {
        return;
    };
    if let Some(entry) = entry
        && let Some(debug) = entry.get_mut("debug_decomp")
    {
        attach_pcode_topology(debug, &pcode);
    }
    if let Some(sink) = sink
        && let Some(last) = sink.last_mut()
    {
        attach_pcode_topology(last, &pcode);
    }
}

fn render_with_rust_sleigh(
    binary: &LoadedBinary,
    func: &FunctionInfo,
) -> Result<RenderedCode, FissionError> {
    let _load_spec = binary.load_spec().ok_or_else(|| {
        FissionError::decompiler(format!(
            "rust_sleigh: missing Ghidra load spec for '{}'",
            binary.path
        ))
    })?;
    let config = fission_decompiler::RustSleighDecompileConfig::cli_defaults();
    let result = fission_decompiler::decompile_with_rust_sleigh(
        binary,
        func.address,
        &func.name,
        &config,
        None,
        None,
    )
    .map_err(FissionError::decompiler)?;

    Ok(RenderedCode {
        code: result.code,
        postprocess_sec: 0.0,
        engine_used: "rust_sleigh",
        fell_back: result.fell_back,
        fallback_reason: result.fallback_reason,
        preview_build_stats: result.build_stats,
        preview_hint_stats: result.hint_stats,
        rust_sleigh_evidence: Some(result.evidence),
    })
}

fn run_rust_sleigh_decompilation(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    binary_data: &[u8],
    functions: &[&FunctionInfo],
    selection_accounting: crate::cli::oneshot::function_select::BatchSelectionAccounting,
    effective_no_header: bool,
    effective_no_warnings: bool,
    effective_json: bool,
    init_start: std::time::Instant,
    debug_bundle_sink: Option<&mut Vec<serde_json::Value>>,
) -> io::Result<()> {
    let mut all_output = String::new();
    let mut json_results = Vec::new();
    let mut total_decomp_secs = 0.0;
    let mut total_postprocess_secs = 0.0;

    for func in functions {
        let start = std::time::Instant::now();
        match render_with_rust_sleigh(binary, func) {
            Ok(rendered) => {
                let decomp_sec = start.elapsed().as_secs_f64();
                total_decomp_secs += decomp_sec;
                total_postprocess_secs += rendered.postprocess_sec;

                let mut filtered = rendered.code.clone();
                if effective_no_warnings {
                    filtered = strip_warnings(&filtered);
                }
                if cli.ghidra_compat {
                    filtered = strip_inferred_structs(&filtered);
                }

                if effective_json {
                    let mut entry = serde_json::json!({
                        "address": format!("0x{:x}", func.address),
                        "name": func.name,
                        "size": func.size,
                        "code": filtered,
                        "engine_used": rendered.engine_used,
                        "fell_back": rendered.fell_back,
                        "fallback_reason": rendered.fallback_reason,
                    });
                    if let Some(stats) = rendered.preview_build_stats {
                        entry["preview_build_stats"] = serde_json::json!(stats);
                    }
                    if let Some(stats) = rendered.preview_hint_stats {
                        entry["preview_hint_stats"] = serde_json::json!(stats);
                    }
                    if cli.benchmark {
                        entry["decomp_sec"] =
                            serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                        entry["postprocess_sec"] = serde_json::json!(
                            (rendered.postprocess_sec * 1_000_000.0).round() / 1_000_000.0
                        );
                    }
                    maybe_record_debug_decomp(
                        cli,
                        effective_json,
                        binary,
                        func,
                        rendered.preview_build_stats.as_ref(),
                        rendered.preview_hint_stats.as_ref(),
                        rendered.rust_sleigh_evidence.as_ref(),
                        None,
                        false,
                        rendered.preview_build_stats.is_none() && rendered.fell_back,
                        Some(&mut entry),
                        debug_bundle_sink.as_mut(),
                    );
                    json_results.push(entry);
                } else {
                    if !effective_no_header {
                        all_output.push_str("// ============================================\n");
                        all_output.push_str(&format!(
                            "// Function: {} @ 0x{:x}\n",
                            func.name, func.address
                        ));
                        all_output.push_str("// ============================================\n\n");
                    }
                    all_output.push_str(&filtered);
                    all_output.push_str("\n\n");
                    maybe_record_debug_decomp(
                        cli,
                        effective_json,
                        binary,
                        func,
                        rendered.preview_build_stats.as_ref(),
                        rendered.preview_hint_stats.as_ref(),
                        rendered.rust_sleigh_evidence.as_ref(),
                        None,
                        false,
                        rendered.preview_build_stats.is_none() && rendered.fell_back,
                        None,
                        debug_bundle_sink.as_mut(),
                    );
                }
            }
            Err(e) => {
                let decomp_sec = start.elapsed().as_secs_f64();
                total_decomp_secs += decomp_sec;
                let error_text = e.to_string();
                if let Some(fallback) =
                    make_assembly_fallback(binary, binary_data, func, &error_text)
                {
                    if effective_json {
                        let mut entry = serde_json::json!({
                            "address": format!("0x{:x}", func.address),
                            "name": func.name,
                            "size": func.size,
                            "code": fallback,
                            "engine_used": "rust_sleigh",
                            "fell_back": true,
                            "fallback": "assembly",
                            "fallback_reason": fallback_reason_with_kind("assembly_fallback", &error_text),
                        });
                        if cli.benchmark {
                            entry["decomp_sec"] =
                                serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                        }
                        maybe_record_debug_decomp(
                            cli,
                            effective_json,
                            binary,
                            func,
                            None,
                            None,
                            None,
                            None,
                            false,
                            true,
                            Some(&mut entry),
                            debug_bundle_sink.as_mut(),
                        );
                        json_results.push(entry);
                    } else {
                        if !effective_no_header {
                            all_output
                                .push_str("// ============================================\n");
                            all_output.push_str(&format!(
                                "// Function: {} @ 0x{:x}\n",
                                func.name, func.address
                            ));
                            all_output
                                .push_str("// ============================================\n\n");
                        }
                        all_output.push_str(&fallback);
                        all_output.push_str("\n\n");
                        maybe_record_debug_decomp(
                            cli,
                            effective_json,
                            binary,
                            func,
                            None,
                            None,
                            None,
                            None,
                            false,
                            true,
                            None,
                            debug_bundle_sink.as_mut(),
                        );
                    }
                } else if effective_json {
                    let mut entry = serde_json::json!({
                        "address": format!("0x{:x}", func.address),
                        "name": func.name,
                        "size": func.size,
                        "engine_used": "rust_sleigh",
                        "fell_back": true,
                        "fallback_reason": fallback_reason_with_kind("rust_sleigh", &error_text),
                        "error": error_text,
                    });
                    if cli.benchmark {
                        entry["decomp_sec"] =
                            serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                    }
                    maybe_record_debug_decomp(
                        cli,
                        effective_json,
                        binary,
                        func,
                        None,
                        None,
                        None,
                        None,
                        true,
                        false,
                        Some(&mut entry),
                        debug_bundle_sink.as_mut(),
                    );
                    json_results.push(entry);
                } else {
                    all_output.push_str(&format!(
                        "// Error decompiling {} (0x{:x}): {}\n\n",
                        func.name, func.address, error_text
                    ));
                    maybe_record_debug_decomp(
                        cli,
                        effective_json,
                        binary,
                        func,
                        None,
                        None,
                        None,
                        None,
                        true,
                        false,
                        None,
                        debug_bundle_sink.as_mut(),
                    );
                }
            }
        }
    }

    if let Some(rows) = debug_bundle_sink {
        if let Some(ref path) = cli.debug_decomp_bundle {
            write_debug_decomp_bundle_file(path, rows)?;
        }
    }

    let final_output = if cli.benchmark {
        let envelope = serde_json::json!({
            "_meta": {
                "tool": "fission",
                "version": env!("CARGO_PKG_VERSION"),
                "profile": cli.profile.as_deref().unwrap_or("balanced"),
                "engine": "rust-sleigh",
                "function_count": functions.len(),
                "functions_discovered_total": selection_accounting.functions_discovered_total,
                "functions_selected_total": selection_accounting.functions_selected_total,
                "functions_excluded_import_count": selection_accounting.functions_excluded_import_count,
                "functions_excluded_runtime_wrapper_count": selection_accounting.functions_excluded_runtime_wrapper_count,
                "functions_excluded_provenance_count": selection_accounting.functions_excluded_provenance_count,
                "include_nonuser_functions": selection_accounting.include_nonuser_functions,
                "init_sec": 0.0,
                "total_decomp_sec": (total_decomp_secs * 1_000_000.0).round() / 1_000_000.0,
                "total_postprocess_sec": (total_postprocess_secs * 1_000_000.0).round() / 1_000_000.0,
                "wall_clock_sec": (init_start.elapsed().as_secs_f64() * 1_000_000.0).round() / 1_000_000.0,
            },
            "functions": json_results,
        });
        serde_json::to_string_pretty(&envelope)
            .map_err(|e| io::Error::other(format!("JSON serialization failed: {}", e)))?
    } else if effective_json {
        serde_json::to_string_pretty(&json_results)
            .map_err(|e| io::Error::other(format!("JSON serialization failed: {}", e)))?
    } else {
        all_output
    };

    if let Some(ref output_path) = cli.output {
        let mut file = fs::File::create(output_path).map_err(|e| {
            io::Error::other(format!(
                "Failed to create output file '{}': {}",
                output_path.display(),
                e
            ))
        })?;
        file.write_all(final_output.as_bytes())?;
        if cli.verbose {
            eprintln!("[✓] Output written to: {}", output_path.display());
        }
    } else {
        let mut stdout = io::stdout().lock();
        stdout.write_all(final_output.as_bytes())?;
    }
    Ok(())
}

fn run_sequential_decompilation<'a>(
    cli: &OneShotArgs,
    decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    binary_data: &[u8],
    functions: &[&'a FunctionInfo],
    selected_profile: &str,
    engine_mode: EngineMode,
    effective_no_header: bool,
    effective_no_warnings: bool,
    effective_json: bool,
    debug_bundle_sink: Option<&mut Vec<serde_json::Value>>,
) -> (String, Vec<serde_json::Value>, f64, f64) {
    let mut all_output = String::new();
    let mut json_results = Vec::new();
    let mut total_decomp_secs = 0.0;
    let mut total_postprocess_secs = 0.0;
    for func in functions {
        if cli.verbose {
            eprintln!("[*] Decompiling {} (0x{:x})...", func.name, func.address);
        }

        let _silencer = OutputSilencer::new_if(!cli.verbose);
        let func_start = std::time::Instant::now();
        match decompile_code_with_profile(
            selected_profile,
            engine_mode,
            decomp,
            binary,
            func.address,
            &func.name,
            cli.timeout_ms,
            cli.verbose,
        ) {
            Ok(rendered) => {
                let postprocess_sec = rendered.postprocess_sec;
                let decomp_sec = func_start.elapsed().as_secs_f64();
                total_decomp_secs += decomp_sec;
                total_postprocess_secs += postprocess_sec;
                let mut filtered = rendered.code.clone();
                if effective_no_warnings {
                    filtered = strip_warnings(&filtered);
                }
                if cli.ghidra_compat {
                    filtered = strip_inferred_structs(&filtered);
                }

                if effective_json {
                    let mut entry = serde_json::json!({
                        "address": format!("0x{:x}", func.address),
                        "name": func.name,
                        "size": func.size,
                        "code": filtered,
                        "engine_used": rendered.engine_used,
                        "fell_back": rendered.fell_back,
                        "fallback_reason": rendered.fallback_reason,
                    });
                    if let Some(stats) = rendered.preview_build_stats {
                        entry["preview_build_stats"] = serde_json::json!(stats);
                    }
                    if let Some(stats) = rendered.preview_hint_stats {
                        entry["preview_hint_stats"] = serde_json::json!(stats);
                    }
                    if cli.benchmark {
                        entry["decomp_sec"] =
                            serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                        entry["postprocess_sec"] = serde_json::json!(
                            (postprocess_sec * 1_000_000.0).round() / 1_000_000.0
                        );
                        attach_native_timing_if_present(&mut entry, decomp);
                    }
                    let native_timing = decomp.get_last_timing_json().ok().and_then(|s| {
                        let t = s.trim();
                        if t.is_empty() || t == "{}" {
                            None
                        } else {
                            serde_json::from_str(&s).ok()
                        }
                    });
                    maybe_record_debug_decomp(
                        cli,
                        effective_json,
                        binary,
                        func,
                        rendered.preview_build_stats.as_ref(),
                        rendered.preview_hint_stats.as_ref(),
                        rendered.rust_sleigh_evidence.as_ref(),
                        native_timing,
                        false,
                        rendered.preview_build_stats.is_none() && rendered.fell_back,
                        Some(&mut entry),
                        debug_bundle_sink.as_mut(),
                    );
                    attach_native_pcode_topology(
                        decomp,
                        func.address,
                        Some(&mut entry),
                        debug_bundle_sink.as_mut(),
                    );
                    json_results.push(entry);
                } else {
                    if !effective_no_header {
                        all_output.push_str("// ============================================\n");
                        all_output.push_str(&format!(
                            "// Function: {} @ 0x{:x}\n",
                            func.name, func.address
                        ));
                        all_output.push_str("// ============================================\n\n");
                    }
                    all_output.push_str(&filtered);
                    all_output.push_str("\n\n");
                    let native_timing = decomp.get_last_timing_json().ok().and_then(|s| {
                        let t = s.trim();
                        if t.is_empty() || t == "{}" {
                            None
                        } else {
                            serde_json::from_str(&s).ok()
                        }
                    });
                    maybe_record_debug_decomp(
                        cli,
                        effective_json,
                        binary,
                        func,
                        rendered.preview_build_stats.as_ref(),
                        rendered.preview_hint_stats.as_ref(),
                        rendered.rust_sleigh_evidence.as_ref(),
                        native_timing,
                        false,
                        rendered.preview_build_stats.is_none() && rendered.fell_back,
                        None,
                        debug_bundle_sink.as_mut(),
                    );
                    attach_native_pcode_topology(
                        decomp,
                        func.address,
                        None,
                        debug_bundle_sink.as_mut(),
                    );
                }
            }
            Err(e) => {
                let decomp_sec = func_start.elapsed().as_secs_f64();
                total_decomp_secs += decomp_sec;
                let error_text = e.to_string();
                if let Some(fallback) =
                    make_assembly_fallback(binary, binary_data, func, &error_text)
                {
                    if effective_json {
                        let fallback_class = classify_native_failure_kind(&error_text);
                        let mut entry = serde_json::json!({
                            "address": format!("0x{:x}", func.address),
                            "name": func.name,
                            "size": func.size,
                            "code": fallback,
                            "engine_used": NirEngineMode::Legacy.as_str(),
                            "fell_back": true,
                            "fallback": "assembly",
                            "fallback_reason": fallback_reason_with_kind("assembly_fallback", &error_text),
                            "fallback_class": fallback_class
                        });
                        if cli.benchmark {
                            entry["decomp_sec"] =
                                serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                            attach_native_timing_if_present(&mut entry, decomp);
                        }
                        let native_timing = decomp.get_last_timing_json().ok().and_then(|s| {
                            let t = s.trim();
                            if t.is_empty() || t == "{}" {
                                None
                            } else {
                                serde_json::from_str(&s).ok()
                            }
                        });
                        maybe_record_debug_decomp(
                            cli,
                            effective_json,
                            binary,
                            func,
                            None,
                            None,
                            None,
                            native_timing,
                            false,
                            true,
                            Some(&mut entry),
                            debug_bundle_sink.as_mut(),
                        );
                        json_results.push(entry);
                    } else {
                        if !effective_no_header {
                            all_output
                                .push_str("// ============================================\n");
                            all_output.push_str(&format!(
                                "// Function: {} @ 0x{:x}\n",
                                func.name, func.address
                            ));
                            all_output
                                .push_str("// ============================================\n\n");
                        }
                        all_output.push_str(&fallback);
                        all_output.push_str("\n\n");
                        let native_timing = decomp.get_last_timing_json().ok().and_then(|s| {
                            let t = s.trim();
                            if t.is_empty() || t == "{}" {
                                None
                            } else {
                                serde_json::from_str(&s).ok()
                            }
                        });
                        maybe_record_debug_decomp(
                            cli,
                            effective_json,
                            binary,
                            func,
                            None,
                            None,
                            None,
                            native_timing,
                            false,
                            true,
                            None,
                            debug_bundle_sink.as_mut(),
                        );
                    }
                    continue;
                }
                if effective_json {
                    let routing =
                        fission_decompiler::native_failure_routing_decision(&error_text);
                    let mut entry = serde_json::json!({
                        "address": format!("0x{:x}", func.address),
                        "name": func.name,
                        "size": func.size,
                        "engine_used": match routing.engine_used {
                            NirEngineMode::Legacy => NirEngineMode::Legacy.as_str(),
                            NirEngineMode::Nir => NirEngineMode::Nir.as_str(),
                            NirEngineMode::Auto => NirEngineMode::Auto.as_str(),
                        },
                        "fell_back": routing.fell_back,
                        "fallback_reason": routing.fallback_reason,
                        "error": error_text
                    });
                    if cli.benchmark {
                        entry["decomp_sec"] =
                            serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                        attach_native_timing_if_present(&mut entry, decomp);
                    }
                    let native_timing = decomp.get_last_timing_json().ok().and_then(|s| {
                        let t = s.trim();
                        if t.is_empty() || t == "{}" {
                            None
                        } else {
                            serde_json::from_str(&s).ok()
                        }
                    });
                    maybe_record_debug_decomp(
                        cli,
                        effective_json,
                        binary,
                        func,
                        None,
                        None,
                        None,
                        native_timing,
                        true,
                        false,
                        Some(&mut entry),
                        debug_bundle_sink.as_mut(),
                    );
                    json_results.push(entry);
                } else {
                    all_output.push_str(&format!(
                        "// Error decompiling {} (0x{:x}): {}\n\n",
                        func.name, func.address, error_text
                    ));
                    let native_timing = decomp.get_last_timing_json().ok().and_then(|s| {
                        let t = s.trim();
                        if t.is_empty() || t == "{}" {
                            None
                        } else {
                            serde_json::from_str(&s).ok()
                        }
                    });
                    maybe_record_debug_decomp(
                        cli,
                        effective_json,
                        binary,
                        func,
                        None,
                        None,
                        None,
                        native_timing,
                        true,
                        false,
                        None,
                        debug_bundle_sink.as_mut(),
                    );
                }
            }
        }
    }

    (
        all_output,
        json_results,
        total_decomp_secs,
        total_postprocess_secs,
    )
}

#[cfg(feature = "native_decomp")]
fn run_parallel_decompilation<'a>(
    cli: &OneShotArgs,
    main_decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    binary_data: &[u8],
    functions: &[&'a FunctionInfo],
    _prepare_timings: &PrepareTimings,
    selected_profile: &str,
    engine_mode: EngineMode,
    _init_elapsed_sec: f64,
    _init_start: std::time::Instant,
    effective_no_header: bool,
    effective_no_warnings: bool,
    effective_json: bool,
    debug_bundle_sink: Option<&mut Vec<serde_json::Value>>,
) -> (String, Vec<serde_json::Value>, f64, f64) {
    let (compiler_id, _) = resolve_compiler_id(binary, cli.compiler_id.as_deref());
    let config = fission_core::config::Config::default();
    let gdt_path_owned = fission_core::PATHS
        .get_gdt_path(binary.is_64bit)
        .and_then(|p| p.to_str().map(String::from));
    let num_workers = 8;

    let mut buckets: Vec<Vec<&'a FunctionInfo>> = (0..num_workers).map(|_| Vec::new()).collect();
    for (i, func) in functions.iter().enumerate() {
        buckets[i % num_workers].push(*func);
    }

    let first_bucket_entries = if !buckets[0].is_empty() {
        let mut entries = Vec::with_capacity(buckets[0].len());
        for func in &buckets[0] {
            let start = std::time::Instant::now();
            let code_result = decompile_code_with_profile(
                selected_profile,
                engine_mode,
                main_decomp,
                binary,
                func.address,
                &func.name,
                cli.timeout_ms,
                false,
            );
            let decomp_sec = start.elapsed().as_secs_f64();
            let (code_result, postprocess_sec) = match code_result {
                Ok(rendered) => {
                    let postprocess_sec = rendered.postprocess_sec;
                    (Ok(rendered), postprocess_sec)
                }
                Err(e) => {
                    let error_text = e.to_string();
                    if let Some(fallback) =
                        make_assembly_fallback(binary, binary_data, func, &error_text)
                    {
                        (
                            Ok(RenderedCode {
                                code: fallback,
                                postprocess_sec: 0.0,
                                engine_used: NirEngineMode::Legacy.as_str(),
                                fell_back: true,
                                fallback_reason: Some(fallback_reason_with_kind(
                                    "assembly_fallback",
                                    &error_text,
                                )),
                                preview_build_stats: None,
                                preview_hint_stats: None,
                                rust_sleigh_evidence: None,
                            }),
                            0.0,
                        )
                    } else {
                        (Err(e), 0.0)
                    }
                }
            };
            let timing = main_decomp.get_last_timing_json().ok();
            entries.push(DecompEntry {
                address: func.address,
                name: func.name.clone(),
                size: func.size,
                code: code_result,
                decomp_sec,
                postprocess_sec,
                last_timing_json: timing,
            });
        }
        entries
    } else {
        Vec::new()
    };

    let signatures_json = serialize_api_signatures_json();

    let rest_buckets: Vec<_> = buckets.into_iter().skip(1).collect();
    let rest_results: Vec<Vec<DecompEntry>> = rest_buckets
        .par_iter()
        .map(|bucket| {
            let mut decomp = init_decompiler(false);
            apply_profile(&mut decomp, selected_profile);
            let mut opts = PrepareOptions {
                verbose: false,
                compiler_id: compiler_id.as_deref(),
                gdt_path: gdt_path_owned.as_deref(),
                timeout_ms: Some(cli.timeout_ms.unwrap_or(config.decompiler.timeout_ms)),
                timings: None,
                signatures_json: signatures_json.as_deref(),
            };
            if prepare_native_decompiler_for_binary(&mut decomp, binary, binary_data, &mut opts)
                .is_err()
            {
                return bucket
                    .iter()
                    .map(|f| DecompEntry {
                        address: f.address,
                        name: f.name.clone(),
                        size: f.size,
                        code: Err(fission_core::FissionError::decompiler("Prepare failed")),
                        decomp_sec: 0.0,
                        postprocess_sec: 0.0,
                        last_timing_json: None,
                    })
                    .collect();
            }

            let mut entries = Vec::with_capacity(bucket.len());
            for func in bucket.iter().copied() {
                let start = std::time::Instant::now();
                let code_result = decompile_code_with_profile(
                    selected_profile,
                    engine_mode,
                    &mut decomp,
                    binary,
                    func.address,
                    &func.name,
                    cli.timeout_ms,
                    false,
                );
                let decomp_sec = start.elapsed().as_secs_f64();
                let (code_result, postprocess_sec) = match code_result {
                    Ok(rendered) => {
                        let postprocess_sec = rendered.postprocess_sec;
                        (Ok(rendered), postprocess_sec)
                    }
                    Err(e) => {
                        let error_text = e.to_string();
                        if let Some(fallback) =
                            make_assembly_fallback(binary, binary_data, func, &error_text)
                        {
                            (
                                Ok(RenderedCode {
                                    code: fallback,
                                    postprocess_sec: 0.0,
                                    engine_used: NirEngineMode::Legacy.as_str(),
                                    fell_back: true,
                                    fallback_reason: Some(fallback_reason_with_kind(
                                        "assembly_fallback",
                                        &error_text,
                                    )),
                                    preview_build_stats: None,
                                    preview_hint_stats: None,
                                    rust_sleigh_evidence: None,
                                }),
                                0.0,
                            )
                        } else {
                            (Err(e), 0.0)
                        }
                    }
                };
                let timing = decomp.get_last_timing_json().ok();
                entries.push(DecompEntry {
                    address: func.address,
                    name: func.name.clone(),
                    size: func.size,
                    code: code_result,
                    decomp_sec,
                    postprocess_sec,
                    last_timing_json: timing,
                });
            }
            entries
        })
        .collect();

    let all_entries: Vec<DecompEntry> = {
        let mut entries: Vec<DecompEntry> = first_bucket_entries
            .into_iter()
            .chain(rest_results.into_iter().flatten())
            .collect();
        entries.sort_by_key(|e| e.address);
        entries
    };

    let mut all_output = String::new();
    let mut json_results = Vec::new();
    let mut total_decomp_secs = 0.0;
    let mut total_postprocess_secs = 0.0;

    for entry in all_entries {
        total_decomp_secs += entry.decomp_sec;
        total_postprocess_secs += entry.postprocess_sec;

        match &entry.code {
            Ok(rendered) => {
                let func_meta = binary.function_at_exact(entry.address).cloned().unwrap_or_else(|| {
                    FunctionInfo {
                        name: entry.name.clone(),
                        address: entry.address,
                        size: entry.size,
                        ..Default::default()
                    }
                });
                let mut filtered = rendered.code.clone();
                if effective_no_warnings {
                    filtered = strip_warnings(&filtered);
                }
                if cli.ghidra_compat {
                    filtered = strip_inferred_structs(&filtered);
                }

                if effective_json {
                    let mut json_entry = serde_json::json!({
                        "address": format!("0x{:x}", entry.address),
                        "name": entry.name,
                        "size": entry.size,
                        "code": filtered,
                        "engine_used": rendered.engine_used,
                        "fell_back": rendered.fell_back,
                        "fallback_reason": rendered.fallback_reason,
                    });
                    if let Some(stats) = rendered.preview_build_stats {
                        json_entry["preview_build_stats"] = serde_json::json!(stats);
                    }
                    if let Some(stats) = rendered.preview_hint_stats {
                        json_entry["preview_hint_stats"] = serde_json::json!(stats);
                    }
                    if cli.benchmark {
                        json_entry["decomp_sec"] = serde_json::json!(
                            (entry.decomp_sec * 1_000_000.0).round() / 1_000_000.0
                        );
                        json_entry["postprocess_sec"] = serde_json::json!(
                            (entry.postprocess_sec * 1_000_000.0).round() / 1_000_000.0
                        );
                        if let Some(ref timing) = entry.last_timing_json {
                            if !timing.is_empty() && timing != "{}" {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(timing) {
                                    json_entry["native_timing"] = v;
                                }
                            }
                        }
                    }
                    let native_timing = timing_json_from_storage(entry.last_timing_json.as_ref());
                    maybe_record_debug_decomp(
                        cli,
                        effective_json,
                        binary,
                        &func_meta,
                        rendered.preview_build_stats.as_ref(),
                        rendered.preview_hint_stats.as_ref(),
                        rendered.rust_sleigh_evidence.as_ref(),
                        native_timing,
                        false,
                        rendered.preview_build_stats.is_none() && rendered.fell_back,
                        Some(&mut json_entry),
                        debug_bundle_sink.as_mut(),
                    );
                    json_results.push(json_entry);
                } else {
                    if !effective_no_header {
                        all_output.push_str("// ============================================\n");
                        all_output.push_str(&format!(
                            "// Function: {} @ 0x{:x}\n",
                            entry.name, entry.address
                        ));
                        all_output.push_str("// ============================================\n\n");
                    }
                    all_output.push_str(&filtered);
                    all_output.push_str("\n\n");
                    let native_timing = timing_json_from_storage(entry.last_timing_json.as_ref());
                    maybe_record_debug_decomp(
                        cli,
                        effective_json,
                        binary,
                        &func_meta,
                        rendered.preview_build_stats.as_ref(),
                        rendered.preview_hint_stats.as_ref(),
                        rendered.rust_sleigh_evidence.as_ref(),
                        native_timing,
                        false,
                        rendered.preview_build_stats.is_none() && rendered.fell_back,
                        None,
                        debug_bundle_sink.as_mut(),
                    );
                }
            }
            Err(e) => {
                let func_meta = binary.function_at_exact(entry.address).cloned().unwrap_or_else(|| {
                    FunctionInfo {
                        name: entry.name.clone(),
                        address: entry.address,
                        size: entry.size,
                        ..Default::default()
                    }
                });
                if effective_json {
                    let mut json_entry = serde_json::json!({
                        "address": format!("0x{:x}", entry.address),
                        "name": entry.name,
                        "size": entry.size,
                        "engine_used": NirEngineMode::Legacy.as_str(),
                        "fell_back": true,
                        "fallback_reason": fallback_reason_with_kind(classify_native_failure_kind(&e.to_string()), e.to_string()),
                        "error": e.to_string()
                    });
                    if cli.benchmark {
                        json_entry["decomp_sec"] = serde_json::json!(
                            (entry.decomp_sec * 1_000_000.0).round() / 1_000_000.0
                        );
                        if let Some(ref timing) = entry.last_timing_json {
                            if !timing.is_empty() && timing != "{}" {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(timing) {
                                    json_entry["native_timing"] = v;
                                }
                            }
                        }
                    }
                    let native_timing = timing_json_from_storage(entry.last_timing_json.as_ref());
                    maybe_record_debug_decomp(
                        cli,
                        effective_json,
                        binary,
                        &func_meta,
                        None,
                        None,
                        None,
                        native_timing,
                        true,
                        false,
                        Some(&mut json_entry),
                        debug_bundle_sink.as_mut(),
                    );
                    json_results.push(json_entry);
                } else {
                    all_output.push_str(&format!(
                        "// Error decompiling {} (0x{:x}): {}\n\n",
                        entry.name, entry.address, e
                    ));
                    let native_timing = timing_json_from_storage(entry.last_timing_json.as_ref());
                    maybe_record_debug_decomp(
                        cli,
                        effective_json,
                        binary,
                        &func_meta,
                        None,
                        None,
                        None,
                        native_timing,
                        true,
                        false,
                        None,
                        debug_bundle_sink.as_mut(),
                    );
                }
            }
        }
    }

    (
        all_output,
        json_results,
        total_decomp_secs,
        total_postprocess_secs,
    )
}

pub(crate) fn run_decompilation(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    binary_data: &[u8],
) -> io::Result<()> {
    let init_start = std::time::Instant::now();
    let (selected_profile, unknown_profile) = resolve_profile(cli.profile.as_deref());
    let (engine_mode, unknown_engine, deprecated_engine_alias, deprecated_profile_alias) =
        resolve_engine_mode(cli.engine.as_deref(), cli.profile.as_deref());
    if let Some(other) = unknown_profile {
        eprintln!(
            "[!] Unknown --profile '{}', using balanced (quality|speed|balanced|nir)",
            other
        );
        warn!(
            profile = other,
            "unknown decompilation profile, using balanced"
        );
    }
    if let Some(other) = unknown_engine {
        eprintln!(
            "[!] Unknown --engine '{}', using auto (nir|auto|rust-sleigh)",
            other
        );
        warn!(engine = other, "unknown decompilation engine, using auto");
    }
    if matches!(engine_mode, EngineMode::Legacy) && cli.verbose {
        eprintln!(
            "[*] '--engine legacy' is a hidden compatibility mode; preview-first remains the product default"
        );
    }
    if deprecated_engine_alias && cli.verbose {
        eprintln!("[*] '--engine mlil-preview' is deprecated; use '--engine nir'");
    }
    if deprecated_profile_alias && cli.verbose {
        eprintln!(
            "[*] '--profile mlil-preview' is deprecated; use '--engine nir --profile quality'"
        );
    }
    if cli.verbose {
        eprintln!("[*] Decompilation profile = {}", selected_profile);
        eprintln!("[*] Decompilation engine = {:?}", engine_mode);
    }

    let selected_functions = collect_target_functions(cli, binary);
    let selection_accounting = selected_functions.accounting;
    let functions = selected_functions.functions;

    let mut debug_bundle_file_rows = cli.debug_decomp_bundle.as_ref().map(|_| Vec::new());

    if matches!(engine_mode, EngineMode::RustSleigh) {
        let effective_no_header = cli.no_header || cli.ghidra_compat;
        let effective_no_warnings = cli.no_warnings || cli.ghidra_compat;
        let effective_json = cli.json || cli.benchmark;

        if functions.is_empty() && cli.address.is_some() {
            if let Some(addr) = cli.address {
                let synthetic = FunctionInfo {
                    name: format!("sub_{:x}", addr),
                    address: addr,
                    size: 0,
                    is_export: false,
                    is_import: false,
                    ..Default::default()
                };
                let one = [&synthetic];
                return run_rust_sleigh_decompilation(
                    cli,
                    binary,
                    binary_data,
                    &one,
                    selection_accounting,
                    effective_no_header,
                    effective_no_warnings,
                    effective_json,
                    init_start,
                    debug_bundle_file_rows.as_mut(),
                );
            }
        }

        return run_rust_sleigh_decompilation(
            cli,
            binary,
            binary_data,
            &functions,
            selection_accounting,
            effective_no_header,
            effective_no_warnings,
            effective_json,
            init_start,
            debug_bundle_file_rows.as_mut(),
        );
    }

    let mut decomp = init_decompiler(cli.verbose);
    apply_profile(&mut decomp, selected_profile);

    let mut prepare_timings = PrepareTimings::default();
    {
        let (compiler_id, unknown_compiler) =
            resolve_compiler_id(binary, cli.compiler_id.as_deref());
        if let Some(user_compiler) = unknown_compiler {
            eprintln!(
                "[!] Unknown --compiler-id '{}', falling back to auto detection",
                user_compiler
            );
            warn!(
                compiler_id = user_compiler,
                "unknown compiler-id, falling back to auto detection"
            );
        }
        if cli.verbose {
            eprintln!(
                "[*] Decompiler compiler_id = {}",
                compiler_id.as_deref().unwrap_or("default")
            );
        }
        let config = fission_core::config::Config::default();
        let gdt_path_owned = fission_core::PATHS
            .get_gdt_path(binary.is_64bit)
            .and_then(|p| p.to_str().map(String::from));
        let signatures_json = serialize_api_signatures_json();
        let mut options = PrepareOptions {
            verbose: cli.verbose,
            compiler_id: compiler_id.as_deref(),
            gdt_path: gdt_path_owned.as_deref(),
            timeout_ms: Some(cli.timeout_ms.unwrap_or(config.decompiler.timeout_ms)),
            timings: if cli.benchmark {
                Some(&mut prepare_timings)
            } else {
                None
            },
            signatures_json: signatures_json.as_deref(),
        };
        if let Err(e) =
            prepare_native_decompiler_for_binary(&mut decomp, binary, binary_data, &mut options)
        {
            eprintln!("Error: Failed to prepare decompiler: {}", e);
            std::process::exit(1);
        }
    }

    let init_elapsed = init_start.elapsed();
    if cli.verbose {
        eprintln!(
            "[✓] Decompiler ready (init: {:.3}s)",
            init_elapsed.as_secs_f64()
        );
    }

    if functions.is_empty() && cli.address.is_some() {
        if let Some(addr) = cli.address {
            eprintln!("Warning: No function found at address 0x{:x}", addr);
            decompile_and_output(
                cli,
                &mut decomp,
                binary,
                binary_data,
                selected_profile,
                engine_mode,
                addr,
                &format!("sub_{:x}", addr),
            )?;
        }
        return Ok(());
    }

    let effective_no_header = cli.no_header || cli.ghidra_compat;
    let effective_no_warnings = cli.no_warnings || cli.ghidra_compat;
    let effective_json = cli.json || cli.benchmark;

    let use_parallel = (cli.decomp_all || cli.decomp_limit.is_some()) && functions.len() > 1;

    let (all_output, json_results, total_decomp_secs, total_postprocess_secs) = if use_parallel {
        run_parallel_decompilation(
            cli,
            &mut decomp,
            binary,
            binary_data,
            &functions,
            &prepare_timings,
            selected_profile,
            engine_mode,
            init_elapsed.as_secs_f64(),
            init_start,
            effective_no_header,
            effective_no_warnings,
            effective_json,
            debug_bundle_file_rows.as_mut(),
        )
    } else {
        run_sequential_decompilation(
            cli,
            &mut decomp,
            binary,
            binary_data,
            &functions,
            selected_profile,
            engine_mode,
            effective_no_header,
            effective_no_warnings,
            effective_json,
            debug_bundle_file_rows.as_mut(),
        )
    };

    if let Some(ref path) = cli.debug_decomp_bundle {
        if let Some(rows) = debug_bundle_file_rows.as_ref() {
            write_debug_decomp_bundle_file(path, rows)?;
        }
    }

    let final_output = if cli.benchmark {
        let envelope = serde_json::json!({
            "_meta": {
                "tool": "fission",
                "version": env!("CARGO_PKG_VERSION"),
                "profile": cli.profile.as_deref().unwrap_or("balanced"),
                "engine": cli.engine.as_deref().unwrap_or("auto"),
                "function_count": functions.len(),
                "functions_discovered_total": selection_accounting.functions_discovered_total,
                "functions_selected_total": selection_accounting.functions_selected_total,
                "functions_excluded_import_count": selection_accounting.functions_excluded_import_count,
                "functions_excluded_runtime_wrapper_count": selection_accounting.functions_excluded_runtime_wrapper_count,
                "functions_excluded_provenance_count": selection_accounting.functions_excluded_provenance_count,
                "include_nonuser_functions": selection_accounting.include_nonuser_functions,
                "init_sec": (init_elapsed.as_secs_f64() * 1_000_000.0).round() / 1_000_000.0,
                "prepare_timings": &prepare_timings,
                "total_decomp_sec": (total_decomp_secs * 1_000_000.0).round() / 1_000_000.0,
                "total_postprocess_sec": (total_postprocess_secs * 1_000_000.0).round() / 1_000_000.0,
                "wall_clock_sec": (init_start.elapsed().as_secs_f64() * 1_000_000.0).round() / 1_000_000.0,
            },
            "functions": json_results
        });
        serde_json::to_string_pretty(&envelope)
            .map_err(|e| io::Error::other(format!("JSON serialization failed: {}", e)))?
    } else if effective_json {
        serde_json::to_string_pretty(&json_results)
            .map_err(|e| io::Error::other(format!("JSON serialization failed: {}", e)))?
    } else {
        all_output
    };

    if let Some(ref output_path) = cli.output {
        let mut file = fs::File::create(output_path).map_err(|e| {
            io::Error::other(format!(
                "Failed to create output file '{}': {}",
                output_path.display(),
                e
            ))
        })?;
        file.write_all(final_output.as_bytes())?;
        if cli.verbose {
            eprintln!("[✓] Output written to: {}", output_path.display());
        }
    } else {
        let mut stdout = io::stdout().lock();
        stdout.write_all(final_output.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_decompiler::{PcodeOp, PcodeOpcode, Varnode};

    fn var(offset: u64, size: u32) -> Varnode {
        Varnode {
            space_id: 3,
            offset,
            size,
            is_constant: false,
            constant_val: 0,
        }
    }

    fn op(
        seq_num: u32,
        address: u64,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
    ) -> PcodeOp {
        PcodeOp {
            seq_num,
            opcode,
            address,
            output,
            inputs,
            asm_mnemonic: None,
        }
    }

    #[test]
    fn cfg_blocks_conditional_branch_has_target_and_fallthrough() {
        let ops = vec![
            op(
                0,
                0x100,
                PcodeOpcode::IntAdd,
                Some(var(0x10, 4)),
                vec![Varnode::constant(1, 4), Varnode::constant(2, 4)],
            ),
            op(
                1,
                0x104,
                PcodeOpcode::CBranch,
                None,
                vec![Varnode::constant(0x110, 8), Varnode::constant(1, 1)],
            ),
            op(
                2,
                0x108,
                PcodeOpcode::IntAdd,
                Some(var(0x20, 4)),
                vec![Varnode::constant(3, 4), Varnode::constant(4, 4)],
            ),
            op(3, 0x10c, PcodeOpcode::Return, None, vec![]),
            op(
                4,
                0x110,
                PcodeOpcode::IntAdd,
                Some(var(0x30, 4)),
                vec![Varnode::constant(5, 4), Varnode::constant(6, 4)],
            ),
        ];

        let blocks = build_cfg_blocks(0x100, ops);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].start_address, 0x100);
        assert_eq!(blocks[1].start_address, 0x108);
        assert_eq!(blocks[2].start_address, 0x110);
        assert_eq!(blocks[0].successors, vec![2, 1]);
        assert!(blocks[1].successors.is_empty());
        assert!(blocks[2].successors.is_empty());
        assert_eq!(blocks[0].ops[0].seq_num, 0);
        assert_eq!(blocks[1].ops[0].seq_num, 0);
    }

    #[test]
    fn cfg_blocks_back_edge_branch_creates_self_loop() {
        let ops = vec![
            op(
                0,
                0x100,
                PcodeOpcode::IntAdd,
                Some(var(0x40, 4)),
                vec![Varnode::constant(1, 4), Varnode::constant(1, 4)],
            ),
            op(
                1,
                0x104,
                PcodeOpcode::Branch,
                None,
                vec![Varnode::constant(0x100, 8)],
            ),
            op(2, 0x108, PcodeOpcode::Return, None, vec![]),
        ];

        let blocks = build_cfg_blocks(0x100, ops);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].successors, vec![0]);
        assert!(blocks[1].successors.is_empty());
    }

    #[test]
    fn cfg_blocks_branch_to_unknown_target_has_no_successor() {
        let ops = vec![
            op(
                0,
                0x100,
                PcodeOpcode::IntAdd,
                Some(var(0x50, 4)),
                vec![Varnode::constant(1, 4), Varnode::constant(2, 4)],
            ),
            op(
                1,
                0x104,
                PcodeOpcode::Branch,
                None,
                vec![Varnode::constant(0x200, 8)],
            ),
            op(
                2,
                0x108,
                PcodeOpcode::IntAdd,
                Some(var(0x60, 4)),
                vec![Varnode::constant(3, 4), Varnode::constant(4, 4)],
            ),
        ];

        let blocks = build_cfg_blocks(0x100, ops);
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].successors.is_empty());
    }

    #[test]
    fn cfg_blocks_conditional_branch_deduplicates_same_target_and_fallthrough() {
        let ops = vec![
            op(
                0,
                0x100,
                PcodeOpcode::IntAdd,
                Some(var(0x70, 4)),
                vec![Varnode::constant(1, 4), Varnode::constant(2, 4)],
            ),
            op(
                1,
                0x104,
                PcodeOpcode::CBranch,
                None,
                vec![Varnode::constant(0x108, 8), Varnode::constant(1, 1)],
            ),
            op(2, 0x108, PcodeOpcode::Return, None, vec![]),
        ];

        let blocks = build_cfg_blocks(0x100, ops);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].successors, vec![1]);
    }

    #[test]
    fn terminal_control_flow_only_stops_on_return_or_indirect_branch() {
        assert!(!is_terminal_control_flow(PcodeOpcode::Branch));
        assert!(!is_terminal_control_flow(PcodeOpcode::CBranch));
        assert!(is_terminal_control_flow(PcodeOpcode::BranchInd));
        assert!(is_terminal_control_flow(PcodeOpcode::Return));
    }
}
