//! One-Shot CLI - Single command execution mode
//!
//! Executes a single command and exits (non-interactive).

mod binary_info;
#[cfg(feature = "native_decomp")]
mod common;
#[cfg(feature = "native_decomp")]
mod decompile;
#[cfg(not(feature = "native_decomp"))]
mod decompile_rust_sleigh;
mod disasm;
mod function_select;
mod functions;
mod inventory;
mod strings;

use binary_info::{print_binary_info, print_exports, print_imports, print_sections};
#[cfg(feature = "native_decomp")]
use decompile::{
    emit_preview_candidate_inventory, emit_preview_candidate_scan_batch, run_decompilation,
};
#[cfg(not(feature = "native_decomp"))]
use decompile_rust_sleigh::run_decompilation_rust_sleigh;
use disasm::{disassemble, disassemble_function};
use functions::print_function_list;
use inventory::emit_function_facts_inventory;
use strings::print_strings;

use crate::cli::args::{FunctionDiscoveryProfileArg, OneShotArgs};
use anyhow::{Context, Result};
use clap::Parser;
use fission_loader::loader::{FunctionDiscoveryProfile, LoadedBinary};
use std::fs;
use std::io;

fn map_discovery_profile_arg(profile: FunctionDiscoveryProfileArg) -> FunctionDiscoveryProfile {
    match profile {
        FunctionDiscoveryProfileArg::Conservative => FunctionDiscoveryProfile::Conservative,
        FunctionDiscoveryProfileArg::Balanced => FunctionDiscoveryProfile::Balanced,
        FunctionDiscoveryProfileArg::Aggressive => FunctionDiscoveryProfile::Aggressive,
    }
}

/// Entry point for one-shot CLI mode
pub fn run_oneshot() -> Result<()> {
    run()
}

/// Main entry point (for bin/fission_cli.rs binary)
pub fn main() -> Result<()> {
    run_oneshot()
}

fn run() -> Result<()> {
    let cli = OneShotArgs::parse();
    let mut logging_options =
        fission_core::logging::LoggingOptions::from_config(&fission_core::CONFIG.logging);
    logging_options.level = if cli.verbose { "info" } else { "warn" }.to_string();
    logging_options.include_span_events = cli.verbose;
    fission_core::logging::init_with_options(logging_options);
    if cli.verbose {
        tracing::info!(binary = %cli.binary.display(), "initialized one-shot CLI");
    }

    // Capture BrokenPipe errors gracefully
    if let Err(error) = execute_command(&cli) {
        if error
            .downcast_ref::<io::Error>()
            .is_some_and(|err| err.kind() == io::ErrorKind::BrokenPipe)
        {
            return Ok(());
        }
        let span_trace = fission_core::logging::capture_span_trace();
        return Err(error.context(format!("span trace:\n{span_trace}")));
    }
    Ok(())
}

