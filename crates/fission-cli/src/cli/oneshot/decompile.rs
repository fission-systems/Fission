use crate::cli::args::OneShotArgs;
use crate::cli::oneshot::common::{
    apply_profile, init_decompiler, load_binary_into_decompiler, resolve_compiler_id,
    resolve_profile,
};
use crate::cli::output::OutputSilencer;
use fission_analysis::analysis::decomp::postprocess::PostProcessor;
use fission_core::PATHS;
use fission_ffi::DecompilerNative;
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use tracing::warn;

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

fn register_memory_sections(decomp: &mut DecompilerNative, binary: &LoadedBinary, verbose: bool) {
    if verbose {
        eprintln!(
            "[*] Registering {} memory sections...",
            binary.sections.len()
        );
    }

    let _silencer = OutputSilencer::new_if(!verbose);
    for section in &binary.sections {
        if let Err(e) = decomp.add_memory_block(
            &section.name,
            section.virtual_address,
            section.virtual_size,
            section.file_offset,
            section.file_size,
            section.is_executable,
            section.is_writable,
        ) && verbose
        {
            eprintln!("[!] Failed to register section {}: {}", section.name, e);
        }
    }
}

fn register_known_functions(decomp: &mut DecompilerNative, binary: &LoadedBinary, verbose: bool) {
    if verbose {
        eprintln!(
            "[*] Registering {} known functions...",
            binary.functions.len()
        );
    }

    let _silencer = OutputSilencer::new_if(!verbose);
    let mut by_addr: BTreeMap<u64, &FunctionInfo> = BTreeMap::new();
    for func in &binary.functions {
        if func.address == 0 || func.name.is_empty() {
            continue;
        }
        match by_addr.get(&func.address) {
            None => {
                by_addr.insert(func.address, func);
            }
            Some(current) => {
                if prefer_function_name(&func.name, &current.name) {
                    by_addr.insert(func.address, func);
                }
            }
        }
    }

    for func in by_addr.values() {
        if func.address != 0
            && !func.name.is_empty()
            && let Err(e) = decomp.add_function(func.address, Some(&func.name))
            && verbose
        {
            eprintln!(
                "[!] Failed to register function at 0x{:x}: {}",
                func.address, e
            );
        }
    }
}

fn load_fid_databases(decomp: &mut DecompilerNative, binary: &LoadedBinary, verbose: bool) {
    let mut fid_loaded_count = 0;
    let fid_paths = PATHS.get_all_fid_paths(binary.is_64bit);
    for fid_full in &fid_paths {
        if verbose {
            eprintln!("[*] Loading FID database: {}", fid_full.display());
        }
        let _silencer = OutputSilencer::new_if(!verbose);
        if let Err(e) = decomp.load_fid_database(&fid_full.to_string_lossy()) {
            if verbose {
                eprintln!("[!] Warning: Failed to load FID database: {}", e);
            }
        } else {
            fid_loaded_count += 1;
            if verbose {
                eprintln!("[✓] FID database loaded");
            }
        }
    }

    if verbose && fid_loaded_count > 0 {
        eprintln!(
            "[✓] Loaded {} FID database(s) for function matching",
            fid_loaded_count
        );
    }
}

