//! Rust-only decompilation CLI path (the only decompiler pipeline Fission ships).

mod debug_bundle;
mod fallback;
mod output;
mod record;
mod selection;
mod serialize;
mod strip;
mod workers;

pub(crate) use selection::collect_target_functions;

use crate::cli::args::OneShotArgs;
use crate::cli::oneshot::function_select::BatchSelectionAccounting;
use debug_bundle::debug_bundle_for_record;
use fission_core::FissionError;
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use output::{benchmark_envelope_json, capture_process_cpu_snapshot, round_six};
use record::{CliRustDecompileRecord, CliRustOutcome, RenderConfig};
use serialize::{record_plain_output, record_to_json};
use std::fs;
use std::io::{self, Write};
use std::sync::Arc;

pub(crate) struct FunctionRenderResult {
    pub address: u64,
    pub decomp_sec: f64,
    pub postprocess_sec: f64,
    pub plain_output: String,
    pub json_entry: serde_json::Value,
    pub debug_bundle: Option<serde_json::Value>,
}

struct RustSleighRender {
    code: String,
    code_nir: Option<String>,
    code_hir: Option<String>,
    fell_back: bool,
    fallback_reason: Option<String>,
    build_stats: Option<fission_decompiler::NirBuildStats>,
    hint_stats: Option<fission_decompiler::NirHintStats>,
    evidence: fission_decompiler::RustSleighPipelineEvidence,
}

fn render_with_rust_sleigh(
    binary: &LoadedBinary,
    facts: &fission_static::analysis::decomp::facts::FactStore,
    func: &FunctionInfo,
    timeout_ms: Option<u64>,
) -> Result<RustSleighRender, FissionError> {
    let mut config = fission_decompiler::RustSleighDecompileConfig::cli_defaults();
    config.nir_timeout_ms = timeout_ms;
    let result = fission_decompiler::decompile_with_rust_sleigh_with_facts(
        binary,
        facts,
        func.address,
        &func.name,
        &config,
        None,
        None,
    )
    .map_err(FissionError::decompiler)?;

    Ok(RustSleighRender {
        code: result.code,
        code_nir: result.code_nir,
        code_hir: result.code_hir,
        fell_back: result.fell_back,
        fallback_reason: result.fallback_reason,
        build_stats: result.build_stats,
        hint_stats: result.hint_stats,
        evidence: result.evidence,
    })
}

fn apply_output_filters(code: &str, config: RenderConfig) -> String {
    let mut filtered = code.to_string();
    if config.effective_no_warnings {
        filtered = strip::strip_warnings(&filtered);
    }
    if config.ghidra_compat {
        filtered = strip::strip_inferred_structs(&filtered);
    }
    filtered
}

fn filter_optional(code: Option<String>, config: RenderConfig) -> Option<String> {
    code.map(|c| apply_output_filters(&c, config))
}

fn record_into_function_render_result(
    record: CliRustDecompileRecord,
    debug_bundle: Option<serde_json::Value>,
    benchmark: bool,
) -> FunctionRenderResult {
    let address = record.func.address;
    let decomp_sec = match &record.outcome {
        CliRustOutcome::Success { decomp_sec, .. } => *decomp_sec,
        CliRustOutcome::AssemblyFallback { decomp_sec, .. } => *decomp_sec,
        CliRustOutcome::HardError { decomp_sec, .. } => *decomp_sec,
        CliRustOutcome::WorkerInternalError { .. } => 0.0,
    };
    let json_entry = record_to_json(&record, benchmark);
    let plain_output = record_plain_output(&record);
    FunctionRenderResult {
        address,
        decomp_sec,
        postprocess_sec: 0.0,
        plain_output,
        json_entry,
        debug_bundle,
    }
}

pub(crate) fn make_internal_error_result(
    binary: &LoadedBinary,
    func: &FunctionInfo,
    message: String,
    config: RenderConfig,
) -> FunctionRenderResult {
    let fallback =
        fallback::make_assembly_fallback(binary, binary.inner().data.as_slice(), func, &message);
    let asm_fallback = fallback.is_some();
    let record = CliRustDecompileRecord {
        func: func.clone(),
        layer: config.layer,
        outcome: CliRustOutcome::WorkerInternalError {
            message,
            assembly_fallback_code: fallback,
        },
    };
    let debug_bundle = debug_bundle_for_record(
        binary,
        func,
        config,
        None,
        None,
        None,
        !asm_fallback,
        asm_fallback,
    );
    record_into_function_render_result(record, debug_bundle, config.benchmark)
}

