use crate::cli::args::OneShotArgs;
use crate::cli::output::OutputSilencer;
use fission_ffi::DecompilerNative;
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use std::fs;
use std::io::{self, Write};

pub(super) fn run_decompilation(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    binary_data: &[u8],
) -> io::Result<()> {
    // Initialize decompiler
    let sla_dir = std::env::current_dir()
        .map_err(|e| {
            io::Error::new(
                io::ErrorKind::Other,
                format!("Failed to get current directory: {}", e),
            )
        })?
        .join("ghidra_decompiler")
        .to_string_lossy()
        .into_owned();

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

    // Load binary
    {
        let _silencer = OutputSilencer::new_if(!cli.verbose);
        if let Err(e) = decomp.load_binary(binary_data, binary.image_base, binary.is_64bit) {
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
        for func in &binary.functions {
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
    // Fallback: utils/ghidra/funtionID/ (legacy location)
    let fid_paths = vec![
        // Primary unified location
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
        // Legacy location (backward compatibility)
        format!("utils/ghidra/funtionID/vs2019{}", target_suffix),
        format!("utils/ghidra/funtionID/vs2017{}", target_suffix),
        format!("utils/ghidra/funtionID/vs2015{}", target_suffix),
        format!("utils/ghidra/funtionID/vs2012{}", target_suffix),
        format!("utils/ghidra/funtionID/vsOlder{}", target_suffix),
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

    // Collect functions to decompile
    let functions: Vec<&FunctionInfo> = if let Some(addr) = cli.address {
        binary
            .functions
            .iter()
            .filter(|f| f.address == addr)
            .collect()
    } else if cli.all {
        binary.functions.iter().filter(|f| !f.is_import).collect()
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
                if cli.json {
                    json_results.push(serde_json::json!({
                        "address": format!("0x{:x}", func.address),
                        "name": func.name,
                        "code": code
                    }));
                } else {
                    all_output.push_str("// ============================================\n");
                    all_output.push_str(&format!(
                        "// Function: {} @ 0x{:x}\n",
                        func.name, func.address
                    ));
                    all_output.push_str("// ============================================\n\n");
                    all_output.push_str(&code);
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
    let _silencer = OutputSilencer::new_if(!cli.verbose);
    match decomp.decompile(addr) {
        Ok(code) => {
            let mut stdout = io::stdout().lock();
            if cli.json {
                let json_output = serde_json::to_string_pretty(&serde_json::json!({
                    "address": format!("0x{:x}", addr),
                    "name": name,
                    "code": code
                }))
                .map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::Other,
                        format!("JSON serialization failed: {}", e),
                    )
                })?;
                writeln!(stdout, "{}", json_output)?;
            } else {
                writeln!(stdout, "// Function: {} @ 0x{:x}\n", name, addr)?;
                writeln!(stdout, "{}", code)?;
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
    Ok(())
}