fn execute_command(cli: &OneShotArgs) -> Result<()> {
    if cli.verbose {
        eprintln!("[*] Loading binary: {}", cli.binary.display());
    }

    anyhow::ensure!(
        cli.binary.exists(),
        "binary path does not exist: {}",
        cli.binary.display()
    );

    let binary_data = fs::read(&cli.binary)
        .with_context(|| format!("failed to read binary `{}`", cli.binary.display()))?;

    let mut binary = LoadedBinary::from_bytes(
        binary_data.clone(),
        cli.binary.to_string_lossy().to_string(),
    )
    .with_context(|| format!("failed to parse binary `{}`", cli.binary.display()))?;

    if let Some(profile_arg) = cli.function_discovery_profile {
        let profile = map_discovery_profile_arg(profile_arg);
        let before = binary.functions.len();
        binary.discover_internal_functions_with_profile(profile);
        binary.discover_functions_by_prologue_with_profile(profile);
        let discovered = binary.functions.len().saturating_sub(before);
        if cli.verbose {
            eprintln!(
                "[*] Function discovery profile {:?}: +{} functions",
                profile, discovered
            );
        }
    }

    if cli.verbose {
        eprintln!(
            "[ok] Loaded: {} ({}-bit, {} functions)",
            cli.binary.display(),
            if binary.is_64bit { 64 } else { 32 },
            binary.functions.len()
        );
    }

    if cli.info {
        return Ok(print_binary_info(&binary, cli.json)?);
    }

    if cli.sections {
        return Ok(print_sections(&binary, cli.json)?);
    }

    if cli.imports {
        return Ok(print_imports(&binary, cli.json)?);
    }

    if cli.exports {
        return Ok(print_exports(&binary, cli.json)?);
    }

    if cli.list {
        return Ok(print_function_list(&binary, cli.json)?);
    }

    if cli.preview_candidate_inventory {
        #[cfg(feature = "native_decomp")]
        {
            return emit_preview_candidate_inventory(cli, &binary, &binary_data);
        }

        #[cfg(not(feature = "native_decomp"))]
        {
            anyhow::bail!("preview candidate inventory is deprecated with native_decomp removal");
        }
    }

    if cli.preview_candidate_scan_batch {
        #[cfg(feature = "native_decomp")]
        {
            return emit_preview_candidate_scan_batch(cli, &binary, &binary_data);
        }

        #[cfg(not(feature = "native_decomp"))]
        {
            anyhow::bail!("preview candidate scan batch is deprecated with native_decomp removal");
        }
    }

    if cli.emit_function_facts_inventory {
        return Ok(emit_function_facts_inventory(cli, &binary, &binary_data)?);
    }

    if let Some(min_len) = cli.strings {
        return Ok(print_strings(&binary_data, min_len.max(4), cli.json)?);
    }

    if let Some(addr) = cli.disasm {
        return Ok(disassemble(&binary, &binary_data, addr, cli.count, cli.json)?);
    }

    if let Some(addr) = cli.disasm_function {
        return Ok(disassemble_function(&binary, &binary_data, addr, cli.json)?);
    }

    if cli.address.is_some() || cli.decomp_all {
        #[cfg(feature = "native_decomp")]
        {
            run_decompilation(cli, &binary, &binary_data)?;
            return Ok(());
        }

        #[cfg(not(feature = "native_decomp"))]
        {
            run_decompilation_rust_sleigh(cli, &binary, &binary_data)?;
            return Ok(());
        }
    }

    print_help();
    Ok(())
}

fn print_help() {
    println!("Fission CLI - one-shot binary analysis and decompilation");
    println!();
    println!("Usage: fission_cli <binary> [OPTIONS]");
    println!();
    println!("Information:");
    println!("  -i, --info                 Show binary info (format, arch, entry point)");
    println!("  -S, --sections             Show all sections with permissions");
    println!("  -l, --list, --funcs        List all discovered functions");
    println!("  -I, --imports              List imported functions");
    println!("  -E, --exports              List exported functions");
    println!();
    println!("Analysis:");
    println!("  -d, --disasm, --asm <ADDR> Disassemble at address");
    println!("      --asm-func <ADDR>      Disassemble full function at address");
    println!("  -n, --count <N>            Number of instructions (default: 20)");
    println!("      --strings [MIN]        Extract strings (min length: 4)");
    println!();
    println!("Decompilation:");
    println!("  -a, --address, --decomp <ADDR>  Decompile function");
    println!();
    println!("Output:");
    println!("  -o, --output <FILE>        Write results to file");
    println!("  -j, --json                 JSON output format");
    println!("  -v, --verbose              Show detailed progress");
    println!("      --compiler-id <ID>     Override compiler ABI hint");
    println!("      --profile <P>          Decomp profile: balanced|quality|speed");
    println!("      --engine <E>           Decomp engine: legacy|nir|auto");
    println!("      --no-header            Suppress function header comments");
    println!("      --ghidra-compat        Suppress headers/warnings + strip inferred structs");
    println!("      --no-warnings          Suppress WARNING/NOTICE lines");
    println!("      --benchmark            Add timing metadata to JSON output");
    println!("      --decomp-limit <N>     Limit --decomp-all to first N functions");
    println!(
        "      --function-discovery-profile <P>   conservative|balanced|aggressive"
    );
    println!();
    println!("Examples:");
    println!("  fission_cli app.exe --info");
    println!("  fission_cli app.exe --funcs");
    println!("  fission_cli app.exe --asm 0x140001000");
    println!("  fission_cli app.exe --decomp 0x140001000");
}