pub(crate) fn render_one_function_inner(
    binary: &LoadedBinary,
    facts: &fission_static::analysis::decomp::facts::FactStore,
    func: &FunctionInfo,
    config: RenderConfig,
) -> FunctionRenderResult {
    let start = std::time::Instant::now();

    match render_with_rust_sleigh(binary, facts, func, config.timeout_ms) {
        Ok(rendered) => {
            let decomp_sec = start.elapsed().as_secs_f64();
            let code = apply_output_filters(&rendered.code, config);
            // Dual surfaces from one IR build; fall back so JSON shape stays stable.
            let code_nir =
                filter_optional(rendered.code_nir, config).unwrap_or_else(|| code.clone());
            let code_hir =
                filter_optional(rendered.code_hir, config).unwrap_or_else(|| code.clone());

            let record = CliRustDecompileRecord {
                func: func.clone(),
                layer: config.layer,
                outcome: CliRustOutcome::Success {
                    code,
                    code_nir: Some(code_nir),
                    code_hir: Some(code_hir),
                    fell_back: rendered.fell_back,
                    fallback_reason: rendered.fallback_reason,
                    build_stats: rendered.build_stats.clone(),
                    hint_stats: rendered.hint_stats.clone(),
                    decomp_sec,
                },
            };

            let debug_bundle = debug_bundle_for_record(
                binary,
                func,
                config,
                rendered.build_stats.as_ref(),
                rendered.hint_stats.as_ref(),
                Some(&rendered.evidence),
                false,
                rendered.build_stats.is_none() && rendered.fell_back,
            );

            record_into_function_render_result(record, debug_bundle, config.benchmark)
        }
        Err(err) => {
            let decomp_sec = start.elapsed().as_secs_f64();
            let error_text = err.to_string();

            if let Some(fallback_code) = fallback::make_assembly_fallback(
                binary,
                binary.inner().data.as_slice(),
                func,
                &error_text,
            ) {
                let record = CliRustDecompileRecord {
                    func: func.clone(),
                    layer: config.layer,
                    outcome: CliRustOutcome::AssemblyFallback {
                        fallback_code,
                        original_error: error_text.clone(),
                        decomp_sec,
                    },
                };

                let debug_bundle =
                    debug_bundle_for_record(binary, func, config, None, None, None, false, true);

                record_into_function_render_result(record, debug_bundle, config.benchmark)
            } else {
                let record = CliRustDecompileRecord {
                    func: func.clone(),
                    layer: config.layer,
                    outcome: CliRustOutcome::HardError {
                        error_text,
                        decomp_sec,
                    },
                };

                let debug_bundle =
                    debug_bundle_for_record(binary, func, config, None, None, None, true, false);

                record_into_function_render_result(record, debug_bundle, config.benchmark)
            }
        }
    }
}

pub(crate) fn run_decompilation_rust_sleigh(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    _binary_data: &[u8],
) -> io::Result<()> {
    if cli.verbose {
        eprintln!("[*] Using Rust-Sleigh pipeline");
    }

    let init_start = std::time::Instant::now();
    let selected_functions = collect_target_functions(cli, binary);
    let selection_accounting = selected_functions.accounting;
    let functions = selected_functions
        .functions
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    if functions.is_empty() && cli.address.is_some() {
        let addr = cli.address.unwrap_or_default();
        let synthetic = vec![FunctionInfo {
            name: format!("sub_{:x}", addr),
            address: addr,
            size: 0,
            is_export: false,
            is_import: false,
            ..Default::default()
        }];
        return run_with_functions(cli, binary, &synthetic, selection_accounting, init_start);
    }
    run_with_functions(cli, binary, &functions, selection_accounting, init_start)
}

