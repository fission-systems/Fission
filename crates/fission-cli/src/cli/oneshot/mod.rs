//! One-Shot CLI - Single command execution mode
//!
//! Executes a single command and exits (non-interactive).

mod assessment;
mod binary_info;
mod callgraph;
#[cfg(feature = "debugger")]
mod debug;
mod debug_bundle_extra;
mod debug_decomp;
mod disasm;
mod function_select;
mod functions;
mod identify;
mod inventory;
mod nir_stats;
mod pcode_diagnostics;
mod pcode_stages;
mod pcode_topology;
mod raw_pcode;
mod rust_decomp;
mod script;
mod strings;
mod xrefs;

use binary_info::{print_binary_info, print_exports, print_imports, print_sections};
use callgraph::run_callgraph;
use disasm::{disassemble, disassemble_function};
use functions::print_function_list;
use identify::run_identify;
use inventory::{emit_function_facts_inventory, emit_program_metadata};
use nir_stats::emit_nir_stats;
use pcode_stages::emit_pcode_stages;
use pcode_topology::emit_pcode_topology;
use raw_pcode::emit_raw_pcode;
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
        ParsedInvocation::Sandbox(args) => run_sandbox(args),
        ParsedInvocation::Verify(args) => run_verify(args),
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

/// Heavy, opt-in "deep verify" over `fission-verify`: concrete DIR/HIR
/// diffing, emulator-grounded ground truth, and solver-backed symbolic
/// equivalence. Distinct from `scripts/quality/dir_hir_check.py`, which
/// stays the fast, cheap, no-solver/no-emulator structural heuristic run
/// routinely across the whole corpus -- this command spins up a real
/// emulator and/or solver per function and is meant for targeted
/// investigation, not a per-commit gate.
fn run_verify(args: crate::cli::args::VerifyArgs) -> Result<()> {
    use crate::cli::args::VerifyTierArg;

    let mut logging_options =
        fission_core::logging::LoggingOptions::from_config(&fission_core::CONFIG.logging);
    logging_options.level = "warn".to_string();
    fission_core::logging::init_with_options(logging_options);

    let binary = LoadedBinary::from_file(&args.binary)
        .with_context(|| format!("failed to read binary at {}", args.binary.display()))?;
    let facts = fission_static::analysis::decomp::facts::FactStore::from_binary(&binary);
    let func = fission_loader::loader::FunctionInfo {
        name: args.name.clone(),
        address: args.addr,
        ..Default::default()
    };

    let pair = fission_verify::decompile_one(&binary, &facts, &func)
        .with_context(|| format!("failed to decompile function at 0x{:x}", args.addr))?;

    let run_concrete = matches!(args.tier, VerifyTierArg::Concrete | VerifyTierArg::All);
    let run_ground_truth = matches!(args.tier, VerifyTierArg::GroundTruth | VerifyTierArg::All);
    let run_symbolic = matches!(args.tier, VerifyTierArg::Symbolic | VerifyTierArg::All);

    let mut out = serde_json::Map::new();

    if run_concrete {
        let samples = fission_verify::default_samples(pair.hir.params.len());
        let outcome = fission_verify::diff_dir_hir(&pair.dir, &pair.hir, &samples);
        report_tier("concrete", &format!("{outcome:?}"), args.json, &mut out);
    }
    if run_ground_truth {
        let samples = fission_verify::default_samples(pair.hir.params.len());
        match fission_verify::EmulatorHarness::build(&args.binary, Some(args.max_inst)) {
            Ok(mut harness) => {
                let outcome = fission_verify::check_ground_truth(
                    &mut harness,
                    func.address,
                    &pair.dir,
                    &pair.hir,
                    &samples,
                );
                report_tier("ground_truth", &format!("{outcome:?}"), args.json, &mut out);
            }
            Err(err) => {
                report_tier("ground_truth", &format!("EmulatorSetupFailed({err})"), args.json, &mut out);
            }
        }
    }
    if run_symbolic {
        let outcome = fission_verify::check_symbolic_equivalence(&pair.dir, &pair.hir);
        let desc = match outcome {
            fission_verify::symbolic::SymbolicOutcome::Equivalent => "Equivalent (proved, Unsat)".to_string(),
            fission_verify::symbolic::SymbolicOutcome::Diverged(cx) => {
                format!("Diverged (solver counterexample args={:?})", cx.args)
            }
            fission_verify::symbolic::SymbolicOutcome::Unsupported(reason) => {
                format!("Unsupported({reason})")
            }
        };
        report_tier("symbolic", &desc, args.json, &mut out);
    }

    if args.json {
        println!("{}", serde_json::Value::Object(out));
    }
    Ok(())
}

