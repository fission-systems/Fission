//! One-Shot CLI - Single command execution mode
//!
//! A powerful, user-friendly binary analysis tool with decompilation capabilities.
//! Executes a single command and exits (non-interactive).

mod binary_info;
#[cfg(feature = "native_decomp")]
mod cfg;
mod disasm;
#[cfg(feature = "native_decomp")]
mod decompile;
#[cfg(feature = "native_decomp")]
pub mod graph;
mod functions;
mod strings;

use binary_info::{print_binary_info, print_exports, print_imports, print_sections};
#[cfg(feature = "native_decomp")]
use cfg::{analyze_cfg, CfgOutputFormat};
#[cfg(feature = "native_decomp")]
use decompile::run_decompilation;
use disasm::{disassemble, disassemble_function};
use functions::print_function_list;
use strings::print_strings;

use crate::analysis::loader::LoadedBinary;
use crate::cli::args::OneShotArgs;
use clap::Parser;
use std::fs;
use std::io;

/// Entry point for one-shot CLI mode
pub fn run_oneshot() -> io::Result<()> {
    run()
}

/// Main entry point (for bin/fission_cli.rs binary)
pub fn main() -> io::Result<()> {
    run_oneshot()
}

fn run() -> io::Result<()> {
    let cli = OneShotArgs::parse();

    // Capture BrokenPipe errors gracefully
    if let Err(e) = execute_command(&cli)
        && e.kind() != io::ErrorKind::BrokenPipe
    {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
    Ok(())
}

fn execute_command(cli: &OneShotArgs) -> io::Result<()> {
    // Load binary
    if cli.verbose {
        eprintln!("[*] Loading binary: {}", cli.binary.display());
    }

    let binary_data = match fs::read(&cli.binary) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Error: Failed to read binary: {}", e);
            std::process::exit(1);
        }
    };

    let binary = match LoadedBinary::from_bytes(
        binary_data.clone(),
        cli.binary.to_string_lossy().to_string(),
    ) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error: Failed to parse binary: {}", e);
            std::process::exit(1);
        }
    };

    if cli.verbose {
        eprintln!(
            "[✓] Loaded: {} ({}-bit, {} functions)",
            cli.binary.display(),
            if binary.is_64bit { 64 } else { 32 },
            binary.functions.len()
        );
    }

    // Handle commands (in priority order)
    if cli.info {
        return print_binary_info(&binary, cli.json);
    }

    if cli.sections {
        return print_sections(&binary, cli.json);
    }

    if cli.imports {
        return print_imports(&binary, cli.json);
    }

    if cli.exports {
        return print_exports(&binary, cli.json);
    }

    if cli.list {
        return print_function_list(&binary, cli.json);
    }

    if let Some(min_len) = cli.strings {
        return print_strings(&binary_data, min_len.max(4), cli.json);
    }

    if let Some(addr) = cli.disasm {
        return disassemble(&binary, &binary_data, addr, cli.count, cli.json);
    }

    if let Some(addr) = cli.disasm_function {
        return disassemble_function(&binary, &binary_data, addr, cli.json);
    }

    // Handle Pcode Graph Generation
    if let Some(addr) = cli.graph {
        #[cfg(feature = "native_decomp")]
        {
            return graph::generate_pcode_graph(&binary, addr, cli.output.as_ref(), cli.verbose);
        }

        #[cfg(not(feature = "native_decomp"))]
        {
            eprintln!("Error: Graph generation requires native_decomp feature");
            eprintln!("Run with: cargo run --bin fission_cli --features native_decomp -- ...");
            std::process::exit(1);
        }
    }

    // Handle CFG Analysis
    if let Some(addr) = cli.cfg_address {
        #[cfg(feature = "native_decomp")]
        {
            let format = match cli.cfg_format.as_str() {
                "dot" => CfgOutputFormat::Dot,
                "ascii" => CfgOutputFormat::Ascii,
                "json" => CfgOutputFormat::Json,
                _ => {
                    if cli.json {
                        CfgOutputFormat::Json
                    } else {
                        CfgOutputFormat::Summary
                    }
                }
            };
            return analyze_cfg(&binary, addr, format, cli.output.as_ref(), cli.verbose);
        }

        #[cfg(not(feature = "native_decomp"))]
        {
            eprintln!("Error: CFG analysis requires native_decomp feature");
            eprintln!("Run with: cargo run --bin fission_cli --features native_decomp -- ...");
            std::process::exit(1);
        }
    }

    // Handle decompilation
    if cli.address.is_some() || cli.all {
        #[cfg(feature = "native_decomp")]
        {
            run_decompilation(cli, &binary, &binary_data)?;
            return Ok(());
        }

        #[cfg(not(feature = "native_decomp"))]
        {
            eprintln!("Error: Decompilation requires native_decomp feature");
            eprintln!("Run with: cargo run --bin fission_cli --features native_decomp -- ...");
            std::process::exit(1);
        }
    }

    // Default: show help
    print_help();
    Ok(())
}

