//! One-Shot CLI - Single command execution mode
//!
//! Executes a single command and exits (non-interactive).

mod assessment;
mod binary_info;
mod callgraph;
#[cfg(feature = "native_decomp")]
mod common;
#[cfg(feature = "debugger")]
mod debug;
mod debug_bundle_extra;
mod debug_decomp;
#[cfg(feature = "native_decomp")]
mod decompile;
mod disasm;
mod function_select;
mod functions;
mod inventory;
mod nir_stats;
mod pcode_diagnostics;
mod pcode_stages;
mod pcode_topology;
mod raw_pcode;
#[cfg(not(feature = "native_decomp"))]
mod rust_decomp;
mod script;
mod strings;
mod xrefs;

use binary_info::{print_binary_info, print_exports, print_imports, print_sections};
use callgraph::run_callgraph;
#[cfg(feature = "native_decomp")]
use decompile::{
    emit_preview_candidate_inventory, emit_preview_candidate_scan_batch, run_decompilation,
};
use disasm::{disassemble, disassemble_function};
use functions::print_function_list;
use inventory::emit_function_facts_inventory;
use nir_stats::emit_nir_stats;
use pcode_stages::emit_pcode_stages;
use pcode_topology::emit_pcode_topology;
use raw_pcode::emit_raw_pcode;
#[cfg(not(feature = "native_decomp"))]
use rust_decomp::run_decompilation_rust_sleigh;
use strings::print_strings;
use xrefs::run_xrefs;

use crate::cli::args::{
    FunctionDiscoveryProfileArg, LegacyInvocationKind, OneShotArgs, ParsedInvocation,
    ParsedOneShotArgs, parse_oneshot_args,
};
use anyhow::{Context, Result};
use fission_loader::loader::LoadedBinary;
use fission_static::analysis::{FunctionDiscoveryProfile, discover_functions_with_runtime};
use script::execute_script;
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
    let parsed = parse_oneshot_args();
    match parsed {
        ParsedInvocation::ResourcesStatus { json, verbose } => {
            let mut logging_options =
                fission_core::logging::LoggingOptions::from_config(&fission_core::CONFIG.logging);
            logging_options.level = if verbose { "info" } else { "warn" }.to_string();
            logging_options.include_span_events = verbose;
            fission_core::logging::init_with_options(logging_options);

            if let Err(error) = crate::cli::resources::print_resources_status(json) {
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|err| err.kind() == io::ErrorKind::BrokenPipe)
                {
                    return Ok(());
                }
                let span_trace = fission_core::logging::capture_span_trace();
                return Err(error.context(format!("span trace:\n{span_trace}")));
            }
            Ok(())
        }
        ParsedInvocation::Script(invocation) => {
            let mut logging_options =
                fission_core::logging::LoggingOptions::from_config(&fission_core::CONFIG.logging);
            logging_options.level = if invocation.verbose { "info" } else { "warn" }.to_string();
            logging_options.include_span_events = invocation.verbose;
            fission_core::logging::init_with_options(logging_options);

            if let Err(error) = execute_script(invocation) {
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|err| err.kind() == io::ErrorKind::BrokenPipe)
                {
                    return Ok(());
                }
                let span_trace = fission_core::logging::capture_span_trace();
                return Err(error.context(format!("span trace:\n{span_trace}")));
            }
            return Ok(());
        }
        ParsedInvocation::Debug(args) => {
            #[cfg(feature = "debugger")]
            {
                return debug::run_debug_command(args);
            }
            #[cfg(not(feature = "debugger"))]
            {
                let _ = args;
                anyhow::bail!(
                    "Debugger support is not compiled into this build. \
                     Rebuild with --features debugger."
                );
            }
        }
        ParsedInvocation::Ai(inv) => {
            // AI subcommands run on a tokio multi-thread runtime.
            let rt = tokio::runtime::Runtime::new()
                .context("failed to create tokio runtime for AI subcommand")?;
            rt.block_on(crate::cli::ai::run_ai(inv))
        }
        ParsedInvocation::Sandbox(args) => {
            run_sandbox(args)
        }
        ParsedInvocation::OneShot(parsed) => run_oneshot_inner(parsed),
    }
}

