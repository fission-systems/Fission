use crate::cli::args::OneShotArgs;
use crate::cli::oneshot::common::{
    apply_profile, init_decompiler, resolve_compiler_id, resolve_profile,
};
use crate::cli::output::OutputSilencer;
use fission_analysis::analysis::decomp::postprocess::PostProcessor;
use fission_analysis::analysis::decomp::{
    prepare_native_decompiler_for_binary, PrepareOptions, PrepareTimings,
};
use fission_ffi::DecompilerNative;
use fission_loader::loader::{FunctionInfo, LoadedBinary};
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
    code: Result<String, fission_core::FissionError>,
    decomp_sec: f64,
    postprocess_sec: f64,
    last_timing_json: Option<String>,
}

fn run_sequential_decompilation<'a>(
    cli: &OneShotArgs,
    decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    functions: &[&'a FunctionInfo],
    effective_no_header: bool,
    effective_no_warnings: bool,
    effective_json: bool,
) -> (
    String,
    Vec<serde_json::Value>,
    f64,
    f64,
) {
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
        match decomp.decompile_with_metadata(func.address) {
            Ok(result) => {
                let decomp_sec = func_start.elapsed().as_secs_f64();
                total_decomp_secs += decomp_sec;
                let merged_types: Vec<_> = result
                    .inferred_types
                    .into_iter()
                    .chain(binary.inferred_types.iter().cloned())
                    .collect();
                let postprocessor = PostProcessor::new()
                    .with_inferred_types(merged_types)
                    .with_string_map(Some(binary.inner().string_map.clone()));
                let postprocess_start = std::time::Instant::now();
                let code = postprocessor.process(&result.code);
                let postprocess_sec = postprocess_start.elapsed().as_secs_f64();
                total_postprocess_secs += postprocess_sec;
                let mut filtered = code.clone();
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
                        "code": filtered
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
                if effective_json {
                    let mut entry = serde_json::json!({
                        "address": format!("0x{:x}", func.address),
                        "name": func.name,
                        "error": e.to_string()
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
                        func.name, func.address, e
                    ));
                }
            }
        }
    }

    (all_output, json_results, total_decomp_secs, total_postprocess_secs)
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
    _init_elapsed_sec: f64,
    _init_start: std::time::Instant,
    effective_no_header: bool,
    effective_no_warnings: bool,
    effective_json: bool,
) -> (
    String,
    Vec<serde_json::Value>,
    f64,
    f64,
) {
    let (compiler_id, _) = resolve_compiler_id(binary, cli.compiler_id.as_deref());
    let config = fission_core::config::Config::default();
    let gdt_path_owned = fission_core::PATHS
        .get_gdt_path(binary.is_64bit)
        .and_then(|p| p.to_str().map(String::from));
    let inferred_types = binary.inferred_types.clone();
    let string_map = binary.inner().string_map.clone();

    let num_workers = rayon::current_num_threads().max(1);

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
            let code_result = main_decomp.decompile_with_metadata(func.address);
            let decomp_sec = start.elapsed().as_secs_f64();
            let (code_result, postprocess_sec) = match code_result {
                Ok(result) => {
                    let merged: Vec<_> = result
                        .inferred_types
                        .into_iter()
                        .chain(inferred_types.iter().cloned())
                        .collect();
                    let pp = PostProcessor::new()
                        .with_inferred_types(merged)
                        .with_string_map(Some(string_map.clone()));
                    let pp_start = std::time::Instant::now();
                    let processed = pp.process(&result.code);
                    let pp_sec = pp_start.elapsed().as_secs_f64();
                    (Ok(processed), pp_sec)
                }
                Err(e) => (Err(e), 0.0),
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

    // Remaining buckets: each worker creates its own decompiler
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
                timeout_ms: Some(config.decompiler.timeout_ms),
                timings: None,
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
                let code_result = decomp.decompile_with_metadata(func.address);
                let decomp_sec = start.elapsed().as_secs_f64();
                let (code_result, postprocess_sec) = match code_result {
                    Ok(result) => {
                        let merged: Vec<_> = result
                            .inferred_types
                            .into_iter()
                            .chain(inferred_types.iter().cloned())
                            .collect();
                        let pp = PostProcessor::new()
                            .with_inferred_types(merged)
                            .with_string_map(Some(string_map.clone()));
                        let pp_start = std::time::Instant::now();
                        let processed = pp.process(&result.code);
                        let pp_sec = pp_start.elapsed().as_secs_f64();
                        (Ok(processed), pp_sec)
                    }
                    Err(e) => (Err(e), 0.0),
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
            Ok(code) => {
                let mut filtered = code.clone();
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
                        "code": filtered
                    });
                    if cli.benchmark {
                        json_entry["decomp_sec"] =
                            serde_json::json!((entry.decomp_sec * 1_000_000.0).round() / 1_000_000.0);
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

    (all_output, json_results, total_decomp_secs, total_postprocess_secs)
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
    if let Some(other) = unknown_profile {
        eprintln!(
            "[!] Unknown --profile '{}', using balanced (quality|speed|balanced)",
            other
        );
        warn!(
            profile = other,
            "unknown decompilation profile, using balanced"
        );
    }
    apply_profile(&mut decomp, selected_profile);

    if cli.verbose {
        eprintln!("[*] Decompilation profile = {}", selected_profile);
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
        let mut options = PrepareOptions {
            verbose: cli.verbose,
            compiler_id: compiler_id.as_deref(),
            gdt_path: gdt_path_owned.as_deref(),
            timeout_ms: Some(config.decompiler.timeout_ms),
            timings: if cli.benchmark {
                Some(&mut prepare_timings)
            } else {
                None
            },
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
    let functions = collect_target_functions(
        binary,
        cli.address,
        cli.decomp_all,
        cli.decomp_limit,
    );

    if functions.is_empty() && cli.address.is_some() {
        // Use if-let for safer unwrapping
        if let Some(addr) = cli.address {
            eprintln!("Warning: No function found at address 0x{:x}", addr);
            // Try to decompile anyway
            decompile_and_output(cli, &decomp, binary, addr, &format!("sub_{:x}", addr))?;
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
            &functions,
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
    decomp: &DecompilerNative,
    binary: &LoadedBinary,
    addr: u64,
    name: &str,
) -> io::Result<()> {
    let effective_no_header = cli.no_header || cli.ghidra_compat;
    let effective_no_warnings = cli.no_warnings || cli.ghidra_compat;

    let _silencer = OutputSilencer::new_if(!cli.verbose);
    match decomp.decompile_with_metadata(addr) {
        Ok(result) => {
            // Apply Rust-side post-processing with StructureAnalyzer inferred types
            let postprocessor = PostProcessor::new()
                .with_inferred_types(result.inferred_types)
                .with_string_map(Some(binary.inner().string_map.clone()));
            let code = postprocessor.process(&result.code);
            // Apply output filters
            let mut filtered = code.clone();
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
                    "code": filtered
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
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
    Ok(())
}
