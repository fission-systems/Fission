use crate::cli::args::OneShotArgs;
use crate::cli::oneshot::disasm::render_function_disassembly_text;
use crate::cli::oneshot::function_select::{
    BatchFunctionSelection, select_batch_functions, select_explicit_functions,
    select_function_by_address, select_functions_from_addresses_file,
};
use fission_core::FissionError;
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use std::cmp::min;
use std::fs;
use std::io::{self, Write};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

const DEFAULT_DECOMP_STACK_MB: usize = 32;

#[derive(Clone, Copy)]
struct RenderConfig {
    benchmark: bool,
    ghidra_compat: bool,
    effective_no_warnings: bool,
}

struct FunctionRenderResult {
    address: u64,
    decomp_sec: f64,
    postprocess_sec: f64,
    plain_output: String,
    json_entry: serde_json::Value,
}

#[derive(Clone, Copy, Debug)]
struct ProcessCpuSnapshot {
    user_sec: f64,
    system_sec: f64,
}

#[derive(Clone, Copy, Debug)]
struct ProcessCpuDelta {
    user_sec: f64,
    system_sec: f64,
    total_sec: f64,
    utilization_pct: f64,
    effective_parallelism: f64,
}

fn should_use_assembly_fallback(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("preview_timeout")
        || lower.contains("could not find op at target address")
        || lower.contains("unsupported architecture")
        || (lower.contains("decoded") && lower.contains("zero semantic ops"))
}

fn make_assembly_fallback(
    binary: &LoadedBinary,
    binary_data: &[u8],
    func: &FunctionInfo,
    error: &str,
) -> Option<String> {
    if !should_use_assembly_fallback(error) {
        return None;
    }
    let asm = render_function_disassembly_text(binary, binary_data, func.address).ok()?;
    Some(format!(
        "// Assembly fallback: {}\n// Function: {} @ 0x{:x}\n\n{}",
        error, func.name, func.address, asm
    ))
}

fn strip_warnings(code: &str) -> String {
    code.lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with("WARNING:")
                && !trimmed.starts_with("NOTICE:")
                && !trimmed.starts_with("/* WARNING")
                && !trimmed.starts_with("// WARNING")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn strip_inferred_structs(code: &str) -> String {
    let mut result = String::new();
    let mut in_struct_block = false;
    for line in code.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("typedef struct") || trimmed.starts_with("// Inferred Structure") {
            in_struct_block = true;
            continue;
        }
        if in_struct_block {
            if trimmed.starts_with('}') && trimmed.ends_with(';') {
                in_struct_block = false;
                continue;
            }
            continue;
        }
        result.push_str(line);
        result.push('\n');
    }
    result
}

fn render_with_rust_sleigh(
    binary: &LoadedBinary,
    func: &FunctionInfo,
) -> Result<
    (
        String,
        bool,
        Option<String>,
        Option<fission_pcode::NirBuildStats>,
        Option<fission_pcode::NirHintStats>,
    ),
    FissionError,
> {
    let config = fission_decompiler_core::RustSleighDecompileConfig::cli_defaults();
    let result = fission_decompiler_core::decompile_with_rust_sleigh(
        binary,
        func.address,
        &func.name,
        &config,
        None,
        None,
    )
    .map_err(FissionError::decompiler)?;

    Ok((
        result.code,
        result.fell_back,
        result.fallback_reason,
        result.build_stats,
        result.hint_stats,
    ))
}

fn collect_target_functions<'a>(
    cli: &OneShotArgs,
    binary: &'a LoadedBinary,
) -> BatchFunctionSelection<'a> {
    if let Some(addr) = cli.address {
        if let Some(func) =
            select_function_by_address(binary, addr).or_else(|| binary.function_at(addr))
        {
            return select_explicit_functions(vec![func], cli.include_nonuser_functions);
        }
        return select_explicit_functions(vec![], cli.include_nonuser_functions);
    }

    if cli.decomp_all {
        if let Some(address_file) = &cli.addresses_file {
            if let Ok(functions) = select_functions_from_addresses_file(binary, address_file) {
                return select_explicit_functions(functions, cli.include_nonuser_functions);
            }
            return select_explicit_functions(vec![], cli.include_nonuser_functions);
        }
        return select_batch_functions(binary, cli.include_nonuser_functions, cli.decomp_limit);
    }

    select_explicit_functions(vec![], cli.include_nonuser_functions)
}

