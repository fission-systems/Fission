//! Decompilation command

use crate::cli::output::OutputSilencer;
use crate::ui::cli::handlers::CliState;
use colored::Colorize;
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
        use fission_ffi::DecompilerNative;

        println!("{} Initializing native decompiler...", "[*]".blue());

        // Get SLA directory
        let sla_dir = match std::env::current_dir() {
            Ok(dir) => dir
                .join("ghidra_decompiler")
                .join("languages")
                .to_string_lossy()
                .into_owned(),
            Err(e) => {
                println!("{} Failed to get current directory: {}", "[!]".red(), e);
                return;
            }
        };

        let suppress_native_logs = std::env::var("FISSION_SUPPRESS_NATIVE_LOGS")
            .ok()
            .as_deref()
            == Some("1");

        let mut native = {
            let _silencer = OutputSilencer::new_if(suppress_native_logs);
            match DecompilerNative::new(&sla_dir) {
                Ok(d) => d,
                Err(e) => {
                    println!("{} Failed to start decompiler: {}", "[!]".red(), e);
                    return;
                }
            }
        };

        {
            let _silencer = OutputSilencer::new_if(suppress_native_logs);
            // Load binary into decompiler
            // Try to detect compiler
            let detection = fission_loader::detect(&binary);
            let compiler_id = detection
                .compiler()
                .map(|d| match d.name.to_lowercase().as_str() {
                    "microsoft visual c++" | "msvc" => "windows",
                    "gcc" | "mingw" => "gcc",
                    "clang" => "clang",
                    _ => "default",
                });

            if let Err(e) = native.load_binary(
                &binary.data,
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
        }

        // Register all sections for proper VA-to-file-offset mapping
        println!(
            "{} Registering {} sections...",
            "[*]".blue(),
            binary.sections.len()
        );
        {
            let _silencer = OutputSilencer::new_if(suppress_native_logs);
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
        }

        // Add symbols
        {
            let _silencer = OutputSilencer::new_if(suppress_native_logs);
            native.add_symbols(&binary.iat_symbols);
            native.add_global_symbols(&binary.global_symbols);
            native.set_symbol_provider(&binary.functions, &binary.global_symbols, &binary.sections);
        }
        println!("{} Decompiling...", "[*]".blue());

        // Decompile
        {
            let _silencer = OutputSilencer::new_if(suppress_native_logs);
            match native.decompile(addr) {
                Ok(c_code) => {
                    println!("{}", c_code);
                }
                Err(e) => {
                    println!("{} Decompilation failed: {}", "[!]".red(), e);
                }
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