fn run_oneshot_inner(parsed: ParsedOneShotArgs) -> Result<()> {
    let cli = parsed.args;
    let mut logging_options =
        fission_core::logging::LoggingOptions::from_config(&fission_core::CONFIG.logging);
    logging_options.level = if cli.verbose { "info" } else { "warn" }.to_string();
    logging_options.include_span_events = cli.verbose;
    fission_core::logging::init_with_options(logging_options);
    if let Some(kind) = parsed.legacy_warning {
        emit_legacy_deprecation_warning(kind, &cli);
    }
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

fn run_sandbox(args: crate::cli::args::SandboxArgs) -> Result<()> {
    let mut logging_options =
        fission_core::logging::LoggingOptions::from_config(&fission_core::CONFIG.logging);
    logging_options.level = "debug".to_string(); // Force debug for sandbox
    logging_options.include_span_events = true;
    fission_core::logging::init_with_options(logging_options);

    tracing::info!("Starting sandbox for {}", args.binary.display());
    
    // Parse binary using fission-loader
    let binary = fission_loader::loader::LoadedBinary::from_file(&args.binary)
        .with_context(|| format!("failed to read binary at {}", args.binary.display()))?;
        
    let mut state = fission_emulator::MachineState::new();
    fission_emulator::os::windows::loader::load_pe(&mut state, &binary)?;
    fission_emulator::os::windows::peb_teb::initialize_peb_teb(&mut state, binary.inner().is_64bit)?;
    
    // Initialize Sleigh Frontend
    let load_spec = binary.load_spec().with_context(|| "Binary lacks load_spec")?;
    let frontends = fission_sleigh::runtime::RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(load_spec)
        .with_context(|| "Failed to create Sleigh frontend candidates")?;
    let sleigh = frontends.into_iter().next().with_context(|| "No suitable Sleigh frontend found")?;
    
    // Derive ArchInfo from Sleigh Language ID + binary format
    let lang_id = load_spec.pair.language_id.as_str();
    let arch = fission_emulator::ArchInfo::from_language_id(lang_id, Some(&binary))
        .with_context(|| format!("Unsupported architecture: {}", lang_id))?;
    
    // Choose OS environment based on binary format
    let os: Box<dyn fission_emulator::OsEnvironment> = match binary.format.as_str() {
        "PE"  => Box::new(fission_emulator::WindowsEnv::new()),
        "ELF" => Box::new(fission_emulator::LinuxEnv),
        fmt   => anyhow::bail!("Unsupported binary format for sandbox: {}", fmt),
    };
    
    // Create Emulator and Run
    let mut emu = fission_emulator::core::Emulator::new(state, binary, sleigh, arch, os)?
        .with_max_inst(args.max_inst)
        .with_stdin_mock(args.stdin_mock);

    // Setup Ctrl+C handler
    ctrlc::set_handler(move || {
        fission_emulator::core::IS_INTERRUPTED.store(true, std::sync::atomic::Ordering::Relaxed);
    }).unwrap_or_else(|e| tracing::warn!("Failed to set Ctrl+C handler: {}", e));
    
    if let Some(trigger) = args.snapshot_at {
        tracing::info!("Configured snapshot trigger at 0x{:X}", trigger);
        emu.snapshot_triggers.push(trigger);
    }
    
    if let Some(snapshot_path) = args.restore_snapshot {
        tracing::info!("Restoring snapshot from {}", snapshot_path.display());
        let snapshot = fission_emulator::EmulatorSnapshot::load_from_disk(&snapshot_path)?;
        snapshot.restore_into(&mut emu);
    }
    if args.dump_trace.is_some() {
        emu.trace.enabled = true;
    }

    tracing::info!("Starting Emulator Execution Loop at PC=0x{:X}", emu.pc);
    emu.run()?;
    
    if let Some(trace_path) = args.dump_trace {
        tracing::info!("Dumping execution trace to {}", trace_path.display());
        let f = std::fs::File::create(&trace_path)
            .with_context(|| format!("Failed to create trace file {}", trace_path.display()))?;
        let mut writer = std::io::BufWriter::new(f);
        // Writing each entry as a separate JSON object per line (JSONL) is better for large traces,
        // but for simplicity, we serialize the array if it's small enough, or serialize entry by entry.
        for entry in &emu.trace.entries {
            serde_json::to_writer(&mut writer, entry)?;
            use std::io::Write;
            writer.write_all(b"\n")?;
        }
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
        let report = discover_functions_with_runtime(&mut binary, profile);
        if cli.verbose {
            eprintln!(
                "[*] SLEIGH function discovery profile {:?}: +{} functions (decoded={}, calls={}, jumps={}, unsupported_runtime={})",
                profile,
                report.accepted_function_count,
                report.decoded_instruction_count,
                report.call_target_count,
                report.jump_target_count,
                report.unsupported_runtime
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
        return Ok(print_binary_info(
            &binary,
            cli.json,
            cli.info_detections,
            cli.info_identity,
            cli.info_xrefs,
        )?);
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

    if cli.xrefs_cmd {
        return Ok(run_xrefs(cli, &binary)?);
    }

    if cli.callgraph_cmd {
        return Ok(run_callgraph(cli, &binary)?);
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
        return Ok(disassemble(
            &binary,
            &binary_data,
            addr,
            cli.count,
            cli.json,
        )?);
    }

    if let Some(addr) = cli.disasm_function {
        return Ok(disassemble_function(&binary, &binary_data, addr, cli.json)?);
    }

    if let Some(addr) = cli.raw_pcode {
        return Ok(emit_raw_pcode(
            &binary,
            addr,
            cli.raw_pcode_max_bytes,
            cli.raw_pcode_instruction_limit,
            cli.raw_pcode_continue_past_indirect,
            cli.json,
        )?);
    }

    if let Some(addr) = cli.pcode_stages {
        return Ok(emit_pcode_stages(
            &binary,
            addr,
            cli.pcode_stages_max_bytes,
            cli.pcode_stages_instruction_limit,
            cli.pcode_stages_strict_indirect_stop,
            cli.json,
        )?);
    }

    if let Some(addr) = cli.nir_stats {
        return Ok(emit_nir_stats(
            &binary,
            addr,
            cli.nir_stats_max_bytes,
            cli.nir_stats_instruction_limit,
            cli.nir_stats_strict_indirect_stop,
            cli.json,
        )?);
    }

    if let Some(addr) = cli.pcode_topology {
        return Ok(emit_pcode_topology(
            &binary,
            addr,
            cli.pcode_topology_max_bytes,
            cli.pcode_topology_instruction_limit,
            cli.pcode_topology_strict_indirect_stop,
            cli.json,
        )?);
    }

    if cli.address.is_some() || cli.decomp_all {
        if cli.debug_decomp && !cli.json && !cli.benchmark {
            anyhow::bail!(
                "`--debug-decomp` requires `--json` or `--benchmark` when embedding `debug_decomp` in stdout output"
            );
        }

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

fn emit_legacy_deprecation_warning(kind: LegacyInvocationKind, cli: &OneShotArgs) {
    let binary = cli.binary.display();
    let replacement = match kind {
        LegacyInvocationKind::Info => {
            if cli.sections {
                format!("fission_cli info {binary} --sections")
            } else if cli.imports {
                format!("fission_cli info {binary} --imports")
            } else if cli.exports {
                format!("fission_cli info {binary} --exports")
            } else {
                format!("fission_cli info {binary}")
            }
        }
        LegacyInvocationKind::List => format!("fission_cli list {binary}"),
        LegacyInvocationKind::Disasm => {
            if let Some(addr) = cli.disasm_function {
                format!("fission_cli disasm {binary} --addr 0x{addr:x} --function")
            } else if let Some(addr) = cli.disasm {
                format!("fission_cli disasm {binary} --addr 0x{addr:x}")
            } else {
                format!("fission_cli disasm {binary} --addr <ADDR>")
            }
        }
        LegacyInvocationKind::Decomp => {
            if cli.decomp_all {
                if let Some(limit) = cli.decomp_limit {
                    format!("fission_cli decomp {binary} --all --limit {limit}")
                } else {
                    format!("fission_cli decomp {binary} --all")
                }
            } else if let Some(addr) = cli.address {
                format!("fission_cli decomp {binary} --addr 0x{addr:x}")
            } else {
                format!("fission_cli decomp {binary} --addr <ADDR>")
            }
        }
    };

    eprintln!(
        "warning: legacy flat CLI syntax is deprecated; use canonical subcommand syntax `{replacement}` instead"
    );
}

fn print_help() {
    println!("Fission CLI - headless-first binary analysis and decompilation");
    println!();
    println!("Usage:");
    println!("  fission_cli info <binary> [--sections|--imports|--exports] [--json]");
    println!("  fission_cli list <binary> [--json]");
    println!("  fission_cli disasm <binary> --addr <ADDR> [--count N] [--function] [--json]");
    println!("  fission_cli raw-pcode <binary> --addr <ADDR> [--json]");
    println!("  fission_cli pcode-stages <binary> --addr <ADDR> [--json]");
    println!("  fission_cli nir-stats <binary> --addr <ADDR> [--json]");
    println!("  fission_cli pcode-topology <binary> --addr <ADDR> [--json]");
    println!("  fission_cli decomp <binary> (--addr <ADDR> | --all) [OPTIONS]");
    println!("  fission_cli strings <binary> [--min-len N] [--json]");
    println!("  fission_cli xrefs <binary> [--json] [--no-disassembly] [--function ADDR]");
    println!("  fission_cli callgraph <binary> [--json]");
    println!("  fission_cli inventory <SUBCOMMAND> <binary> [OPTIONS]");
    println!("  fission_cli script check --script <FILE>");
    println!("  fission_cli script run <binary> --script <FILE> [--json]");
    println!("  fission_cli debug init <path> [args...] [--json]");
    println!("  fission_cli debug attach <PID> [--json]");
    println!("  fission_cli debug detach");
    println!("  fission_cli debug continue | pause | stop");
    println!("  fission_cli debug step | step-over | step-out | skip");
    println!("  fission_cli debug bp <addr> [--json]");
    println!("  fission_cli debug rmbp <addr> [--json]");
    println!("  fission_cli debug bpenable <addr> | bpdisable <addr> | bplist [--json]");
    println!("  fission_cli debug hwbp <addr> [--kind execute|write|read-write] [--json]");
    println!(
        "  fission_cli debug membp <addr> --size <N> [--kind read|write|execute|access] [--json]"
    );
    println!("  fission_cli debug rmmembp <addr> [--json]");
    println!("  fission_cli debug dllbp <name> | rmdllbp <name>");
    println!("  fission_cli debug exbp <code> | rmexbp <code>");
    println!("  fission_cli debug regs | setreg <name> <hex-value> [--json]");
    println!("  fission_cli debug getflag <name> | setflag <name> <true|false> [--json]");
    println!("  fission_cli debug read <addr> --size <N> [--json]");
    println!("  fission_cli debug write <addr> <hex-data> [--json]");
    println!("  fission_cli debug alloc <size> [--addr <hex>] [--json]");
    println!("  fission_cli debug free <addr> [--json]");
    println!("  fission_cli debug getprotect <addr> [--json]");
    println!("  fission_cli debug setprotect <addr> <size> <protect-u32> [--json]");
    println!(
        "  fission_cli debug stack-peek [--offset <N>] | stack-pop | stack-push <value> [--json]"
    );
    println!("  fission_cli debug find <start> <size> <hex-pattern> [--json]");
    println!("  fission_cli debug exports <base-addr> | imports <base-addr> [--json]");
    println!("  fission_cli debug modules | threads | switch-thread <tid>");
    println!("  fission_cli debug event");
    println!();
    println!("Commands:");
    println!("  info       Show binary metadata and inventory views");
    println!("  list       List discovered functions");
    println!("  disasm     Disassemble instructions or full functions");
    println!("  raw-pcode  Emit Rust-Sleigh raw p-code");
    println!("  pcode-stages  Emit Rust-Sleigh/NIR stage diagnostics");
    println!("  nir-stats  Emit canonical NirBuildStats telemetry");
    println!("  pcode-topology  Emit raw p-code block topology");
    println!("  decomp     Decompile one function or all discovered functions");
    println!("  strings    Extract strings");
    println!("  xrefs      Canonical xref index (loader + optional disassembly)");
    println!("  callgraph  Call graph from xref analysis");
    println!("  inventory  Operator-oriented inventory and batch emitters");
    println!("  script     Rhai automation against read-only binary inventory");
    println!("  debug      Live process debugger (requires --features debugger)");
    println!();
    println!("Decomp options:");
    println!("      --profile <P>          balanced|quality|speed|nir");
    println!("      --engine <E>           auto|nir|rust-sleigh");
    println!("      --compiler-id <ID>     Override compiler ABI hint");
    println!("      --timeout-ms <MS>      Per-function timeout");
    println!("      --output <FILE>        Write results to file");
    println!("      --json                 JSON output format");
    println!("      --verbose              Show detailed progress");
    println!("      --no-header            Suppress function header comments");
    println!("      --ghidra-compat        Suppress headers/warnings + strip inferred structs");
    println!("      --no-warnings          Suppress WARNING/NOTICE lines");
    println!("      --benchmark            Add timing metadata to JSON output");
    println!();
    println!("Examples:");
    println!("  fission_cli info app.exe");
    println!("  fission_cli list app.exe");
    println!("  fission_cli disasm app.exe --addr 0x140001000");
    println!("  fission_cli raw-pcode app.exe --addr 0x140001000");
    println!("  fission_cli pcode-stages app.exe --addr 0x140001000 --json");
    println!("  fission_cli nir-stats app.exe --addr 0x140001000 --json");
    println!("  fission_cli pcode-topology app.exe --addr 0x140001000");
    println!("  fission_cli decomp app.exe --addr 0x140001000");
    println!("  fission_cli decomp app.exe --all --limit 10");
    println!(
        "  fission_cli inventory function-facts app.exe --output-jsonl rows.jsonl --summary-json summary.json"
    );
    println!();
    println!(
        "Legacy flat invocations still work during the transition, but now emit deprecation warnings and normalize into the canonical subcommand path."
    );
}