pub(crate) fn run_decompilation_rust_sleigh(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    _binary_data: &[u8],
) -> io::Result<()> {
    if cli.verbose {
        eprintln!("[*] native_decomp is disabled; using Rust-Sleigh pipeline");
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
    selection_accounting: crate::cli::oneshot::function_select::BatchSelectionAccounting,
    init_start: std::time::Instant,
) -> io::Result<()> {
    let cpu_start = capture_process_cpu_snapshot();
    let effective_no_header = cli.no_header || cli.ghidra_compat;
    let effective_json = cli.json || cli.benchmark;
    let config = RenderConfig {
        benchmark: cli.benchmark,
        ghidra_compat: cli.ghidra_compat,
        effective_no_warnings: cli.no_warnings || cli.ghidra_compat,
    };
    let stack_size_bytes = resolve_decomp_stack_size_bytes();
    let available_parallelism = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let worker_env_requested = std::env::var("FISSION_RUST_DECOMP_WORKERS").ok();

    let use_worker_fanout = cli.decomp_all && functions.len() > 1;
    let worker_count = if use_worker_fanout {
        resolve_worker_count(functions.len())
    } else {
        1
    };
    let mut results = if use_worker_fanout {
        if cli.verbose {
            eprintln!(
                "[*] Rust-only decomp-all worker fan-out/fan-in: workers={}, functions={}, stack_mb={}",
                worker_count,
                functions.len(),
                stack_size_bytes / (1024 * 1024)
            );
        }
        run_worker_fanout_fanin(
            Arc::new(binary.clone()),
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
                render_one_function_on_large_stack(
                    Arc::clone(&binary_arc),
                    func,
                    config,
                    stack_size_bytes,
                )
            })
            .collect::<Vec<_>>()
    };

    // Deterministic merge point for fan-in results.
    results.sort_by_key(|entry| entry.address);

    let mut all_output = String::new();
    let mut json_results = Vec::with_capacity(results.len());
    let total_decomp_secs: f64 = results.iter().map(|entry| entry.decomp_sec).sum();
    let total_postprocess_secs: f64 = results.iter().map(|entry| entry.postprocess_sec).sum();

    for entry in &results {
        if effective_json {
            json_results.push(entry.json_entry.clone());
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
    }

    let final_output = if cli.benchmark {
        let wall_clock_sec = round_six(init_start.elapsed().as_secs_f64());
        let cpu_delta = cpu_start.and_then(|start| {
            capture_process_cpu_snapshot().map(|end| process_cpu_delta(start, end, wall_clock_sec))
        });
        let envelope = serde_json::json!({
            "_meta": {
                "tool": "fission",
                "version": env!("CARGO_PKG_VERSION"),
                "profile": cli.profile.as_deref().unwrap_or("balanced"),
                "engine": "rust-sleigh",
                "function_count": results.len(),
                "worker_count": worker_count,
                "worker_fanout_enabled": use_worker_fanout,
                "available_parallelism": available_parallelism,
                "worker_env_requested": worker_env_requested,
                "decomp_stack_mb": stack_size_bytes / (1024 * 1024),
                "functions_discovered_total": selection_accounting.functions_discovered_total,
                "functions_selected_total": selection_accounting.functions_selected_total,
                "functions_excluded_import_count": selection_accounting.functions_excluded_import_count,
                "functions_excluded_runtime_wrapper_count": selection_accounting.functions_excluded_runtime_wrapper_count,
                "include_nonuser_functions": selection_accounting.include_nonuser_functions,
                "init_sec": 0.0,
                "total_decomp_sec": round_six(total_decomp_secs),
                "total_postprocess_sec": round_six(total_postprocess_secs),
                "wall_clock_sec": wall_clock_sec,
                "cpu_user_sec": cpu_delta.map(|delta| round_six(delta.user_sec)),
                "cpu_system_sec": cpu_delta.map(|delta| round_six(delta.system_sec)),
                "cpu_total_sec": cpu_delta.map(|delta| round_six(delta.total_sec)),
                "cpu_utilization_pct": cpu_delta.map(|delta| round_three(delta.utilization_pct)),
                "effective_parallelism": cpu_delta.map(|delta| round_three(delta.effective_parallelism)),
            },
            "functions": json_results,
        });
        serde_json::to_string_pretty(&envelope)
            .map_err(|e| io::Error::other(format!("JSON serialization failed: {e}")))?
    } else if effective_json {
        serde_json::to_string_pretty(&json_results)
            .map_err(|e| io::Error::other(format!("JSON serialization failed: {e}")))?
    } else {
        all_output
    };

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

fn round_six(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn round_three(value: f64) -> f64 {
    (value * 1_000.0).round() / 1_000.0
}

fn process_cpu_delta(
    start: ProcessCpuSnapshot,
    end: ProcessCpuSnapshot,
    wall_clock_sec: f64,
) -> ProcessCpuDelta {
    let user_sec = (end.user_sec - start.user_sec).max(0.0);
    let system_sec = (end.system_sec - start.system_sec).max(0.0);
    let total_sec = user_sec + system_sec;
    let wall = wall_clock_sec.max(1e-9);
    ProcessCpuDelta {
        user_sec,
        system_sec,
        total_sec,
        utilization_pct: (total_sec / wall) * 100.0,
        effective_parallelism: total_sec / wall,
    }
}

#[cfg(unix)]
fn capture_process_cpu_snapshot() -> Option<ProcessCpuSnapshot> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    let usage = unsafe { usage.assume_init() };
    Some(ProcessCpuSnapshot {
        user_sec: timeval_to_seconds(usage.ru_utime),
        system_sec: timeval_to_seconds(usage.ru_stime),
    })
}

#[cfg(unix)]
fn timeval_to_seconds(value: libc::timeval) -> f64 {
    value.tv_sec as f64 + (value.tv_usec as f64 / 1_000_000.0)
}

#[cfg(not(unix))]
fn capture_process_cpu_snapshot() -> Option<ProcessCpuSnapshot> {
    None
}

fn resolve_worker_count(total_functions: usize) -> usize {
    if total_functions <= 1 {
        return 1;
    }

    if let Ok(value) = std::env::var("FISSION_RUST_DECOMP_WORKERS") {
        if let Ok(parsed) = value.parse::<usize>() {
            return parsed.max(1).min(total_functions);
        }
    }

    let cpu = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    min(total_functions, cpu.clamp(1, 8))
}

fn resolve_decomp_stack_size_bytes() -> usize {
    let mb = std::env::var("FISSION_RUST_DECOMP_STACK_MB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_DECOMP_STACK_MB)
        .clamp(8, 256);
    mb * 1024 * 1024
}

fn make_internal_error_result(
    binary: &LoadedBinary,
    func: &FunctionInfo,
    message: String,
    config: RenderConfig,
) -> FunctionRenderResult {
    let fallback = make_assembly_fallback(binary, binary.inner().data.as_slice(), func, &message);
    let plain = fallback.clone().unwrap_or_else(|| {
        format!(
            "// Error decompiling {} (0x{:x}): {}",
            func.name, func.address, message
        )
    });
    let mut entry = serde_json::json!({
        "address": format!("0x{:x}", func.address),
        "name": func.name,
        "size": func.size,
        "engine_used": "rust_sleigh",
        "fell_back": true,
        "fallback_reason": "rust_sleigh:worker_internal_error",
        "error": message,
    });
    if let Some(code) = fallback {
        entry["code"] = serde_json::json!(code);
        entry["fallback"] = serde_json::json!("assembly");
    } else {
        entry["code"] = serde_json::json!(plain.clone());
    }
    if config.benchmark {
        entry["decomp_sec"] = serde_json::json!(0.0);
        entry["postprocess_sec"] = serde_json::json!(0.0);
    }

    FunctionRenderResult {
        address: func.address,
        decomp_sec: 0.0,
        postprocess_sec: 0.0,
        plain_output: plain,
        json_entry: entry,
    }
}

fn render_one_function_on_large_stack(
    binary: Arc<LoadedBinary>,
    func: &FunctionInfo,
    config: RenderConfig,
    stack_size_bytes: usize,
) -> FunctionRenderResult {
    let func_owned = func.clone();
    let func_for_error = func.clone();
    let binary_for_thread = Arc::clone(&binary);

    let spawn = thread::Builder::new()
        .name(format!("fission-rust-decomp-0x{:x}", func.address))
        .stack_size(stack_size_bytes)
        .spawn(move || render_one_function(binary_for_thread.as_ref(), &func_owned, config));

    match spawn {
        Ok(handle) => match handle.join() {
            Ok(result) => result,
            Err(_) => make_internal_error_result(
                binary.as_ref(),
                &func_for_error,
                "worker thread panicked while rendering function".to_string(),
                config,
            ),
        },
        Err(err) => make_internal_error_result(
            binary.as_ref(),
            &func_for_error,
            format!("failed to spawn render worker: {err}"),
            config,
        ),
    }
}

fn run_worker_fanout_fanin(
    binary: Arc<LoadedBinary>,
    functions: &[FunctionInfo],
    config: RenderConfig,
    worker_count: usize,
    stack_size_bytes: usize,
) -> Vec<FunctionRenderResult> {
    let (task_tx, task_rx) = mpsc::channel::<FunctionInfo>();
    let task_rx = Arc::new(Mutex::new(task_rx));
    let (result_tx, result_rx) = mpsc::channel::<FunctionRenderResult>();

    let mut worker_handles = Vec::with_capacity(worker_count);
    for worker_idx in 0..worker_count {
        let rx = Arc::clone(&task_rx);
        let tx = result_tx.clone();
        let binary = Arc::clone(&binary);
        let spawn = thread::Builder::new()
            .name(format!("fission-rust-decomp-worker-{worker_idx}"))
            .stack_size(stack_size_bytes)
            .spawn(move || {
                loop {
                    let task = match rx.lock() {
                        Ok(locked) => locked.recv(),
                        Err(_) => return,
                    };
                    let func = match task {
                        Ok(func) => func,
                        Err(_) => return,
                    };
                    let rendered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        render_one_function(binary.as_ref(), &func, config)
                    }))
                    .unwrap_or_else(|_| {
                        make_internal_error_result(
                            binary.as_ref(),
                            &func,
                            "worker thread panicked while rendering function".to_string(),
                            config,
                        )
                    });
                    if tx.send(rendered).is_err() {
                        return;
                    }
                }
            });

        if let Ok(handle) = spawn {
            worker_handles.push(handle);
        }
    }
    drop(result_tx);

    for func in functions {
        if task_tx.send(func.clone()).is_err() {
            break;
        }
    }
    drop(task_tx);

    let mut outputs = Vec::with_capacity(functions.len());
    for _ in 0..functions.len() {
        if let Ok(output) = result_rx.recv() {
            outputs.push(output);
        } else {
            break;
        }
    }

    for handle in worker_handles {
        let _ = handle.join();
    }

    outputs
}

