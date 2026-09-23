//! Rust-only decompilation CLI path (the only decompiler pipeline Fission ships).

mod debug_bundle;
mod fallback;
mod output;
mod record;
mod selection;
mod serialize;
mod strip;
mod unit;
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
    /// The flattened, goto/label-based PreHIR snapshot structuring received
    /// as input. It is only captured when requested (`decomp --prehir`).
    code_prehir: Option<String>,
    fell_back: bool,
    fallback_reason: Option<String>,
    build_stats: Option<fission_decompiler::NirBuildStats>,
    hint_stats: Option<fission_decompiler::NirHintStats>,
    evidence: fission_decompiler::RustSleighPipelineEvidence,
    /// The variables this decompilation recovered, for a consumer that wants
    /// them as data rather than as printed declarations.
    variables: Vec<fission_decompiler::RecoveredVariable>,
}

fn render_with_rust_sleigh(
    binary: &LoadedBinary,
    facts: &fission_static::analysis::decomp::facts::FactStore,
    func: &FunctionInfo,
    timeout_ms: Option<u64>,
    want_prehir: bool,
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

    // Consume the typed artifacts carried by this exact decompile result. The
    // older thread-local accessors remain only for compatibility with callers
    // that have not migrated yet.
    let code_prehir = if want_prehir {
        result
            .render_output
            .as_ref()
            .and_then(|output| output.prehir.as_ref())
            .map(fission_decompiler::print_prehir_function)
    } else {
        None
    };
    let variables = result
        .render_output
        .as_ref()
        .and_then(|output| output.recovered_variables.clone())
        .unwrap_or_default();

    Ok(RustSleighRender {
        code: result.code,
        code_nir: result.code_nir,
        code_hir: result.code_hir,
        code_prehir,
        fell_back: result.fell_back,
        fallback_reason: result.fallback_reason,
        build_stats: result.build_stats,
        hint_stats: result.hint_stats,
        evidence: result.evidence,
        variables,
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

fn shared_fact_store(
    binary: &LoadedBinary,
    scan_signature_matches: bool,
    seed_project_call_arities: bool,
) -> fission_static::analysis::decomp::facts::FactStore {
    let mut facts = if scan_signature_matches {
        fission_static::analysis::decomp::facts::FactStore::from_binary(binary)
    } else {
        fission_static::analysis::decomp::facts::FactStore::from_binary_without_signature_matches(
            binary,
        )
    };

    if seed_project_call_arities {
        fission_decompiler::facts::seed_whole_program_call_arity_facts(binary, &mut facts);
    }

    facts
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
    // `FunctionInfo::name` may legitimately be empty (stripped binary, no
    // symbol) -- unlike the empty name, an empty rendered signature
    // (`uint (void) { ... }`) isn't valid C and can't be addressed by
    // identifier downstream. Synthesize the same `sub_{addr:x}` fallback
    // every other render path already applies, so every render path
    // agrees on one name for an unnamed function.
    let named_func;
    let func: &FunctionInfo = if func.name.trim().is_empty() {
        named_func = FunctionInfo {
            name: format!("sub_{:x}", func.address),
            ..func.clone()
        };
        &named_func
    } else {
        func
    };

    match render_with_rust_sleigh(binary, facts, func, config.timeout_ms, config.prehir) {
        Ok(rendered) => {
            let decomp_sec = start.elapsed().as_secs_f64();
            let code = apply_output_filters(&rendered.code, config);
            // Dual surfaces from one IR build; fall back so JSON shape stays stable.
            let code_nir =
                filter_optional(rendered.code_nir, config).unwrap_or_else(|| code.clone());
            let code_hir =
                filter_optional(rendered.code_hir, config).unwrap_or_else(|| code.clone());
            // No fallback for DIR -- absent means "not requested" or "not
            // captured" (e.g. decompile fell back before structuring ran),
            // not "same as the default layer".
            let code_prehir = filter_optional(rendered.code_prehir, config);

            let record = CliRustDecompileRecord {
                func: func.clone(),
                layer: config.layer,
                outcome: CliRustOutcome::Success {
                    code,
                    code_nir: Some(code_nir),
                    code_hir: Some(code_hir),
                    code_prehir,
                    fell_back: rendered.fell_back,
                    fallback_reason: rendered.fallback_reason,
                    build_stats: rendered.build_stats.clone(),
                    hint_stats: rendered.hint_stats.clone(),
                    decomp_sec,
                    variables: rendered.variables,
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
    let functions = selected_functions.functions;
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
        timeout_ms: workers::resolve_render_timeout_ms(cli.timeout_ms),
        layer,
        prehir: cli.prehir,
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
    // A project unit renders functions independently (and may do so in
    // parallel), so call-site arity must be known before workers start. The
    // existing whole-program analysis is intentionally limited to project
    // assembly; ordinary one-function and batch renders keep their current
    // cost profile.
    let facts = Arc::new(shared_fact_store(
        binary,
        cli.decomp_all || functions.len() > 1,
        cli.project && functions.len() > 1,
    ));
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

    let mut project_renders: Vec<String> = Vec::new();
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
            if cli.project {
                project_renders.push(entry.plain_output.clone());
            } else {
                all_output.push_str(&entry.plain_output);
                all_output.push_str("\n\n");
            }
        }
        if let Some(rows) = debug_bundle_rows.as_mut() {
            if let Some(ref b) = entry.debug_bundle {
                rows.push(b.clone());
            }
        }
    }

    if cli.project {
        all_output = unit::assemble(&project_renders);
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

#[cfg(test)]
mod project_call_arity_tests {
    use super::shared_fact_store;
    use fission_loader::loader::{
        BinaryLoadSpec, DataBuffer, FunctionInfo, LoadedBinaryBuilder, SectionInfo,
    };

    fn function(name: &str, address: u64, size: u64) -> FunctionInfo {
        FunctionInfo {
            name: name.to_string(),
            address,
            size,
            is_export: false,
            is_import: false,
            ..Default::default()
        }
    }

    fn binary_with_three_argument_internal_call(
        callee_name: &str,
    ) -> fission_loader::loader::LoadedBinary {
        let mut code = vec![0x90; 0x21];
        code[..21].copy_from_slice(&[
            0xbf, 1, 0, 0, 0, // mov edi, 1
            0xbe, 2, 0, 0, 0, // mov esi, 2
            0xba, 3, 0, 0, 0, // mov edx, 3
            0xe8, 0x0c, 0, 0, 0,    // call 0x1020
            0xc3, // ret
        ]);
        code[0x20] = 0xc3;

        LoadedBinaryBuilder::new(
            "synthetic-call-arity.elf".to_string(),
            DataBuffer::Heap(code),
        )
        .format("ELF")
        .load_spec(BinaryLoadSpec::new(
            "ELF",
            0x1000,
            "x86:LE:64:default",
            "gcc",
            "synthetic-test",
        ))
        .entry_point(0x1000)
        .image_base(0x1000)
        .is_64bit(true)
        .add_section(SectionInfo {
            name: ".text".to_string(),
            virtual_address: 0x1000,
            virtual_size: 0x21,
            file_offset: 0,
            file_size: 0x21,
            is_executable: true,
            is_readable: true,
            is_writable: false,
        })
        .add_functions([
            function("caller", 0x1000, 0x15),
            function(callee_name, 0x1020, 1),
        ])
        .build()
        .expect("synthetic x86-64 binary builds")
    }

    #[test]
    fn project_fact_store_seeds_arity_before_parallel_function_renders() {
        let binary = binary_with_three_argument_internal_call("target_fn");
        let facts = shared_fact_store(&binary, false, true);

        let hints = facts
            .structuring_hints(0x1020)
            .expect("resolved internal call seeds a callee hint");
        assert_eq!(hints.param_names, ["param_1", "param_2", "param_3"]);

        let target = binary
            .function_at_exact(0x1020)
            .expect("synthetic callee exists");
        let rendered = super::render_with_rust_sleigh(&binary, &facts, target, None, false)
            .expect("synthetic callee decompiles with the seeded facts");
        let signature = rendered
            .code
            .lines()
            .find(|line| line.contains("target_fn(") && !line.starts_with("extern "))
            .expect("rendered callee signature exists");
        assert!(
            (1..=3).all(|index| signature.contains(&format!("param_{index}"))),
            "the project render should include all three call-site parameters: {signature}"
        );
    }

    #[test]
    fn non_project_fact_store_does_not_run_whole_program_arity_scan() {
        let binary = binary_with_three_argument_internal_call("target_fn");
        let facts = shared_fact_store(&binary, false, false);

        assert!(facts.structuring_hints(0x1020).is_none());
    }

    #[test]
    fn project_main_call_keeps_runtime_prototype() {
        let binary = binary_with_three_argument_internal_call("main");
        let facts = shared_fact_store(&binary, false, true);

        let hints = facts
            .structuring_hints(0x1020)
            .expect("observed runtime call seeds the C entry prototype");
        assert_eq!(hints.param_names, ["argc", "argv", "envp"]);
        assert_eq!(hints.param_type_names[&0], "int");
        assert_eq!(hints.param_type_names[&1], "char **");
        assert_eq!(hints.param_type_names[&2], "char **");
        assert_eq!(hints.return_type_name.as_deref(), Some("int"));

        let entry = binary
            .function_at_exact(0x1020)
            .expect("synthetic C entry exists");
        let rendered_entry = super::render_with_rust_sleigh(&binary, &facts, entry, None, false)
            .expect("entry decompiles with the recovered runtime prototype");
        let signature = rendered_entry
            .code
            .lines()
            .find(|line| line.contains("main(") && !line.starts_with("extern "))
            .expect("rendered entry signature exists");
        assert!(signature.contains("int argc"), "{signature}");
        assert!(signature.contains("char **"), "{signature}");
        assert!(signature.contains("envp"), "{signature}");

        let caller = binary
            .function_at_exact(0x1000)
            .expect("synthetic caller exists");
        let rendered_caller = super::render_with_rust_sleigh(&binary, &facts, caller, None, false)
            .expect("startup call site decompiles");
        assert!(
            rendered_caller.code.contains("main("),
            "{}",
            rendered_caller.code
        );
        assert!(
            rendered_caller.code.contains("(void *)"),
            "typed pointer parameters should convert scalar-shaped ABI actuals at the call only. code:\n{}\nNIR:\n{}\nHIR:\n{}",
            rendered_caller.code,
            rendered_caller.code_nir.as_deref().unwrap_or("<none>"),
            rendered_caller.code_hir.as_deref().unwrap_or("<none>")
        );

        let project =
            super::unit::assemble(&[rendered_caller.code.clone(), rendered_entry.code.clone()]);
        assert!(
            project.contains("int main(int argc, char ** argv, char ** envp);"),
            "the project prototype should come from the typed definition:\n{project}"
        );
        assert!(
            !project.contains("extern unsigned long long main();"),
            "the stale inferred declaration must not shadow the project definition:\n{project}"
        );
    }
}
