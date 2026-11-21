//! One-Shot CLI - Single command execution mode
//!
//! Executes a single command and exits (non-interactive).

mod analysis_db;
mod assessment;
mod binary_info;
mod callgraph;
#[cfg(feature = "debugger")]
mod debug;
mod debug_bundle_extra;
mod debug_decomp;
#[cfg(feature = "debugger")]
mod debug_session;
mod disasm;
mod function_select;
mod functions;
mod hex;
mod identify;
mod inventory;
mod languages;
mod nir_stats;
mod patch;
mod pcode_diagnostics;
mod pcode_stages;
mod pcode_topology;
mod raw_pcode;
mod rust_decomp;
mod script;
mod search;
mod similar;
mod strings;
mod xrefs;

use binary_info::{print_binary_info, print_exports, print_imports, print_sections};
pub use callgraph::build_protocol_callgraph;
use callgraph::run_callgraph;
pub use disasm::to_disasm_response;
use disasm::{disassemble, disassemble_function};
pub(crate) use function_select::NameLookupError;
use functions::print_function_list;
pub use functions::{build_functions_response, function_info_to_fn_entry};
use hex::print_hex;
use identify::run_identify;
use inventory::{emit_function_facts_inventory, emit_program_metadata};
use nir_stats::emit_nir_stats;
pub use patch::PatchRequest;
use patch::run_patch;
use pcode_stages::emit_pcode_stages;
use pcode_topology::emit_pcode_topology;
use raw_pcode::emit_raw_pcode;
use rust_decomp::run_decompilation_rust_sleigh;
pub use search::SearchQuery;
use search::run_search;
use similar::run_similar;
use strings::print_strings;
use xrefs::run_xrefs;

use crate::cli::args::{
    LegacyInvocationKind, OneShotArgs, ParsedInvocation, ParsedOneShotArgs, parse_oneshot_args,
};
use anyhow::{Context, Result};
use fission_loader::loader::LoadedBinary;
use fission_static::analysis::discover_functions_with_runtime;
use script::execute_script;
use std::fs;
use std::io;

/// Force `binary`'s SLEIGH language to `language_id`, bypassing the usual
/// ELF/PE/Mach-O `.opinion`-based auto-detection entirely.
///
/// Most of Fission's ~130 lift-ready languages (see `fission-sleigh`'s
/// runtime-readiness audit -- 100% of constructor templates covered)
/// describe raw-firmware/embedded targets (8-bit MCUs, legacy ISA variants)
/// that never carry an ELF/PE/Mach-O header to auto-detect from -- same as
/// in Ghidra itself, where these need to be picked from a language list,
/// not inferred. This is that manual picker for the CLI.
fn language_descriptor(
    language_id: &str,
) -> Result<fission_core::architecture::ArchitectureDescriptor> {
    let known = fission_core::architecture::known_language_ids()
        .map_err(|e| anyhow::anyhow!("failed to load SLEIGH language manifest: {e}"))?;
    if !known.contains(language_id) {
        let prefix = language_id.split(':').next().unwrap_or(language_id);
        let mut suggestions: Vec<&String> =
            known.iter().filter(|id| id.starts_with(prefix)).collect();
        suggestions.sort();
        anyhow::bail!(
            "unknown --language `{language_id}` (not a lift-ready SLEIGH language id).{}",
            if suggestions.is_empty() {
                String::new()
            } else {
                format!(
                    "\nDid you mean one of:\n  {}",
                    suggestions
                        .iter()
                        .take(15)
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join("\n  ")
                )
            }
        );
    }

    let parts: Vec<&str> = language_id.split(':').collect();
    let [processor, endian, size, variant] = parts[..] else {
        anyhow::bail!(
            "malformed --language id `{language_id}` (expected processor:endian:size:variant)"
        );
    };
    let bitness: u8 = size.parse().map_err(|_| {
        anyhow::anyhow!("malformed --language id `{language_id}`: size `{size}` is not a number")
    })?;

    let endian_word = match endian {
        "LE" | "le" => "little",
        "BE" | "be" => "big",
        other => other,
    };
    Ok(fission_core::architecture::ArchitectureDescriptor {
        processor: processor.to_string(),
        endian: endian_word.to_string(),
        bitness,
        variant: variant.to_string(),
        abi: None,
        raw_machine: "cli-language-override".to_string(),
    })
}

