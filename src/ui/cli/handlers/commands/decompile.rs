//! Decompilation command

use colored::Colorize;
use crate::ui::cli::handlers::CliState;

pub fn cmd_decompile(state: &CliState, addr: Option<u64>) {
    let binary = match &state.binary {
        Some(b) => b,
        None => {
            println!("{} No binary loaded. Use 'load <path>' first.", "[!]".yellow());
            return;
        }
    };

    let addr = match addr {
        Some(a) => a,
        None => {
            println!("{} Please specify an address: decompile <address>", "[!]".yellow());
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
        use crate::analysis::decomp::ffi::DecompilerNative;

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

        match DecompilerNative::new(&sla_dir) {
            Ok(mut native) => {
                // Load binary into decompiler
                if let Err(e) = native.load_binary(&binary.data, binary.image_base, binary.is_64bit)
                {
                    println!(
                        "{} Failed to load binary into decompiler: {}",
                        "[!]".red(),
                        e
                    );
                    return;
                }

                // Register all sections for proper VA-to-file-offset mapping
                println!("{} Registering {} sections...", "[*]".blue(), binary.sections.len());
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
                native.add_symbols(&binary.iat_symbols);
                println!("{} Decompiling...", "[*]".blue());

                // Decompile
                match native.decompile(addr) {
                    Ok(c_code) => {
                        println!("{}", c_code);
                    }
                    Err(e) => {
                        println!("{} Decompilation failed: {}", "[!]".red(), e);
                    }
                }
            }
            Err(e) => {
                println!("{} Failed to start decompiler: {}", "[!]".red(), e);
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