fn report_tier(tier: &str, desc: &str, json: bool, out: &mut serde_json::Map<String, serde_json::Value>) {
    if json {
        out.insert(tier.to_string(), serde_json::Value::String(desc.to_string()));
    } else {
        println!("{tier}: {desc}");
    }
}

fn run_sandbox(args: crate::cli::args::SandboxArgs) -> Result<()> {
    // Offline SRD diff: no guest execution required.
    if let Some(paths) = &args.srd_diff {
        anyhow::ensure!(
            paths.len() == 2,
            "--srd-diff expects exactly two snapshot paths"
        );
        let left = fission_emulator::SemanticReplaySnapshot::read_json_file(&paths[0])
            .with_context(|| format!("read SRD left {}", paths[0].display()))?;
        let right = fission_emulator::SemanticReplaySnapshot::read_json_file(&paths[1])
            .with_context(|| format!("read SRD right {}", paths[1].display()))?;
        let delta = fission_emulator::SemanticReplayDelta::diff(&left, &right);
        let json = delta.to_json_pretty().context("serialize SRD delta")?;
        if let Some(out) = &args.srd_diff_out {
            std::fs::write(out, &json)
                .with_context(|| format!("write SRD delta to {}", out.display()))?;
            tracing::info!("Wrote SRD delta to {}", out.display());
        }
        println!("{json}");
        return Ok(());
    }

    let binary_path = args.binary.as_ref().ok_or_else(|| {
        anyhow::anyhow!("sandbox requires BINARY (or use --srd-diff LEFT RIGHT for offline SRD)")
    })?;

    let mut logging_options =
        fission_core::logging::LoggingOptions::from_config(&fission_core::CONFIG.logging);
    logging_options.level = "debug".to_string(); // Force debug for sandbox
    logging_options.include_span_events = true;
    fission_core::logging::init_with_options(logging_options);

    tracing::info!("Starting sandbox for {}", binary_path.display());

    // Parse binary using fission-loader
    let binary = fission_loader::loader::LoadedBinary::from_file(binary_path)
        .with_context(|| format!("failed to read binary at {}", binary_path.display()))?;

    let mut state = fission_emulator::MachineState::new();

    // Load binary sections into emulator RAM (format-aware)
    let mut linux_image = None;
    let mut pe_image = None;
    match binary.format.as_str() {
        "PE" => {
            pe_image = Some(fission_emulator::os::windows::loader::load_pe(
                &mut state, &binary,
            )?);
        }
        "ELF" | "ELF64" => {
            linux_image = Some(fission_emulator::os::linux::loader::load_elf(
                &mut state, &binary,
            )?);
        }
        fmt => anyhow::bail!("Unsupported binary format for sandbox loader: {}", fmt),
    }

    // Initialize Sleigh Frontend
    let load_spec = binary
        .load_spec()
        .with_context(|| "Binary lacks load_spec")?;
    let frontends =
        fission_sleigh::runtime::RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(
            load_spec,
        )
        .with_context(|| "Failed to create Sleigh frontend candidates")?;
    let sleigh = frontends
        .into_iter()
        .next()
        .with_context(|| "No suitable Sleigh frontend found")?;

    // Derive ArchInfo from Sleigh Language ID + binary format
    let lang_id = load_spec.pair.language_id.as_str();
    let arch = fission_emulator::ArchInfo::from_language_id(lang_id, Some(&binary))
        .with_context(|| format!("Unsupported architecture: {}", lang_id))?;

    // Choose OS environment based on binary format
    let os: Box<dyn fission_emulator::OsEnvironment> = match binary.format.as_str() {
        "PE" => Box::new(fission_emulator::WindowsEnv::new()),
        "ELF" | "ELF64" => Box::new(fission_emulator::LinuxEnv::new()),
        fmt => anyhow::bail!("Unsupported binary format for sandbox: {}", fmt),
    };

    // Create Emulator and Run
    let mut emu = fission_emulator::core::Emulator::new(state, binary, sleigh, arch, os)?
        .with_max_inst(args.max_inst)
        .with_stdin_mock(args.stdin_mock)
        .with_ttd(args.ttd_record.unwrap_or(0));

    if let Some(info) = linux_image {
        emu.apply_linux_image(info)?;
    }
    if let Some(info) = pe_image {
        emu.apply_windows_image(info)?;
    }

    // Setup Ctrl+C handler
    ctrlc::set_handler(move || {
        fission_emulator::core::IS_INTERRUPTED.store(true, std::sync::atomic::Ordering::Relaxed);
    })
    .unwrap_or_else(|e| tracing::warn!("Failed to set Ctrl+C handler: {}", e));

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

    if args.sym_explore {
        let mut sym_runner = fission_emulator::sym::SymbolicExecutor::new(emu);
        sym_runner.explore()?;
        emu = sym_runner.emu;
    } else {
        emu.run()?;
    }

    if let Some(seek_step) = args.ttd_seek {
        tracing::info!("Seeking to TTD step {} after execution", seek_step);
        emu.ttd_seek(seek_step)?;
        tracing::info!("PC after seek: 0x{:X}", emu.pc);
        // We could also dump registers here if requested, but for now we just seek.
    }

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

    // Metrics JSON + optional quality budget (opcodes + HLE misses + unknown syscalls).
    let want_budget = args.fail_on_budget
        || args.max_unimpl_events.is_some()
        || args.max_unimpl_kinds.is_some()
        || args.max_hle_misses.is_some()
        || args.max_unknown_syscalls.is_some();
    let binary_label = binary_path.display().to_string();
    let report = if want_budget {
        let max_events = args.max_unimpl_events.unwrap_or(0);
        let max_kinds = args.max_unimpl_kinds.unwrap_or(0);
        // HLE gates: unlimited unless explicitly set (or fail_on_budget with explicit flags).
        let max_hle = args.max_hle_misses.unwrap_or(u64::MAX);
        let max_unk = args.max_unknown_syscalls.unwrap_or(u64::MAX);
        fission_emulator::SandboxMetricsReport::from_run_quality(
            binary_label.clone(),
            emu.binary.format.clone(),
            emu.halt_requested,
            emu.pc,
            emu.metrics.clone(),
            Some((max_events, max_kinds, max_hle, max_unk)),
        )
    } else {
        fission_emulator::SandboxMetricsReport::from_run(
            binary_label.clone(),
            emu.binary.format.clone(),
            emu.halt_requested,
            emu.pc,
            emu.metrics.clone(),
            None,
        )
    };
    if let Some(path) = args.metrics_out {
        let json = report
            .to_json_pretty()
            .context("serialize sandbox metrics report")?;
        std::fs::write(&path, json)
            .with_context(|| format!("write metrics to {}", path.display()))?;
        tracing::info!("Wrote sandbox metrics to {}", path.display());
    }
    if args.json {
        println!(
            "{}",
            report
                .to_json_pretty()
                .context("serialize sandbox metrics report")?
        );
    }
    if args.fail_on_budget && !report.budget_ok() {
        let err = report
            .budget
            .as_ref()
            .and_then(|b| b.error.clone())
            .unwrap_or_else(|| "quality budget exceeded".into());
        anyhow::bail!("{err}");
    }

    if let Some(path) = args.srd_out {
        let label = args.srd_label.clone().unwrap_or_else(|| {
            binary_path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "run".into())
        });
        let snap = fission_emulator::SemanticReplaySnapshot::capture(
            &mut emu,
            fission_emulator::CaptureOpts {
                label,
                binary: binary_label,
                probe_mallocng: args.srd_mallocng,
                ..Default::default()
            },
        );
        snap.write_json_file(&path)
            .with_context(|| format!("write SRD snapshot to {}", path.display()))?;
        tracing::info!("Wrote SRD snapshot to {}", path.display());
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

    if cli.identify_cmd {
        return Ok(run_identify(cli, &binary)?);
    }

    if cli.list {
        return Ok(print_function_list(
            &binary,
            cli.json,
            cli.include_nonuser_functions,
        )?);
    }

    if cli.preview_candidate_inventory {
        anyhow::bail!("preview candidate inventory is deprecated with native_decomp removal");
    }

    if cli.preview_candidate_scan_batch {
        anyhow::bail!("preview candidate scan batch is deprecated with native_decomp removal");
    }

    if cli.emit_function_facts_inventory {
        return Ok(emit_function_facts_inventory(cli, &binary, &binary_data)?);
    }

    if cli.emit_program_metadata {
        return emit_program_metadata(cli, &binary);
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

        run_decompilation_rust_sleigh(cli, &binary, &binary_data)?;
        return Ok(());
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