fn apply_language_override(binary: &mut LoadedBinary, language_id: &str) -> Result<()> {
    let architecture = language_descriptor(language_id)?;
    binary.load_spec = Some(fission_core::architecture::BinaryLoadSpec::new(
        binary.format.clone(),
        binary.image_base,
        language_id,
        "default",
        "cli-language-override",
    ));
    binary.arch_spec = language_id.to_string();
    binary.is_64bit = architecture.bitness == 64;
    binary.architecture = Some(architecture);
    Ok(())
}

/// Load `data` as a raw image mapped at `base`: one executable section
/// holding the whole file, one function at `entry`. A raw image carries no
/// header, so the language is the caller's to give.
fn load_raw_image(
    data: Vec<u8>,
    path: String,
    base: u64,
    entry: Option<u64>,
    language_id: Option<&str>,
) -> Result<LoadedBinary> {
    let language_id = language_id.ok_or_else(|| {
        anyhow::anyhow!(
            "--base loads the file as a raw image, which says nothing about what it holds; \
             give --language too (`fission_cli languages` lists the IDs)"
        )
    })?;
    let architecture = language_descriptor(language_id)?;
    let entry = entry.unwrap_or(base);
    let end = base.checked_add(data.len() as u64).ok_or_else(|| {
        anyhow::anyhow!("--base 0x{base:x} plus the file's size overflows the address space")
    })?;
    anyhow::ensure!(
        (base..end).contains(&entry),
        "--entry 0x{entry:x} is outside the image (0x{base:x}..0x{end:x})"
    );
    let hint = fission_loader::loader::formats::raw::RawLoadHint {
        image_base: base,
        entry_point: entry,
        architecture,
        load_spec: fission_core::architecture::BinaryLoadSpec::new(
            "Raw Binary",
            base,
            language_id,
            "default",
            "cli-raw-load",
        ),
    };
    fission_loader::loader::formats::raw::BinaryLoader::parse_with_hint(
        fission_loader::loader::DataBuffer::Heap(data),
        path,
        hint,
    )
    .map_err(|err| anyhow::anyhow!("failed to load raw image: {err}"))
}

/// Print a one-time stderr warning when the DIE detection engine
/// (`fission_loader::detect`, backed by the full `utils/signatures/die/`
/// mirror) finds a Packer/Protector/SFX on this binary at Medium+
/// confidence.
///
/// This data was already being computed for every binary load (`fission_cli
/// info --detections` has always been able to show it), but nothing
/// surfaced it during ordinary decompilation -- a packed/protected
/// binary's `.text` is largely encrypted/compressed stub code, so
/// decompiling it as-is produces plausible-looking but meaningless output
/// with no indication why. Low-confidence hits are suppressed here (kept
/// for `info --detections`'s more exhaustive listing) since a single weak
/// indicator isn't worth interrupting every run for.
/// Where this invocation's analysis database lives, unless it was told not to
/// have one.
pub(crate) fn analysis_db_path(
    binary_path: &std::path::Path,
    explicit: Option<&std::path::Path>,
    disabled: bool,
) -> Option<std::path::PathBuf> {
    if disabled {
        return None;
    }
    Some(match explicit {
        Some(path) => path.to_path_buf(),
        None => fission_project::Project::default_path(binary_path),
    })
}

/// Read the analysis database and put its decisions onto the binary.
///
/// A database for a *different* binary is an error rather than a shrug: its
/// names would land on whatever happens to live at those addresses and the
/// result would read like analysis. A missing one is the ordinary case and
/// says nothing.
fn apply_analysis_db(cli: &OneShotArgs, binary: &mut LoadedBinary) -> Result<()> {
    let Some(path) = analysis_db_path(&cli.binary, cli.analysis_db.as_deref(), cli.no_analysis_db)
    else {
        return Ok(());
    };
    let Some(project) =
        fission_project::Project::read(&path).map_err(|e| anyhow::anyhow!("{e}"))?
    else {
        return Ok(());
    };
    let report = project
        .apply(binary)
        .map_err(|e| anyhow::anyhow!("{e} (database: {})", path.display()))?;
    if cli.verbose && report.total_applied() > 0 {
        eprintln!(
            "[*] {} decision(s) ({} name(s), {} signature(s), {} comment(s)) from {}",
            report.total_applied(),
            report.names_applied,
            report.signatures_applied,
            report.comments_applied,
            path.display()
        );
    }
    Ok(())
}

