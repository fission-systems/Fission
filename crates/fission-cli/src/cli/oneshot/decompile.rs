use crate::cli::args::OneShotArgs;
use crate::cli::output::OutputSilencer;
use fission_core::find_sla_dir;
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

pub(super) fn run_decompilation(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    binary_data: &[u8],
) -> io::Result<()> {
    // Initialize decompiler
    let sla_dir = find_sla_dir();

    if cli.verbose {
        eprintln!("[*] Initializing native decompiler...");
    }

    let mut decomp = {
        let _silencer = OutputSilencer::new_if(!cli.verbose);
        match DecompilerNative::new(&sla_dir) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("Error: Failed to create decompiler: {}", e);
                std::process::exit(1);
            }
        }
    };

    // Apply one-shot profile before binary load/decompilation.
    let selected_profile = cli.profile.as_deref().unwrap_or("balanced");
    match selected_profile.to_ascii_lowercase().as_str() {
        "quality" => {
            decomp.set_feature("infer_pointers", true);
            decomp.set_feature("analyze_loops", true);
            decomp.set_feature("readonly_propagate", true);
        }
        "speed" => {
            decomp.set_feature("infer_pointers", false);
            decomp.set_feature("analyze_loops", false);
            decomp.set_feature("readonly_propagate", false);
        }
        "balanced" => {
            decomp.set_feature("infer_pointers", true);
            decomp.set_feature("analyze_loops", false);
            decomp.set_feature("readonly_propagate", true);
        }
        other => {
            // Show inline so user always sees it, even without RUST_LOG set
            eprintln!("[!] Unknown --profile '{}', using balanced (quality|speed|balanced)", other);
            warn!(profile = other, "unknown decompilation profile, using balanced");
            decomp.set_feature("infer_pointers", true);
            decomp.set_feature("analyze_loops", false);
            decomp.set_feature("readonly_propagate", true);
        }
    }

    if cli.verbose {
        eprintln!("[*] Decompilation profile = {}", selected_profile);
    }

    // Load binary
    {
        let _silencer = OutputSilencer::new_if(!cli.verbose);
        // Allow explicit compiler override for deterministic one-shot runs.
        let compiler_id = if let Some(user_compiler) = cli.compiler_id.as_deref() {
            Some(match user_compiler.to_ascii_lowercase().as_str() {
                "windows" => "windows",
                "gcc" => "gcc",
                "clang" => "clang",
                "default" => "default",
                "auto" => {
                    let detection = fission_loader::detect(binary);
                    let is_pe = binary.format.to_ascii_uppercase().starts_with("PE");
                    detection
                        .compiler()
                        .map(|d| match d.name.to_lowercase().as_str() {
                            "microsoft visual c++" | "msvc" => "windows",
                            "gcc" | "mingw" => {
                                if is_pe {
                                    "windows"
                                } else {
                                    "gcc"
                                }
                            }
                            "clang" => "clang",
                            _ => "default",
                        })
                        .unwrap_or("default")
                }
                _ => {
                    eprintln!("[!] Unknown --compiler-id '{}', falling back to auto detection", user_compiler);
                    warn!(compiler_id = user_compiler, "unknown compiler-id, falling back to auto detection");
                    let detection = fission_loader::detect(binary);
                    let is_pe = binary.format.to_ascii_uppercase().starts_with("PE");
                    detection
                        .compiler()
                        .map(|d| match d.name.to_lowercase().as_str() {
                            "microsoft visual c++" | "msvc" => "windows",
                            "gcc" | "mingw" => {
                                if is_pe {
                                    "windows"
                                } else {
                                    "gcc"
                                }
                            }
                            "clang" => "clang",
                            _ => "default",
                        })
                        .unwrap_or("default")
                }
            })
        } else {
            let detection = fission_loader::detect(binary);
            let is_pe = binary.format.to_ascii_uppercase().starts_with("PE");
            detection
                .compiler()
                .map(|d| match d.name.to_lowercase().as_str() {
                    "microsoft visual c++" | "msvc" => "windows",
                    "gcc" | "mingw" => {
                        if is_pe {
                            "windows"
                        } else {
                            "gcc"
                        }
                    }
                    "clang" => "clang",
                    _ => "default",
                })
        };

        if cli.verbose {
            eprintln!(
                "[*] Decompiler compiler_id = {}",
                compiler_id.unwrap_or("default")
            );
        }

        if let Err(e) = decomp.load_binary(
            binary_data,
            binary.image_base,
            binary.is_64bit,
            Some(&binary.arch_spec),
            compiler_id,
        ) {
            eprintln!("Error: Failed to load binary: {}", e);
            std::process::exit(1);
        }
    }

    // Add IAT symbols
    decomp.add_symbols(&binary.iat_symbols);
    decomp.add_global_symbols(&binary.global_symbols);
    decomp.set_symbol_provider(&binary.functions, &binary.global_symbols, &binary.sections);

    // Add memory blocks (sections) to improve analysis
    if cli.verbose {
        eprintln!(
            "[*] Registering {} memory sections...",
            binary.sections.len()
        );
    }

    {
        let _silencer = OutputSilencer::new_if(!cli.verbose);
        for section in &binary.sections {
            if let Err(e) = decomp.add_memory_block(
                &section.name,
                section.virtual_address,
                section.virtual_size,
                section.file_offset,
                section.file_size,
                section.is_executable,
                section.is_writable,
            ) && cli.verbose
            {
                eprintln!("[!] Failed to register section {}: {}", section.name, e);
            }
        }
    }

    // Add all known functions to improve decompilation quality
    if cli.verbose {
        eprintln!(
            "[*] Registering {} known functions...",
            binary.functions.len()
        );
    }

    {
        let _silencer = OutputSilencer::new_if(!cli.verbose);
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
                && cli.verbose
            {
                eprintln!(
                    "[!] Failed to register function at 0x{:x}: {}",
                    func.address, e
                );
            }
        }
    }

    // Try to load FID databases if available (load all matching ones)
    let target_suffix = if binary.is_64bit {
        "_x64.fidbf"
    } else {
        "_x86.fidbf"
    };

    // Build comprehensive FID database list
    // Primary: utils/signatures/fid/ (unified location)
    let fid_paths = vec![
        // MSVC databases
        format!("utils/signatures/fid/vs2019{}", target_suffix),
        format!("utils/signatures/fid/vs2017{}", target_suffix),
        format!("utils/signatures/fid/vs2015{}", target_suffix),
        format!("utils/signatures/fid/vs2012{}", target_suffix),
        format!("utils/signatures/fid/vsOlder{}", target_suffix),
        // GCC/MinGW databases
        format!("utils/signatures/fid/gcc13{}", target_suffix),
        format!("utils/signatures/fid/gcc12{}", target_suffix),
        format!("utils/signatures/fid/gcc11{}", target_suffix),
        format!("utils/signatures/fid/mingw{}", target_suffix),
    ];

    // Load all available FID databases for better matching coverage
    let mut fid_loaded_count = 0;
    for fid_path in &fid_paths {
        if let Ok(full_path) = std::env::current_dir() {
            let fid_full = full_path.join(fid_path);
            if fid_full.exists() {
                if cli.verbose {
                    eprintln!("[*] Loading FID database: {}", fid_full.display());
                }
                let _silencer = OutputSilencer::new_if(!cli.verbose);
                if let Err(e) = decomp.load_fid_database(&fid_full.to_string_lossy()) {
                    if cli.verbose {
                        eprintln!("[!] Warning: Failed to load FID database: {}", e);
                    }
                } else {
                    fid_loaded_count += 1;
                    if cli.verbose {
                        eprintln!("[✓] FID database loaded");
                    }
                }
            }
        }
    }

    if cli.verbose && fid_loaded_count > 0 {
        eprintln!(
            "[✓] Loaded {} FID database(s) for function matching",
            fid_loaded_count
        );
    }

    if cli.verbose {
        eprintln!("[✓] Decompiler ready");
    }

    // Collect functions to decompile and deduplicate by address.
    // Some loaders may expose multiple aliases for a single address
    // (e.g., sub_xxx + exported symbol), which can trigger duplicate
    // decompile attempts and noisy recursive-guard errors.
    let functions: Vec<&FunctionInfo> = if let Some(addr) = cli.address {
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
        best.into_iter().collect()
    } else if cli.all {
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
        by_addr.into_values().collect()
    } else {
        vec![]
    };

    if functions.is_empty() && cli.address.is_some() {
        let addr = cli.address.expect("address should be Some");
        eprintln!("Warning: No function found at address 0x{:x}", addr);
        // Try to decompile anyway
        decompile_and_output(cli, &decomp, addr, &format!("sub_{:x}", addr))?;
        return Ok(());
    }

    // Derive effective flags: --ghidra-compat implies --no-header + --no-warnings
    let effective_no_header = cli.no_header || cli.ghidra_compat;
    let effective_no_warnings = cli.no_warnings || cli.ghidra_compat;

    // Decompile each function
    let mut all_output = String::new();
    let mut json_results: Vec<serde_json::Value> = Vec::new();

    for func in &functions {
        if cli.verbose {
            eprintln!("[*] Decompiling {} (0x{:x})...", func.name, func.address);
        }

        let _silencer = OutputSilencer::new_if(!cli.verbose);
        match decomp.decompile(func.address) {
            Ok(code) => {
                // Apply output filters
                let mut filtered = code.clone();
                if effective_no_warnings {
                    filtered = strip_warnings(&filtered);
                }
                if cli.ghidra_compat {
                    filtered = strip_inferred_structs(&filtered);
                }

                if cli.json {
                    json_results.push(serde_json::json!({
                        "address": format!("0x{:x}", func.address),
                        "name": func.name,
                        "code": filtered
                    }));
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
                if cli.json {
                    json_results.push(serde_json::json!({
                        "address": format!("0x{:x}", func.address),
                        "name": func.name,
                        "error": e.to_string()
                    }));
                } else {
                    all_output.push_str(&format!(
                        "// Error decompiling {} (0x{:x}): {}\n\n",
                        func.name, func.address, e
                    ));
                }
            }
        }
    }

    // Output results
    let final_output = if cli.json {
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
        let mut file = fs::File::create(output_path).expect("Failed to create output file");
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
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("JSON serialization failed: {}", e),
                    )
                })?;
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