fn run_with_functions(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    functions: &[FunctionInfo],
    selection_accounting: BatchSelectionAccounting,
    init_start: std::time::Instant,
) -> io::Result<()> {
    let cpu_start = capture_process_cpu_snapshot();
    let effective_no_header = cli.no_header || cli.ghidra_compat;
    let effective_json = cli.json || cli.benchmark;
    let layer = cli
        .layer
        .as_deref()
        .and_then(fission_decompiler::PseudocodeLayer::parse)
        .unwrap_or(fission_decompiler::PseudocodeLayer::Nir);
    let config = RenderConfig {
        benchmark: cli.benchmark,
        ghidra_compat: cli.ghidra_compat,
        effective_no_warnings: cli.no_warnings || cli.ghidra_compat,
        debug_decomp: cli.debug_decomp,
        debug_decomp_bundle: cli.debug_decomp_bundle.is_some(),
        requested_address: cli.address,
        timeout_ms: cli.timeout_ms,
        layer,
    };
    let stack_size_bytes = workers::resolve_decomp_stack_size_bytes();
    let available_parallelism = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let worker_env_requested = std::env::var("FISSION_RUST_DECOMP_WORKERS").ok();

    let use_worker_fanout = cli.decomp_all && functions.len() > 1;
    let worker_count = if use_worker_fanout {
        workers::resolve_worker_count(functions.len())
    } else {
        1
    };
    // Built once per binary and shared across every function: FactStore
    // construction runs FID signature matching against every function in
    // the binary, so rebuilding it per function (as `decompile_with_rust_
    // sleigh`'s convenience wrapper does) turned a `--all` batch of N
    // functions into N redundant whole-binary analyses.
    let facts = Arc::new(fission_static::analysis::decomp::facts::FactStore::from_binary(binary));
    let mut results = if use_worker_fanout {
        if cli.verbose {
            eprintln!(
                "[*] Rust-only decomp-all worker fan-out/fan-in: workers={}, functions={}, stack_mb={}",
                worker_count,
                functions.len(),
                stack_size_bytes / (1024 * 1024)
            );
        }
        workers::run_worker_fanout_fanin(
            Arc::new(binary.clone()),
            Arc::clone(&facts),
            functions,
            config,
            worker_count,
            stack_size_bytes,
        )
    } else {
        let binary_arc = Arc::new(binary.clone());
        functions
            .iter()
            .map(|func| {
                workers::render_one_function_on_large_stack(
                    Arc::clone(&binary_arc),
                    Arc::clone(&facts),
                    func,
                    config,
                    stack_size_bytes,
                )
            })
            .collect::<Vec<_>>()
    };

    results.sort_by_key(|entry| entry.address);

    let mut all_output = String::new();
    let mut json_results = Vec::with_capacity(results.len());
    let mut debug_bundle_rows = cli.debug_decomp_bundle.as_ref().map(|_| Vec::new());
    let total_decomp_secs: f64 = results.iter().map(|entry| entry.decomp_sec).sum();
    let total_postprocess_secs: f64 = results.iter().map(|entry| entry.postprocess_sec).sum();

    for entry in &results {
        if effective_json {
            let mut je = entry.json_entry.clone();
            if cli.debug_decomp {
                if let Some(ref b) = entry.debug_bundle {
                    je["debug_decomp"] = b.clone();
                }
            }
            json_results.push(je);
        } else {
            if !effective_no_header {
                let name = entry
                    .json_entry
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("unknown");
                all_output.push_str("// ============================================\n");
                all_output.push_str(&format!("// Function: {} @ 0x{:x}\n", name, entry.address));
                all_output.push_str("// ============================================\n\n");
            }
            all_output.push_str(&entry.plain_output);
            all_output.push_str("\n\n");
        }
        if let Some(rows) = debug_bundle_rows.as_mut() {
            if let Some(ref b) = entry.debug_bundle {
                rows.push(b.clone());
            }
        }
    }

    let wall_clock_sec = round_six(init_start.elapsed().as_secs_f64());

    let final_output = if cli.benchmark {
        let envelope = benchmark_envelope_json(
            cli,
            json_results,
            results.len(),
            worker_count,
            use_worker_fanout,
            available_parallelism,
            worker_env_requested,
            stack_size_bytes,
            &selection_accounting,
            total_decomp_secs,
            total_postprocess_secs,
            wall_clock_sec,
            cpu_start,
        );
        serde_json::to_string_pretty(&envelope)
            .map_err(|e| io::Error::other(format!("JSON serialization failed: {e}")))?
    } else if effective_json {
        serde_json::to_string_pretty(&json_results)
            .map_err(|e| io::Error::other(format!("JSON serialization failed: {e}")))?
    } else {
        all_output
    };

    if let Some(ref path) = cli.debug_decomp_bundle {
        if let Some(rows) = debug_bundle_rows.as_ref() {
            crate::cli::oneshot::debug_decomp::write_debug_decomp_bundle_file(path, rows)?;
        }
    }

    if let Some(ref output_path) = cli.output {
        fs::write(output_path, final_output.as_bytes())?;
        if cli.verbose {
            eprintln!("[✓] Output written to: {}", output_path.display());
        }
    } else {
        let mut stdout = io::stdout().lock();
        stdout.write_all(final_output.as_bytes())?;
    }

    Ok(())
}