fn warn_if_packed_or_protected(binary: &LoadedBinary) {
    let detection = fission_loader::detect(binary);
    let hits: Vec<&fission_loader::Detection> = detection
        .detections
        .iter()
        .filter(|d| {
            matches!(
                d.detection_type,
                fission_loader::DetectionType::Packer
                    | fission_loader::DetectionType::Protector
                    | fission_loader::DetectionType::Sfx
            ) && d.confidence >= fission_loader::Confidence::Medium
        })
        .collect();
    if hits.is_empty() {
        return;
    }
    let names: Vec<String> = hits
        .iter()
        .map(|d| match &d.version {
            Some(v) => format!("{} {v}", d.name),
            None => d.name.clone(),
        })
        .collect();
    eprintln!(
        "[!] WARNING: binary appears packed/protected ({}) -- decompiled output may be unreliable until unpacked. Run `fission_cli info --detections` for the full list.",
        names.join(", ")
    );
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
    fission_core::resource_roots::validate_user_signature_dirs()
        .map_err(|message| anyhow::anyhow!(message))?;
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
                return Err(match fission_core::logging::span_trace_context() {
                    Some(context) => error.context(context),
                    None => error,
                });
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
                return Err(match fission_core::logging::span_trace_context() {
                    Some(context) => error.context(context),
                    None => error,
                });
            }
            return Ok(());
        }
        ParsedInvocation::Db(args) => return analysis_db::run(args),
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
        ParsedInvocation::Sandbox(args) => run_sandbox(args),
        ParsedInvocation::Verify(args) => run_verify(args),
        ParsedInvocation::Schema(args) => crate::cli::contract::run_schema(&args),
        ParsedInvocation::Skill => crate::cli::skill::run_skill(),
        ParsedInvocation::Languages(args) => languages::run(&args),
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
        return Err(match fission_core::logging::span_trace_context() {
            Some(context) => error.context(context),
            None => error,
        });
    }
    Ok(())
}

