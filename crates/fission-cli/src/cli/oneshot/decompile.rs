use crate::cli::args::OneShotArgs;
use crate::cli::oneshot::common::{
    EngineMode, apply_profile, fallback_reason_with_kind, init_decompiler, resolve_compiler_id,
    resolve_engine_mode, resolve_profile,
};
use crate::cli::oneshot::disasm::render_function_disassembly_text;
use crate::cli::output::OutputSilencer;
use fission_core::FissionError;
use fission_ffi::DecompilerNative;
use fission_loader::loader::types::{InferredFieldInfo, InferredTypeInfo};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::postprocess::PostProcessor;
use fission_static::analysis::decomp::{
    PrepareOptions, PrepareTimings, PreviewEngineMode, prepare_native_decompiler_for_binary,
    rescue_preview_output, select_preview_output, serialize_win_api_signatures_json,
};
use std::env;
use std::fs;
use std::io::{self, Write};
use tracing::warn;

#[cfg(feature = "native_decomp")]
use rayon::prelude::*;

fn prefer_function_name(candidate: &str, current: &str) -> bool {
    let candidate_is_sub = candidate.starts_with("sub_");
    let current_is_sub = current.starts_with("sub_");
    if candidate_is_sub != current_is_sub {
        return !candidate_is_sub;
    }
    candidate.len() > current.len()
}