fn collect_target_functions(
    binary: &LoadedBinary,
    address: Option<u64>,
    all: bool,
) -> Vec<&FunctionInfo> {
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

    if all {
        let mut by_addr: BTreeMap<u64, &FunctionInfo> = BTreeMap::new();
        for func in binary.functions.iter().filter(|f| !f.is_import) {
            match by_addr.get(&func.address) {
                None => {
                    by_addr.insert(func.address, func);
                }
                Some(current) => {
                    if prefer_function_name(&func.name, &current.name) {
                        by_addr.insert(func.address, func);
                    }
                }
            }
        }
        return by_addr.into_values().collect();
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
                compiler_id.unwrap_or("default")
            );
        }
        load_binary_into_decompiler(&mut decomp, binary, binary_data, compiler_id, cli.verbose);
    }

    // Add IAT symbols
    decomp.add_symbols(&binary.iat_symbols);
    decomp.add_global_symbols(&binary.global_symbols);
    decomp.set_symbol_provider(&binary.functions, &binary.global_symbols, &binary.sections);

    register_memory_sections(&mut decomp, binary, cli.verbose);
    register_known_functions(&mut decomp, binary, cli.verbose);
    load_fid_databases(&mut decomp, binary, cli.verbose);

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
    let functions = collect_target_functions(binary, cli.address, cli.all);

    if functions.is_empty() && cli.address.is_some() {
        // Use if-let for safer unwrapping
        if let Some(addr) = cli.address {
            eprintln!("Warning: No function found at address 0x{:x}", addr);
            // Try to decompile anyway
            decompile_and_output(cli, &decomp, addr, &format!("sub_{:x}", addr))?;
        }
        return Ok(());
    }

    // Derive effective flags: --ghidra-compat implies --no-header + --no-warnings
    // --benchmark implies --json
    let effective_no_header = cli.no_header || cli.ghidra_compat;
    let effective_no_warnings = cli.no_warnings || cli.ghidra_compat;
    let effective_json = cli.json || cli.benchmark;

    // Decompile each function
    let mut all_output = String::new();
    let mut json_results: Vec<serde_json::Value> = Vec::new();
    let mut total_decomp_secs: f64 = 0.0;

    for func in &functions {
        if cli.verbose {
            eprintln!("[*] Decompiling {} (0x{:x})...", func.name, func.address);
        }

        let _silencer = OutputSilencer::new_if(!cli.verbose);
        let func_start = std::time::Instant::now();
        match decomp.decompile(func.address) {
            Ok(code) => {
                let decomp_sec = func_start.elapsed().as_secs_f64();
                total_decomp_secs += decomp_sec;
                // Apply Rust-side post-processing (switch reconstruction, while→for, etc.)
                let postprocessor =
                    PostProcessor::new().with_inferred_types(binary.inferred_types.clone());
                let code = postprocessor.process(&code);
                // Apply output filters
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

    // In benchmark mode, wrap results with metadata envelope
    let final_output = if cli.benchmark {
        let envelope = serde_json::json!({
            "_meta": {
                "tool": "fission",
                "version": env!("CARGO_PKG_VERSION"),
                "profile": cli.profile.as_deref().unwrap_or("balanced"),
                "function_count": functions.len(),
                "init_sec": (init_elapsed.as_secs_f64() * 1_000_000.0).round() / 1_000_000.0,
                "total_decomp_sec": (total_decomp_secs * 1_000_000.0).round() / 1_000_000.0,
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

pub(super) fn decompile_and_output(
    cli: &OneShotArgs,
    decomp: &DecompilerNative,
    addr: u64,
    name: &str,
) -> io::Result<()> {
    let effective_no_header = cli.no_header || cli.ghidra_compat;
    let effective_no_warnings = cli.no_warnings || cli.ghidra_compat;

    let _silencer = OutputSilencer::new_if(!cli.verbose);
    match decomp.decompile(addr) {
        Ok(code) => {
            // Apply Rust-side post-processing
            let postprocessor = PostProcessor::new();
            let code = postprocessor.process(&code);
            // Apply output filters
            let mut filtered = code.clone();
            if effective_no_warnings {
                filtered = strip_warnings(&filtered);
            }
            if cli.ghidra_compat {
                filtered = strip_inferred_structs(&filtered);
            }

            let mut stdout = io::stdout().lock();
            if cli.json {
                let json_output = serde_json::to_string_pretty(&serde_json::json!({
                    "address": format!("0x{:x}", addr),
                    "name": name,
                    "code": filtered
                }))
                .map_err(|e| io::Error::other(format!("JSON serialization failed: {}", e)))?;
                writeln!(stdout, "{}", json_output)?;
            } else {
                if !effective_no_header {
                    writeln!(stdout, "// Function: {} @ 0x{:x}\n", name, addr)?;
                }
                writeln!(stdout, "{}", filtered)?;
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
    Ok(())
}