/// Heavy, opt-in "deep verify" over `fission-dir`: concrete PreHIR/HIR
/// diffing, emulator-grounded ground truth, and solver-backed symbolic
/// equivalence. Distinct from `scripts/quality/prehir_hir_check.py`, which
/// stays the fast, cheap, no-solver/no-emulator structural heuristic run
/// routinely across the whole corpus -- this command spins up a real
/// emulator and/or solver per function and is meant for targeted
/// investigation, not a per-commit gate.
fn run_verify(args: crate::cli::args::VerifyArgs) -> Result<()> {
    use crate::cli::args::VerifyTierArg;

    let mut logging_options =
        fission_core::logging::LoggingOptions::from_config(&fission_core::CONFIG.logging);
    let batch = args.addr.is_none();
    // A batch runs thousands of emulator calls; the emulator's per-call
    // warnings (an unmodeled libc routine) would bury the summary.
    logging_options.level = if batch { "error" } else { "warn" }.to_string();
    fission_core::logging::init_with_options(logging_options);

    let binary = LoadedBinary::from_file(&args.binary)
        .with_context(|| format!("failed to read binary at {}", args.binary.display()))?;
    let facts =
        fission_static::analysis::decomp::facts::FactStore::from_binary_without_signature_matches(
            &binary,
        );

    let functions = if let Some(addr) = args.addr {
        vec![fission_loader::loader::FunctionInfo {
            name: args.name.clone(),
            address: addr,
            ..Default::default()
        }]
    } else if let Some(path) = &args.addresses_file {
        function_select::select_functions_from_addresses_file(&binary, path)
            .with_context(|| format!("failed to read {}", path.display()))?
    } else {
        function_select::select_batch_functions(&binary, false, None).functions
    };

    let run_concrete = matches!(args.tier, VerifyTierArg::Concrete | VerifyTierArg::All);
    let run_ground_truth = matches!(args.tier, VerifyTierArg::GroundTruth | VerifyTierArg::All);
    let run_symbolic = matches!(args.tier, VerifyTierArg::Symbolic | VerifyTierArg::All);

    // One emulator per binary: building it loads the whole image, and every
    // call restores the same baseline, so functions cannot see each other.
    let mut harness = if run_ground_truth {
        Some(
            fission_dir::EmulatorHarness::build(&args.binary, Some(args.max_inst))
                .map_err(|err| err.to_string()),
        )
    } else {
        None
    };

    let mut counts: std::collections::BTreeMap<(String, String), usize> = Default::default();
    let mut reasons: std::collections::BTreeMap<(String, String), usize> = Default::default();
    for func in &functions {
        let mut out = serde_json::Map::new();
        if batch {
            out.insert("address".into(), format!("0x{:x}", func.address).into());
            out.insert("name".into(), func.name.clone().into());
        }
        let mut tiers: Vec<(&str, String, String, String, serde_json::Value)> = Vec::new();
        match fission_dir::decompile_one(&binary, &facts, func) {
            Err(err) => {
                if !batch {
                    return Err(err).with_context(|| {
                        format!("failed to decompile function at 0x{:x}", func.address)
                    });
                }
                tiers.push((
                    "decompile",
                    "error".into(),
                    err.to_string(),
                    err.to_string(),
                    serde_json::json!({}),
                ));
            }
            Ok(pair) => {
                if run_concrete {
                    let samples = fission_dir::default_samples(pair.hir.params.len());
                    let outcome = fission_dir::diff_prehir_hir(&pair.prehir, &pair.hir, &samples);
                    tiers.push((
                        "concrete",
                        outcome.category().into(),
                        format!("{outcome:?}"),
                        verify_outcome_reason(&outcome),
                        verify_outcome_fields(&outcome),
                    ));
                }
                if let Some(harness) = harness.as_mut() {
                    match harness {
                        Ok(harness) => {
                            let plan = fission_dir::RunPlan {
                                runs: args.runs,
                                seed: args.seed,
                            };
                            let grounding =
                                fission_dir::Grounding::for_function(&binary, func.address, &pair);
                            let outcome = fission_dir::check_ground_truth_with(
                                harness,
                                func.address,
                                &pair.prehir,
                                &pair.hir,
                                &plan,
                                &grounding,
                            );
                            tiers.push((
                                "ground_truth",
                                outcome.category().into(),
                                format!("{outcome:?}"),
                                verify_outcome_reason(&outcome),
                                verify_outcome_fields(&outcome),
                            ));
                        }
                        Err(err) => tiers.push((
                            "ground_truth",
                            "harness".into(),
                            format!("EmulatorSetupFailed({err})"),
                            format!("emulator setup failed: {err}"),
                            serde_json::json!({}),
                        )),
                    }
                }
                if run_symbolic {
                    let outcome = fission_dir::check_symbolic_equivalence(&pair.prehir, &pair.hir);
                    let (category, desc, reason, fields) = match outcome {
                        fission_dir::symbolic::SymbolicOutcome::Equivalent => (
                            "equivalent",
                            "Equivalent (proved, Unsat)".to_string(),
                            String::new(),
                            serde_json::json!({ "proved": true }),
                        ),
                        fission_dir::symbolic::SymbolicOutcome::Diverged(cx) => (
                            "diverged",
                            format!("Diverged (solver counterexample args={:?})", cx.args),
                            "solver counterexample".to_string(),
                            serde_json::json!({
                                "counterexamples": [{
                                    "args": cx.args.iter().map(|a| format!("0x{a:x}")).collect::<Vec<_>>(),
                                }],
                            }),
                        ),
                        fission_dir::symbolic::SymbolicOutcome::Unsupported(reason) => (
                            "unsupported",
                            format!("Unsupported({reason})"),
                            reason.to_string(),
                            serde_json::json!({}),
                        ),
                    };
                    tiers.push(("symbolic", category.into(), desc, reason, fields));
                }
            }
        }
        let mut structured = serde_json::Map::new();
        for (tier, category, desc, reason, fields) in tiers {
            let mut tier_obj = serde_json::Map::new();
            tier_obj.insert("category".into(), category.clone().into());
            tier_obj.insert("detail".into(), desc.clone().into());
            tier_obj.insert("reason".into(), reason.clone().into());
            if let serde_json::Value::Object(fields) = fields {
                tier_obj.extend(fields);
            }
            *counts
                .entry((tier.to_string(), category.clone()))
                .or_default() += 1;
            if batch {
                if category != "equivalent" {
                    *reasons
                        .entry((tier.to_string(), verify_reason_key(&reason)))
                        .or_default() += 1;
                }
                out.insert(tier.to_string(), serde_json::Value::Object(tier_obj));
            } else {
                report_tier(tier, &desc, args.json, &mut out);
                structured.insert(tier.to_string(), serde_json::Value::Object(tier_obj));
            }
        }
        if !batch && args.json {
            // The per-tier strings above are the original single-function
            // shape and stay as they are; the same results as fields live
            // under `tiers`.
            out.insert("tiers".into(), serde_json::Value::Object(structured));
        }
        if args.json || batch {
            out.insert("seed".into(), args.seed.into());
        }
        if batch || args.json {
            println!("{}", serde_json::Value::Object(out));
        }
    }

    if batch {
        eprintln!("verified {} functions", functions.len());
        for ((tier, category), n) in &counts {
            eprintln!("  {tier:<13} {category:<12} {n}");
        }
        let mut top: Vec<_> = reasons.into_iter().collect();
        top.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        eprintln!("most common reasons:");
        for ((tier, reason), n) in top.into_iter().take(25) {
            eprintln!("  {n:>6}  {tier:<13} {reason}");
        }
    }
    Ok(())
}