/// Strip WARNING / NOTICE diagnostic lines from decompiler output.
/// Removes lines starting with `WARNING:`, `NOTICE:`, or `/* WARNING` comments.
fn strip_warnings(code: &str) -> String {
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

/// Strip inferred struct definitions (typedef struct ... } name;) blocks
/// from the top of decompiler output for cleaner Ghidra-compatible comparison.
fn strip_inferred_structs(code: &str) -> String {
    let mut result = String::new();
    let mut in_struct_block = false;
    for line in code.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("typedef struct") || trimmed.starts_with("// Inferred Structure") {
            in_struct_block = true;
            continue;
        }
        if in_struct_block {
            // End of struct block: closing `} name;`
            if trimmed.starts_with('}') && trimmed.ends_with(';') {
                in_struct_block = false;
                continue;
            }
            // Still inside struct definition
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

fn classify_native_failure_kind(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("preview_timeout") {
        "preview_timeout"
    } else if lower.contains("could not find op at target address")
        || lower.contains("ghidra lowlevelerror")
    {
        "native_pcode_failure"
    } else {
        "legacy_fallback"
    }
}

fn make_assembly_fallback(
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

fn attach_native_timing(entry: &mut serde_json::Value, decomp: &DecompilerNative) {
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

/// Result of decompiling one function (used for both sequential and parallel paths)
struct DecompEntry {
    address: u64,
    name: String,
    code: Result<RenderedCode, fission_core::FissionError>,
    decomp_sec: f64,
    postprocess_sec: f64,
    last_timing_json: Option<String>,
}

struct RenderedCode {
    code: String,
    postprocess_sec: f64,
    engine_used: &'static str,
    fell_back: bool,
    fallback_reason: Option<String>,
}

fn inferred_type_identity(ty: &InferredTypeInfo) -> (&str, u64, &str) {
    (&ty.name, ty.metadata_address, &ty.mangled_name)
}

fn merge_type_fields(existing: &mut Vec<InferredFieldInfo>, incoming: Vec<InferredFieldInfo>) {
    for field in incoming {
        if let Some(current) = existing
            .iter_mut()
            .find(|current| current.offset == field.offset)
        {
            if current.name.is_empty() && !field.name.is_empty() {
                current.name = field.name.clone();
            }
            if current.type_name.is_empty() && !field.type_name.is_empty() {
                current.type_name = field.type_name.clone();
            }
            if current.size == 0 && field.size != 0 {
                current.size = field.size;
            }
            continue;
        }
        existing.push(field);
    }
}

fn merge_inferred_types(
    function_types: Vec<InferredTypeInfo>,
    loader_types: &[InferredTypeInfo],
) -> Vec<InferredTypeInfo> {
    let mut merged: Vec<InferredTypeInfo> = Vec::new();
    for ty in function_types
        .into_iter()
        .chain(loader_types.iter().cloned())
    {
        if let Some(existing) = merged.iter_mut().find(|current| {
            let (name, metadata_address, mangled_name) = inferred_type_identity(current);
            let (incoming_name, incoming_metadata, incoming_mangled) = inferred_type_identity(&ty);
            metadata_address != 0 && incoming_metadata != 0 && metadata_address == incoming_metadata
                || (!mangled_name.is_empty()
                    && !incoming_mangled.is_empty()
                    && mangled_name == incoming_mangled)
                || (!name.is_empty() && !incoming_name.is_empty() && name == incoming_name)
        }) {
            if existing.kind.is_empty() && !ty.kind.is_empty() {
                existing.kind = ty.kind.clone();
            }
            if existing.mangled_name.is_empty() && !ty.mangled_name.is_empty() {
                existing.mangled_name = ty.mangled_name.clone();
            }
            if existing.metadata_address == 0 && ty.metadata_address != 0 {
                existing.metadata_address = ty.metadata_address;
            }
            if existing.size == 0 && ty.size != 0 {
                existing.size = ty.size;
            }
            merge_type_fields(&mut existing.fields, ty.fields);
            continue;
        }
        merged.push(ty);
    }
    merged
}

fn type_diag_enabled() -> bool {
    env::var_os("FISSION_TYPE_DIAG").is_some()
}

fn log_type_diag(
    address: u64,
    function_types: &[InferredTypeInfo],
    loader_types: &[InferredTypeInfo],
    merged_types: &[InferredTypeInfo],
) {
    if !type_diag_enabled() {
        return;
    }

    let count_fields =
        |items: &[InferredTypeInfo]| -> usize { items.iter().map(|ty| ty.fields.len()).sum() };
    let sample_names = |items: &[InferredTypeInfo]| -> String {
        items
            .iter()
            .take(5)
            .map(|ty| ty.name.as_str())
            .filter(|name| !name.is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    };

    eprintln!(
        "[TYPE-DIAG] addr=0x{:x} function_types={} function_fields={} loader_types={} loader_fields={} merged_types={} merged_fields={} samples=[{}]",
        address,
        function_types.len(),
        count_fields(function_types),
        loader_types.len(),
        count_fields(loader_types),
        merged_types.len(),
        count_fields(merged_types),
        sample_names(merged_types),
    );
}

fn render_legacy_code(
    address: u64,
    binary: &LoadedBinary,
    result: fission_ffi::DecompilationResult,
) -> (String, f64) {
    let function_types = result.inferred_types;
    let merged_types = merge_inferred_types(function_types.clone(), &binary.inferred_types);
    log_type_diag(
        address,
        &function_types,
        &binary.inferred_types,
        &merged_types,
    );
    let postprocessor = PostProcessor::new()
        .with_inferred_types(merged_types)
        .with_string_map(Some(binary.inner().string_map.clone()));
    let postprocess_start = std::time::Instant::now();
    let code = postprocessor.process(&result.code);
    let postprocess_sec = postprocess_start.elapsed().as_secs_f64();
    (code, postprocess_sec)
}

fn legacy_rendered_code(
    address: u64,
    binary: &LoadedBinary,
    result: fission_ffi::DecompilationResult,
) -> RenderedCode {
    let (code, postprocess_sec) = render_legacy_code(address, binary, result);
    RenderedCode {
        code,
        postprocess_sec,
        engine_used: PreviewEngineMode::Legacy.as_str(),
        fell_back: false,
        fallback_reason: None,
    }
}

fn decompile_code_with_profile(
    _profile: &str,
    engine_mode: EngineMode,
    decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    timeout_ms: Option<u64>,
    _verbose: bool,
) -> Result<RenderedCode, FissionError> {
    let preview_mode = match engine_mode {
        EngineMode::Legacy => PreviewEngineMode::Legacy,
        EngineMode::MlilPreview => PreviewEngineMode::MlilPreview,
        EngineMode::Auto => PreviewEngineMode::Auto,
    };
    let preview = select_preview_output(decomp, binary, address, name, preview_mode, timeout_ms)
        .map_err(FissionError::decompiler)?;

    if let Some(code) = preview.preview_code {
        return Ok(RenderedCode {
            code,
            postprocess_sec: 0.0,
            engine_used: PreviewEngineMode::MlilPreview.as_str(),
            fell_back: false,
            fallback_reason: None,
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
                if let Some(selection) =
                    rescue_preview_output(decomp, binary, address, name, &error_text, timeout_ms)
                        .map_err(FissionError::decompiler)?
                {
                    if let Some(code) = selection.preview_code {
                        return Ok(RenderedCode {
                            code,
                            postprocess_sec: 0.0,
                            engine_used: PreviewEngineMode::MlilPreview.as_str(),
                            fell_back: true,
                            fallback_reason: selection.fallback_reason,
                        });
                    }
                }
            }
            return Err(e);
        }
    };
    let mut rendered = legacy_rendered_code(address, binary, result);
    rendered.fell_back = preview.fell_back;
    rendered.fallback_reason = preview.fallback_reason;
    Ok(rendered)
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
                        "code": filtered,
                        "engine_used": rendered.engine_used,
                        "fell_back": rendered.fell_back,
                        "fallback_reason": rendered.fallback_reason,
                    });
                    if cli.benchmark {
                        entry["decomp_sec"] =
                            serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                        entry["postprocess_sec"] = serde_json::json!(
                            (postprocess_sec * 1_000_000.0).round() / 1_000_000.0
                        );
                        attach_native_timing(&mut entry, decomp);
                    }
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
                            "code": fallback,
                            "engine_used": PreviewEngineMode::Legacy.as_str(),
                            "fell_back": true,
                            "fallback": "assembly",
                            "fallback_reason": fallback_reason_with_kind("assembly_fallback", &error_text),
                            "fallback_class": fallback_class
                        });
                        if cli.benchmark {
                            entry["decomp_sec"] =
                                serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                            attach_native_timing(&mut entry, decomp);
                        }
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
                    }
                    continue;
                }
                if effective_json {
                    let mut entry = serde_json::json!({
                        "address": format!("0x{:x}", func.address),
                        "name": func.name,
                        "engine_used": PreviewEngineMode::Legacy.as_str(),
                        "fell_back": true,
                        "fallback_reason": fallback_reason_with_kind(classify_native_failure_kind(&error_text), &error_text),
                        "error": error_text
                    });
                    if cli.benchmark {
                        entry["decomp_sec"] =
                            serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                        attach_native_timing(&mut entry, decomp);
                    }
                    json_results.push(entry);
                } else {
                    all_output.push_str(&format!(
                        "// Error decompiling {} (0x{:x}): {}\n\n",
                        func.name, func.address, error_text
                    ));
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
) -> (String, Vec<serde_json::Value>, f64, f64) {
    let (compiler_id, _) = resolve_compiler_id(binary, cli.compiler_id.as_deref());
    let config = fission_core::config::Config::default();
    let gdt_path_owned = fission_core::PATHS
        .get_gdt_path(binary.is_64bit)
        .and_then(|p| p.to_str().map(String::from));
    // Dynamic worker scaling: avoid negative scaling when function count is low.
    // Each worker incurs ~3–4s init (FID/GDT/.sla). With 20 functions, 8 workers → 62s vs 1 → 26s.
    // Heuristic: aim for ≥50 functions per worker so init cost is amortized (Amdahl's Law).
    let num_workers = 8;

    // Round-robin distribution: spread heavy functions (often at low addresses) across workers
    // instead of clustering them in the first chunk (address-ordered chunks).
    let mut buckets: Vec<Vec<&'a FunctionInfo>> = (0..num_workers).map(|_| Vec::new()).collect();
    for (i, func) in functions.iter().enumerate() {
        buckets[i % num_workers].push(*func);
    }

    // Bucket 0: use the already-prepared main_decomp on the main thread
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
                                engine_used: PreviewEngineMode::Legacy.as_str(),
                                fell_back: true,
                                fallback_reason: Some(fallback_reason_with_kind(
                                    "assembly_fallback",
                                    &error_text,
                                )),
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

    // Pre-serialize Win API signatures once (avoid per-worker JSON serialization).
    let signatures_json = serialize_win_api_signatures_json();

    // Each worker creates its own decompiler (init per bucket). num_workers is capped above
    // so that small batches (e.g. limit 20) use 1 worker → 26s; large batches use all cores.
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
                                    engine_used: PreviewEngineMode::Legacy.as_str(),
                                    fell_back: true,
                                    fallback_reason: Some(fallback_reason_with_kind(
                                        "assembly_fallback",
                                        &error_text,
                                    )),
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
                        "code": filtered,
                        "engine_used": rendered.engine_used,
                        "fell_back": rendered.fell_back,
                        "fallback_reason": rendered.fallback_reason,
                    });
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
                }
            }
            Err(e) => {
                if effective_json {
                    let mut json_entry = serde_json::json!({
                        "address": format!("0x{:x}", entry.address),
                        "name": entry.name,
                        "engine_used": PreviewEngineMode::Legacy.as_str(),
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
                    json_results.push(json_entry);
                } else {
                    all_output.push_str(&format!(
                        "// Error decompiling {} (0x{:x}): {}\n\n",
                        entry.name, entry.address, e
                    ));
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

fn collect_target_functions<'a>(
    binary: &'a LoadedBinary,
    address: Option<u64>,
    decomp_all: bool,
    decomp_limit: Option<usize>,
) -> Vec<&'a FunctionInfo> {
    if decomp_all {
        let collected: Vec<_> = binary.functions.iter().collect();
        if let Some(n) = decomp_limit {
            return collected.into_iter().take(n).collect();
        }
        return collected;
    }

    if let Some(addr) = address {
        let mut best: Option<&FunctionInfo> = None;
        for func in &binary.functions {
            if func.address != addr {
                continue;
            }
            match best {
                None => best = Some(func),
                Some(current) => {
                    if prefer_function_name(&func.name, &current.name) {
                        best = Some(func);
                    }
                }
            }
        }
        return best.into_iter().collect();
    }

    vec![]
}

pub(super) fn run_decompilation(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    binary_data: &[u8],
) -> io::Result<()> {
    let init_start = std::time::Instant::now();
    let mut decomp = init_decompiler(cli.verbose);

    // Apply one-shot profile before binary load/decompilation.
    let (selected_profile, unknown_profile) = resolve_profile(cli.profile.as_deref());
    let (engine_mode, unknown_engine, deprecated_preview_alias) =
        resolve_engine_mode(cli.engine.as_deref(), cli.profile.as_deref());
    if let Some(other) = unknown_profile {
        eprintln!(
            "[!] Unknown --profile '{}', using balanced (quality|speed|balanced|mlil-preview)",
            other
        );
        warn!(
            profile = other,
            "unknown decompilation profile, using balanced"
        );
    }
    if let Some(other) = unknown_engine {
        eprintln!(
            "[!] Unknown --engine '{}', using auto (mlil-preview|auto)",
            other
        );
        warn!(engine = other, "unknown decompilation engine, using auto");
    }
    if matches!(engine_mode, EngineMode::Legacy) && cli.verbose {
        eprintln!(
            "[*] '--engine legacy' is a hidden compatibility mode; preview-first remains the product default"
        );
    }
    if deprecated_preview_alias && cli.verbose {
        eprintln!(
            "[*] '--profile mlil-preview' is deprecated; use '--engine mlil-preview --profile quality'"
        );
    }
    apply_profile(&mut decomp, selected_profile);

    if cli.verbose {
        eprintln!("[*] Decompilation profile = {}", selected_profile);
        eprintln!("[*] Decompilation engine = {:?}", engine_mode);
    }

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
        let signatures_json = serialize_win_api_signatures_json();
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

    // Collect functions to decompile and deduplicate by address.
    // Some loaders may expose multiple aliases for a single address
    // (e.g., sub_xxx + exported symbol), which can trigger duplicate
    // decompile attempts and noisy recursive-guard errors.
    let functions = collect_target_functions(binary, cli.address, cli.decomp_all, cli.decomp_limit);

    if functions.is_empty() && cli.address.is_some() {
        // Use if-let for safer unwrapping
        if let Some(addr) = cli.address {
            eprintln!("Warning: No function found at address 0x{:x}", addr);
            // Try to decompile anyway
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

    // Derive effective flags: --ghidra-compat implies --no-header + --no-warnings
    // --benchmark implies --json
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
        )
    };

    // In benchmark mode, wrap results with metadata envelope
    let final_output = if cli.benchmark {
        let envelope = serde_json::json!({
            "_meta": {
                "tool": "fission",
                "version": env!("CARGO_PKG_VERSION"),
                "profile": cli.profile.as_deref().unwrap_or("balanced"),
                "engine": cli.engine.as_deref().unwrap_or("auto"),
                "function_count": functions.len(),
                "init_sec": (init_elapsed.as_secs_f64() * 1_000_000.0).round() / 1_000_000.0,
                "prepare_timings": &prepare_timings,
                "total_decomp_sec": (total_decomp_secs * 1_000_000.0).round() / 1_000_000.0,
                "total_postprocess_sec": (total_postprocess_secs * 1_000_000.0).round() / 1_000_000.0,
                "wall_clock_sec": (init_start.elapsed().as_secs_f64() * 1_000_000.0).round() / 1_000_000.0,
            },
            "functions": json_results
        });
        serde_json::to_string_pretty(&envelope).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("JSON serialization failed: {}", e),
            )
        })?
    } else if effective_json {
        serde_json::to_string_pretty(&json_results).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("JSON serialization failed: {}", e),
            )
        })?
    } else {
        all_output
    };

    if let Some(ref output_path) = cli.output {
        let mut file = fs::File::create(output_path).map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!(
                    "Failed to create output file '{}': {}",
                    output_path.display(),
                    e
                ),
            )
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