fn render_one_function(
    binary: &LoadedBinary,
    func: &FunctionInfo,
    config: RenderConfig,
) -> FunctionRenderResult {
    let start = std::time::Instant::now();

    match render_with_rust_sleigh(binary, func) {
        Ok((mut code, fell_back, fallback_reason, build_stats, hint_stats)) => {
            let decomp_sec = start.elapsed().as_secs_f64();

            if config.effective_no_warnings {
                code = strip_warnings(&code);
            }
            if config.ghidra_compat {
                code = strip_inferred_structs(&code);
            }

            let mut entry = serde_json::json!({
                "address": format!("0x{:x}", func.address),
                "name": func.name,
                "size": func.size,
                "code": code,
                "engine_used": "rust_sleigh",
                "fell_back": fell_back,
                "fallback_reason": fallback_reason,
            });
            if let Some(stats) = build_stats {
                entry["preview_build_stats"] = serde_json::json!(stats);
            }
            if let Some(stats) = hint_stats {
                entry["preview_hint_stats"] = serde_json::json!(stats);
            }
            if config.benchmark {
                entry["decomp_sec"] =
                    serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                entry["postprocess_sec"] = serde_json::json!(0.0);
            }

            FunctionRenderResult {
                address: func.address,
                decomp_sec,
                postprocess_sec: 0.0,
                plain_output: entry
                    .get("code")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                json_entry: entry,
            }
        }
        Err(err) => {
            let decomp_sec = start.elapsed().as_secs_f64();
            let error_text = err.to_string();

            if let Some(fallback) =
                make_assembly_fallback(binary, binary.inner().data.as_slice(), func, &error_text)
            {
                let mut entry = serde_json::json!({
                    "address": format!("0x{:x}", func.address),
                    "name": func.name,
                    "size": func.size,
                    "code": fallback,
                    "engine_used": "rust_sleigh",
                    "fell_back": true,
                    "fallback": "assembly",
                    "fallback_reason": format!("assembly_fallback: {}", error_text),
                });
                if config.benchmark {
                    entry["decomp_sec"] =
                        serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                    entry["postprocess_sec"] = serde_json::json!(0.0);
                }

                FunctionRenderResult {
                    address: func.address,
                    decomp_sec,
                    postprocess_sec: 0.0,
                    plain_output: entry
                        .get("code")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    json_entry: entry,
                }
            } else {
                let plain = format!(
                    "// Error decompiling {} (0x{:x}): {}",
                    func.name, func.address, error_text
                );
                let mut entry = serde_json::json!({
                    "address": format!("0x{:x}", func.address),
                    "name": func.name,
                    "size": func.size,
                    "engine_used": "rust_sleigh",
                    "fell_back": true,
                    "fallback_reason": format!("rust_sleigh: {}", error_text),
                    "error": error_text,
                });
                if config.benchmark {
                    entry["decomp_sec"] =
                        serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                    entry["postprocess_sec"] = serde_json::json!(0.0);
                }

                FunctionRenderResult {
                    address: func.address,
                    decomp_sec,
                    postprocess_sec: 0.0,
                    plain_output: plain,
                    json_entry: entry,
                }
            }
        }
    }
}