/// A tier's outcome as fields, so a caller does not have to parse the
/// `Debug` text in `detail`: how many inputs agreed, and each input that did
/// not, with every side's result.
fn verify_outcome_fields(outcome: &fission_dir::VerifyOutcome) -> serde_json::Value {
    fn divergence(d: &fission_dir::Divergence) -> serde_json::Value {
        serde_json::json!({
            "run": d.run,
            "args": d.args,
            "prehir_result": d.prehir_result,
            "hir_result": d.hir_result,
            "emulator_result": d.emulator_result,
            "detail": d.detail,
        })
    }
    match outcome {
        fission_dir::VerifyOutcome::Equivalent { checked } => {
            serde_json::json!({ "checked": checked })
        }
        fission_dir::VerifyOutcome::Diverged(found)
        | fission_dir::VerifyOutcome::Suspect(found) => {
            serde_json::json!({ "counterexamples": found.iter().map(divergence).collect::<Vec<_>>() })
        }
        fission_dir::VerifyOutcome::Unsupported(_)
        | fission_dir::VerifyOutcome::Inconclusive { .. } => serde_json::json!({}),
    }
}

/// What made a tier say something other than "equivalent", in words.
fn verify_outcome_reason(outcome: &fission_dir::VerifyOutcome) -> String {
    match outcome {
        fission_dir::VerifyOutcome::Equivalent { .. } => String::new(),
        fission_dir::VerifyOutcome::Diverged(_) => "diverged".to_string(),
        fission_dir::VerifyOutcome::Suspect(_) => {
            "an argument differs at a call with a recovered prototype".to_string()
        }
        fission_dir::VerifyOutcome::Unsupported(reason) => reason.to_string(),
        fission_dir::VerifyOutcome::Inconclusive { reason } => reason.clone(),
    }
}

/// A reason with the parts that differ per function removed, so a batch can
/// count them: numbers and single-quoted names become placeholders.
fn verify_reason_key(desc: &str) -> String {
    let mut key = String::with_capacity(desc.len().min(160));
    let mut chars = desc.chars().peekable();
    while let Some(c) = chars.next() {
        if key.len() >= 160 {
            break;
        }
        match c {
            '\'' => {
                key.push(c);
                key.push('_');
                for d in chars.by_ref() {
                    if d == c {
                        break;
                    }
                }
                key.push(c);
            }
            '0'..='9' => {
                key.push('N');
                while chars
                    .peek()
                    .is_some_and(|d| d.is_ascii_hexdigit() || *d == 'x')
                {
                    chars.next();
                }
            }
            _ => key.push(c),
        }
    }
    key
}