fn print_help() {
    println!("\x1b[1;36m╔══════════════════════════════════════════════════════════╗\x1b[0m");
    println!("\x1b[1;36m║\x1b[0m  \x1b[1;35m🔬 Fission\x1b[0m - Next-Gen Binary Analysis          \x1b[1;36m║\x1b[0m");
    println!("\x1b[1;36m╚══════════════════════════════════════════════════════════╝\x1b[0m");
    println!();
    println!("\x1b[1;33mUsage:\x1b[0m fission <binary> [OPTIONS]");
    println!();
    println!("\x1b[1;32m📊 Information:\x1b[0m");
    println!("  \x1b[1m-i\x1b[0m, --info          Show binary info (format, arch, entry point)");
    println!("  \x1b[1m-S\x1b[0m, --sections      Show all sections with permissions");
    println!("  \x1b[1m-l\x1b[0m, --list          List all discovered functions");
    println!("  \x1b[1m-I\x1b[0m, --imports       List imported functions");
    println!("  \x1b[1m-E\x1b[0m, --exports       List exported functions");
    println!();
    println!("\x1b[1;34m🔍 Analysis:\x1b[0m");
    println!("  \x1b[1m-d\x1b[0m, --asm <ADDR>    Disassemble at address (alias: --disasm)");
    println!("  \x1b[1m-n\x1b[0m, --count <N>     Number of instructions (default: 20)");
    println!("      --strings [MIN]  Extract strings (min length: 4)");
    println!("      --cfg <ADDR>     Analyze CFG (Control Flow Graph)");
    println!("      --cfg-format <F> CFG format: summary, dot, ascii, json");
    println!();
    println!("\x1b[1;35m⚙️  Decompilation:\x1b[0m");
    println!("  \x1b[1m-a\x1b[0m, --decomp <ADDR> Decompile function (alias: --address)");
    println!("  \x1b[1m-A\x1b[0m, --decomp-all    Decompile all functions (alias: --all)");
    println!();
    println!("\x1b[1;36m💾 Output:\x1b[0m");
    println!("  \x1b[1m-o\x1b[0m, --output <FILE> Write results to file");
    println!("  \x1b[1m-j\x1b[0m, --json          JSON output format");
    println!("  \x1b[1m-v\x1b[0m, --verbose       Show detailed progress");
    println!();
    println!("\x1b[1;33m📚 Examples:\x1b[0m");
    println!("  fission app.exe -i                    \x1b[90m# Show binary info\x1b[0m");
    println!("  fission app.exe -l                    \x1b[90m# List functions\x1b[0m");
    println!("  fission app.exe --asm 0x140001000     \x1b[90m# Disassemble\x1b[0m");
    println!("  fission app.exe --decomp 0x140001000  \x1b[90m# Decompile\x1b[0m");
    println!("  fission app.exe --decomp-all -o out/  \x1b[90m# Decompile all\x1b[0m");
    println!("  fission app.exe --cfg 0x140001000     \x1b[90m# CFG analysis\x1b[0m");
    println!("  fission app.exe --cfg 0x140001000 --cfg-format dot -o out.dot \x1b[90m# CFG graph\x1b[0m");
    println!();
}
