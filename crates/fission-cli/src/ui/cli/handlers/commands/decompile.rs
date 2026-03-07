//! Decompilation command

use crate::cli::output::OutputSilencer;
use crate::ui::cli::handlers::CliState;
use colored::Colorize;
use fission_core::{DEFAULT_DECOMP_MEMORY_LIMIT, find_sla_dir};
use fission_loader::loader::LoadedBinary;

pub fn cmd_decompile(state: &CliState, addr: Option<u64>) {
    let binary: &LoadedBinary = match &state.binary {
        Some(b) => b.as_ref(),
        None => {
            println!(
                "{} No binary loaded. Use 'load <path>' first.",
                "[!]".yellow()
            );
            return;
        }
    };

    let addr = match addr {
        Some(a) => a,
        None => {
            println!(
                "{} Please specify an address: decompile <address>",
                "[!]".yellow()
            );
            return;
        }
    };

    // Find function at address using optimized lookup
    // First try exact match with O(1) HashMap, then fall back to range check
    let func = binary.function_at(addr);

    let func_name = func.map(|f| f.name.as_str()).unwrap_or("unknown");

    println!();
    println!(
        "{} {} @ 0x{:X}",
        "Decompile".bold().underline(),
        func_name,
        addr
    );
    println!();

    // Try to use the FFI decompiler
    #[cfg(feature = "native_decomp")]
    {
        use fission_analysis::analysis::decomp::RecommendedDecompiler;

        println!("{} Initializing native decompiler...", "[*]".blue());

        // Get SLA directory
        let sla_dir = find_sla_dir();

        let suppress_native_logs = std::env::var("FISSION_SUPPRESS_NATIVE_LOGS")
            .ok()
            .as_deref()
            == Some("1");

        let mut decompiler = {
            let _silencer = OutputSilencer::new_if(suppress_native_logs);
            // Default cache size per DEFAULT_DECOMP_MEMORY_LIMIT
            match RecommendedDecompiler::new(binary, &sla_dir, DEFAULT_DECOMP_MEMORY_LIMIT) {
                Ok(d) => d,
                Err(e) => {
                    println!("{} Failed to start decompiler: {}", "[!]".red(), e);
                    return;
                }
            }
        };

        // Inner setup block to access underlying native interface for loading
        {
            let _silencer = OutputSilencer::new_if(suppress_native_logs);
            let native = decompiler.inner_mut();

            // Try to detect compiler
            let detection = fission_loader::detect(binary);
            let is_pe = binary.format.to_ascii_uppercase().starts_with("PE");
            let compiler_id = detection
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
                });

            if let Err(e) = native.load_binary(
                binary.data.as_slice(),
                binary.image_base,
                binary.is_64bit,
                Some(&binary.arch_spec),
                compiler_id,
            ) {
                println!(
                    "{} Failed to load binary into decompiler: {}",
                    "[!]".red(),
                    e
                );
                return;
            }

            // Register sections
            println!(
                "{} Registering {} sections...",
                "[*]".blue(),
                binary.sections.len()
            );
            for section in &binary.sections {
                if let Err(e) = native.add_memory_block(
                    &section.name,
                    section.virtual_address,
                    section.virtual_size,
                    section.file_offset,
                    section.file_size,
                    section.is_executable,
                    section.is_writable,
                ) {
                    println!(
                        "{} Warning: Failed to add section {}: {}",
                        "[!]".yellow(),
                        section.name,
                        e
                    );
                }
            }

            // Add symbols
            // Mix IAT symbols and function names (which key include demangled names)
            let mut all_symbols = binary.iat_symbols.clone();
            for func in &binary.functions {
                if !func.name.is_empty() {
                    all_symbols.insert(func.address, func.name.clone());
                }
            }

            native.add_symbols(&all_symbols);
            native.add_global_symbols(&binary.global_symbols);
            native.set_symbol_provider(&binary.functions, &binary.global_symbols, &binary.sections);

            // Register inferred types from metadata (Swift, Go, etc.)
            if !binary.inferred_types.is_empty() {
                println!(
                    "{} Registering {} inferred types...",
                    "[*]".blue(),
                    binary.inferred_types.len()
                );
                if let Err(e) = native.register_inferred_types(&binary.inferred_types) {
                    println!(
                        "{} Warning: Failed to register types: {}",
                        "[!]".yellow(),
                        e
                    );
                }
            }
        }

        println!("{} Decompiling...", "[*]".blue());

        // Decompile via caching wrapper (which handles post-processing)
        let result = {
            let _silencer = OutputSilencer::new_if(suppress_native_logs);
            decompiler.decompile(addr)
        };

        match result {
            Ok(c_code) => {
                println!("{}", c_code);
            }
            Err(e) => {
                println!("{} Decompilation failed: {}", "[!]".red(), e);
            }
        }
    }

    #[cfg(not(feature = "native_decomp"))]
    {
        println!(
            "{} Native decompiler not available. Build with --features native_decomp",
            "[!]".yellow()
        );
    }
    println!();
}