fn report_tier(
    tier: &str,
    desc: &str,
    json: bool,
    out: &mut serde_json::Map<String, serde_json::Value>,
) {
    if json {
        out.insert(
            tier.to_string(),
            serde_json::Value::String(desc.to_string()),
        );
    } else {
        println!("{tier}: {desc}");
    }
}

/// How many explored states the sandbox report lists with solved inputs;
/// each one costs a solver query, and the counts per stash cover the rest.
const SYMBOLIC_REPORT_STATE_CAP: usize = 64;

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

    // Dynamic linking is decided once for the process, before any image is
    // mapped. Options left unset keep reading their environment variables.
    if args.dynlink || args.ld_so.is_some() || !args.lib_path.is_empty() || args.lazy_bind {
        fission_emulator::os::linux::dynlink::set_dynlink_options(
            fission_emulator::os::linux::dynlink::DynlinkOptions {
                host_interpreter: args.dynlink.then_some(true),
                ld_so: args.ld_so.clone(),
                lib_paths: args.lib_path.clone(),
                lazy_bind: args.lazy_bind.then_some(true),
            },
        );
    }

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
            // argv[0] mirrors what a real `execve` gives the guest when
            // invoked as `./binary`: the path as named on the command line,
            // not the loader's placeholder "fission-guest". Without a real
            // argv[0] plus the user's own trailing args here, `if (argc !=
            // 2) return 2;` (or any argument-reading crackme) always takes
            // the same "wrong invocation" branch, so dynamic confirmation
            // can never actually run the path it exists to check.
            let mut argv = vec![binary_path.display().to_string()];
            argv.extend(args.args.iter().cloned());
            let process_args = fission_emulator::os::linux::ProcessArgs {
                argv,
                ..fission_emulator::os::linux::ProcessArgs::default()
            };
            linux_image = Some(fission_emulator::os::linux::loader::load_elf_with_args(
                &mut state,
                &binary,
                &process_args,
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
        .with_deadline(
            args.timeout_ms
                .map(|ms| std::time::Instant::now() + std::time::Duration::from_millis(ms)),
        )
        .with_max_inst(args.max_inst)
        // The guest's console reads reach this process's stdin only when a
        // person is at it. Piped or backgrounded, an open stdin that never
        // closes would block the run forever on the first REPL prompt.
        .with_host_stdin(
            args.stdin_mock.is_none() && std::io::IsTerminal::is_terminal(&std::io::stdin()),
        )
        .with_stdin_mock(args.stdin_mock)
        .with_ttd(args.ttd_record.unwrap_or(0));
    // Protocol stdout is one JSON document. The guest bytes remain in VFS
    // and are attached to the typed report; text mode keeps console echo.
    emu.set_host_stdout_echo(!args.json);

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

    // The behaviour log needs no instrumentation in compiled code -- syscalls
    // and HLE stubs dispatch outside the JIT -- so it is always on. Coverage
    // costs a callback per block, so it is asked for.
    emu.add_observer(Box::new(fission_emulator::observe::BehaviorLog::new()));
    if args.coverage {
        emu.add_observer(Box::new(fission_emulator::observe::Coverage::new()));
    }
    if args.taint {
        emu.set_shadow_mode(fission_emulator::observe::ShadowMode::Taint);
    }

    tracing::info!("Starting Emulator Execution Loop at PC=0x{:X}", emu.pc);

    if args.interp {
        emu.force_interpreter = true;
    }

    let mut symbolic_report = None;
    if args.sym_explore {
        let mut sym_runner =
            fission_emulator::sym::SymbolicExecutor::new(emu).with_max_steps(args.sym_max_steps);
        if let Some(target) = args.find {
            sym_runner.use_technique(Box::new(
                fission_emulator::sym::exploration::DirectedSearch::new(target, args.avoid.clone()),
            ));
        }
        sym_runner.explore()?;
        symbolic_report = Some(sym_runner.report(SYMBOLIC_REPORT_STATE_CAP));
        emu = sym_runner.emu;
    } else {
        emu.run()?;
    }

    let mut ttd_state = None;
    if let Some(seek_step) = args.ttd_seek {
        tracing::info!("Seeking to TTD step {} after execution", seek_step);
        emu.ttd_seek(seek_step)?;
        tracing::info!("PC after seek: 0x{:X}", emu.pc);
        ttd_state = Some(emu.machine_state_report(&args.ttd_read));
    }

    if let Some(path) = &args.snapshot_out {
        let snapshot = emu.snapshots.first().ok_or_else(|| {
            anyhow::anyhow!(
                "the run never reached --snapshot-at 0x{:x}, so there is no snapshot to write",
                args.snapshot_at.unwrap_or_default()
            )
        })?;
        snapshot
            .save_to_disk(path)
            .with_context(|| format!("write snapshot to {}", path.display()))?;
        tracing::info!("Wrote snapshot to {}", path.display());
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
    let observers = emu.take_observers();
    let behavior_log = observers.iter().find_map(|o| {
        o.as_any()
            .downcast_ref::<fission_emulator::observe::BehaviorLog>()
    });
    let coverage = observers.iter().find_map(|o| {
        o.as_any()
            .downcast_ref::<fission_emulator::observe::Coverage>()
    });
    let report = match behavior_log {
        Some(log) => report.with_behavior(log, coverage),
        None => report,
    };
    let report = if args.taint {
        report.with_taint(&emu.taint)
    } else {
        report
    };
    if let Some(code) = emu.exit_code {
        tracing::info!("Guest exited with code {code}");
    }
    let report = report
        .with_exit_code(emu.exit_code)
        .with_guest_output(&emu.vfs);
    let report = match symbolic_report {
        Some(symbolic) => report.with_symbolic(symbolic),
        None => report,
    };
    let report = match ttd_state {
        Some(state) => report.with_ttd_state(state),
        None => report,
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
        return Err(anyhow::Error::new(
            crate::cli::contract::QualityBudgetExceeded(err),
        ));
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

    // A universal file holds several architectures, and every later command
    // sees only the one taken here. The inventory is read before slicing so
    // `info` can report what the file carries, not just what was picked.
    let universal = fission_loader::loader::macho::universal_slices(&binary_data);
    let binary_data = match cli.arch.as_deref() {
        Some(arch) => {
            let range = fission_loader::loader::macho::universal_slice_range(&binary_data, arch)
                .ok_or_else(|| {
                    let carried: Vec<&str> =
                        universal.iter().map(|slice| slice.arch.as_str()).collect();
                    if carried.is_empty() {
                        anyhow::anyhow!(
                            "`{}` is not a universal binary; --arch has nothing to choose from",
                            cli.binary.display()
                        )
                    } else {
                        anyhow::anyhow!(
                            "`{}` carries {}, not `{arch}`",
                            cli.binary.display(),
                            carried.join(", ")
                        )
                    }
                })?;
            binary_data[range].to_vec()
        }
        None => binary_data,
    };

    anyhow::ensure!(
        cli.raw_entry.is_none() || cli.raw_base.is_some(),
        "--entry only applies to a raw image; give --base too"
    );
    let mut binary = match cli.raw_base {
        Some(base) => load_raw_image(
            binary_data.clone(),
            cli.binary.to_string_lossy().to_string(),
            base,
            cli.raw_entry,
            cli.language_override.as_deref(),
        )?,
        None => LoadedBinary::from_bytes(
            binary_data.clone(),
            cli.binary.to_string_lossy().to_string(),
        )
        .with_context(|| {
            if cli.language_override.is_some() {
                format!(
                    "failed to parse binary `{}` (to read it as a raw image, give --base <ADDR>)",
                    cli.binary.display()
                )
            } else {
                format!("failed to parse binary `{}`", cli.binary.display())
            }
        })?,
    };

    if let Some(language_id) = cli.language_override.as_deref() {
        apply_language_override(&mut binary, language_id)?;
        if cli.verbose {
            eprintln!("[*] Forced SLEIGH language: {language_id}");
        }
    }

    // The one function discovery. Every command that reads the function table
    // reads the table this builds, so `list`, `decomp --all`, `callgraph`,
    // `xrefs` and the rest cannot disagree about which functions exist.
    let report = discover_functions_with_runtime(&mut binary);
    if cli.verbose {
        eprintln!(
            "[*] SLEIGH function discovery: +{} functions (decoded={}, calls={}, jumps={}, unsupported_runtime={})",
            report.accepted_function_count,
            report.decoded_instruction_count,
            report.call_target_count,
            report.jump_target_count,
            report.unsupported_runtime
        );
    }

    if cli.verbose {
        eprintln!(
            "[ok] Loaded: {} ({}-bit, {} functions)",
            cli.binary.display(),
            if binary.is_64bit { 64 } else { 32 },
            binary.functions.len()
        );
    }

    // Decisions someone made about this binary, put back onto it before
    // anything reads it. Everything downstream -- listing, decompiling,
    // disassembling -- goes through this same `LoadedBinary`, so a chosen
    // name reaches all of them from here and nowhere else.
    apply_analysis_db(cli, &mut binary)?;

    warn_if_packed_or_protected(&binary);

    if cli.info {
        return Ok(print_binary_info(
            &binary,
            &universal,
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
        return Ok(print_imports(&binary, cli.info_xrefs, cli.json)?);
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

    if cli.similar_cmd {
        return Ok(run_similar(cli, &binary)?);
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

    if let Some(request) = &cli.patch {
        return Ok(run_patch(&mut binary, request, cli.json)?);
    }

    if let Some(query) = &cli.search {
        return Ok(run_search(&binary, query, cli.json)?);
    }

    if let Some(addr) = cli.hex_addr {
        return Ok(print_hex(&binary, addr, cli.hex_count, cli.json)?);
    }

    if let Some(min_len) = cli.strings {
        return Ok(print_strings(
            &binary,
            &binary_data,
            // The caller's threshold, not a silent floor: `--min-len 2` used to
            // behave as 4. The default (4) is set where the flag is declared.
            min_len.max(1),
            &strings::StringsView {
                with_xrefs: cli.strings_xrefs,
                context: cli.strings_context,
                contains: cli.strings_contains.clone(),
                sections: cli.strings_sections.clone(),
            },
            cli.json,
        )?);
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

    let by_name;
    let cli = match cli.function_name.as_deref() {
        Some(name) => {
            let address = function_select::select_function_address_by_name(&binary, name)
                .map_err(crate::cli::contract::FunctionSelectorError)?;
            by_name = OneShotArgs {
                address: Some(address),
                function_name: None,
                ..cli.clone()
            };
            &by_name
        }
        None => cli,
    };

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
    println!(
        "  fission_cli xrefs <binary> [--json] [--no-disassembly] [--pcode] [--function ADDR]"
    );
    println!("  fission_cli callgraph <binary> [--json]");
    println!("  fission_cli inventory <SUBCOMMAND> <binary> [OPTIONS]");
    println!("  fission_cli script check --script <FILE>");
    println!("  fission_cli script run <binary> --script <FILE> [--json]");
    println!("  fission_cli debug capabilities [--json]");
    println!("  fission_cli debug --emulator capabilities [--json]");
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
    println!("  xrefs      Canonical xref index (loader + optional disassembly/p-code)");
    println!("  callgraph  Call graph from xref analysis");
    println!("  inventory  Operator-oriented inventory and batch emitters");
    println!("  script     Rhai automation against read-only binary inventory");
    println!(
        "  debug      Headless emulation and platform-native debugger operations; run debug capabilities"
    );
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

#[cfg(test)]
mod verify_fields_tests {
    use super::verify_outcome_fields;

    /// A divergence reaches the JSON as fields, every side's result included,
    /// instead of only inside the `Debug` text of `detail`.
    #[test]
    fn a_divergence_is_reported_field_by_field() {
        let found = fission_dir::Divergence {
            run: Some(3),
            args: vec![1, -2],
            prehir_result: Some(5),
            hir_result: Some(6),
            emulator_result: Some(5),
            detail: Some("return value".to_string()),
        };
        let fields = verify_outcome_fields(&fission_dir::VerifyOutcome::Diverged(vec![found]));
        let cx = &fields["counterexamples"][0];
        assert_eq!(cx["run"], 3);
        assert_eq!(cx["args"], serde_json::json!([1, -2]));
        assert_eq!(cx["hir_result"], 6);
        assert_eq!(cx["emulator_result"], 5);
        assert_eq!(cx["detail"], "return value");

        let fields = verify_outcome_fields(&fission_dir::VerifyOutcome::Equivalent { checked: 9 });
        assert_eq!(fields["checked"], 9);
        assert!(fields.get("counterexamples").is_none());
    }
}