pub(super) fn decompile_and_output(
    cli: &OneShotArgs,
    decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    binary_data: &[u8],
    selected_profile: &str,
    engine_mode: EngineMode,
    addr: u64,
    name: &str,
) -> io::Result<()> {
    let effective_no_header = cli.no_header || cli.ghidra_compat;
    let effective_no_warnings = cli.no_warnings || cli.ghidra_compat;

    let _silencer = OutputSilencer::new_if(!cli.verbose);
    match decompile_code_with_profile(
        selected_profile,
        engine_mode,
        decomp,
        binary,
        addr,
        name,
        cli.timeout_ms,
        cli.verbose,
    ) {
        Ok(rendered) => {
            // Apply output filters
            let mut filtered = rendered.code.clone();
            if effective_no_warnings {
                filtered = strip_warnings(&filtered);
            }
            if cli.ghidra_compat {
                filtered = strip_inferred_structs(&filtered);
            }
            // Prepare final output string (respect --output when provided)
            if cli.json {
                let json_output = serde_json::to_string_pretty(&serde_json::json!({
                    "address": format!("0x{:x}", addr),
                    "name": name,
                    "code": filtered,
                    "engine_used": rendered.engine_used,
                    "fell_back": rendered.fell_back,
                    "fallback_reason": rendered.fallback_reason,
                }))
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("JSON serialization failed: {}", e),
                    )
                })?;
                if let Some(ref output_path) = cli.output {
                    fs::write(output_path, json_output.as_bytes())?;
                    if cli.verbose {
                        eprintln!("[✓] Output written to: {}", output_path.display());
                    }
                } else {
                    let mut stdout = io::stdout().lock();
                    writeln!(stdout, "{}", json_output)?;
                }
            } else {
                let mut out_buf = String::new();
                if !effective_no_header {
                    out_buf.push_str(&format!("// Function: {} @ 0x{:x}\n\n", name, addr));
                }
                out_buf.push_str(&filtered);
                out_buf.push_str("\n");

                if let Some(ref output_path) = cli.output {
                    fs::write(output_path, out_buf.as_bytes())?;
                    if cli.verbose {
                        eprintln!("[✓] Output written to: {}", output_path.display());
                    }
                } else {
                    let mut stdout = io::stdout().lock();
                    writeln!(stdout, "{}", out_buf)?;
                }
            }
        }
        Err(e) => {
            let error_text = e.to_string();
            if let Some(func) = binary.function_at_exact(addr)
                && let Some(fallback) =
                    make_assembly_fallback(binary, binary_data, func, &error_text)
            {
                let mut stdout = io::stdout().lock();
                writeln!(stdout, "{}", fallback)?;
                return Ok(());
            }
            eprintln!("Error: {}", error_text);
            std::process::exit(1);
        }
    }
    Ok(())
}
