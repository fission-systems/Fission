use crate::cli::args::OneShotArgs;
use crate::cli::oneshot::disasm::render_function_disassembly_text;
use fission_core::FissionError;
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_pcode::{NirRenderOptions, PcodeFunction, Varnode};
use std::cmp::min;
use std::fs;
use std::io::{self, Write};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

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

fn sleigh_language_for_arch_spec(arch_spec: &str) -> Option<&'static str> {
    if arch_spec.starts_with("AARCH64:LE:64") && arch_spec.contains("AppleSilicon") {
        return Some("AARCH64_AppleSilicon");
    }
    if arch_spec.starts_with("AARCH64:LE:64") {
        return Some("AARCH64");
    }
    if arch_spec.starts_with("AARCH64:BE:64") {
        return Some("AARCH64BE");
    }
    if arch_spec.starts_with("x86:LE:64") {
        return Some("x86-64");
    }
    if arch_spec.starts_with("x86:LE:32") || arch_spec.starts_with("x86:LE:16") {
        return Some("x86");
    }
    None
}

fn decode_rust_sleigh_pcode(
    lifter: &fission_sleigh::lifter::SleighLifter,
    binary: &LoadedBinary,
    func: &FunctionInfo,
) -> Result<PcodeFunction, FissionError> {
    let max_bytes = if func.size > 0 {
        usize::try_from(func.size)
            .unwrap_or(0x400)
            .min(0x1000)
            .max(1)
    } else {
        0x100
    };

    let bytes = binary.view_bytes(func.address, max_bytes).ok_or_else(|| {
        FissionError::decompiler(format!(
            "rust_sleigh: unable to read bytes at 0x{:x}",
            func.address
        ))
    })?;

    let lifted = lifter
        .lift_raw_pcode_function_with_contract(&bytes, func.address, 512)
        .map_err(|err| {
            FissionError::decompiler(format!(
                "rust_sleigh: function lift failed for {} at 0x{:x}: {:#}",
                func.name, func.address, err
            ))
        })?;

    Ok(lifted.function)
}

fn format_varnode_for_pcode(vn: &Varnode) -> String {
    if vn.is_constant {
        format!("const(0x{:x}:{})", vn.constant_val as u64, vn.size)
    } else {
        format!("v(space={},off=0x{:x},size={})", vn.space_id, vn.offset, vn.size)
    }
}

fn render_pcode_text(func: &FunctionInfo, pcode: &PcodeFunction) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "// rust_sleigh direct pcode output: {}\n",
        func.name
    ));
    for block in &pcode.blocks {
        out.push_str(&format!("block_{} @ 0x{:x}\n", block.index, block.start_address));
        for op in &block.ops {
            let out_vn = op
                .output
                .as_ref()
                .map(format_varnode_for_pcode)
                .unwrap_or_else(|| "-".to_string());
            let in_vn = op
                .inputs
                .iter()
                .map(format_varnode_for_pcode)
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "  [{:04}] 0x{:x} {:?}  {} <- {}\n",
                op.seq_num, op.address, op.opcode, out_vn, in_vn
            ));
        }
    }
    out
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
) -> Result<(String, bool, Option<String>, Option<fission_pcode::NirBuildStats>, Option<fission_pcode::NirHintStats>), FissionError> {
    let language = sleigh_language_for_arch_spec(&binary.arch_spec).ok_or_else(|| {
        FissionError::decompiler(format!(
            "rust_sleigh: unsupported arch_spec '{}'",
            binary.arch_spec
        ))
    })?;

    let lifter = fission_sleigh::lifter::SleighLifter::new_for_language(language)
        .map_err(|e| FissionError::decompiler(format!("rust_sleigh: {e:#}")))?;

    let pcode = decode_rust_sleigh_pcode(&lifter, binary, func)?;

    let mut options = NirRenderOptions::from_loaded_binary(binary);
    options.pe_x64_only = false;
    options.conservative_irreducible_fallback = true;

    let selection = fission_decompiler_core::select_nir_output_from_prebuilt_pcode(
        &pcode,
        binary,
        func.address,
        &func.name,
        fission_decompiler_core::NirEngineMode::Nir,
        None,
        options,
    )
    .map_err(|e| FissionError::decompiler(format!("rust_sleigh routing failed: {e}")))?;

    if let Some(code) = selection.nir_code {
        return Ok((
            code,
            selection.fell_back,
            selection.fallback_reason,
            selection.build_stats,
            selection.hint_stats,
        ));
    }

    let fallback_reason = selection
        .fallback_reason
        .unwrap_or_else(|| "nir skipped: function not supported by Fission NIR builder".to_string());
    let lower = fallback_reason.to_ascii_lowercase();
    let is_unsupported_arch = lower.contains("unsupported architecture in mlil-preview")
        || matches!(selection.fallback_kind_refined, Some("preview_architecture_unsupported"));
    if is_unsupported_arch {
        return Ok((
            render_pcode_text(func, &pcode),
            true,
            Some("nir_unsupported_arch:pcode_dump".to_string()),
            None,
            None,
        ));
    }

    Err(FissionError::decompiler(format!(
        "rust_sleigh render failed: {}",
        fallback_reason
    )))
}

fn collect_target_functions(cli: &OneShotArgs, binary: &LoadedBinary) -> Vec<FunctionInfo> {
    if let Some(addr) = cli.address {
        if let Some(func) = binary.function_at(addr) {
            return vec![func.clone()];
        }
        return Vec::new();
    }

    if cli.decomp_all {
        let mut functions: Vec<FunctionInfo> = binary.functions.clone();
        functions.sort_by_key(|f| f.address);
        if let Some(limit) = cli.decomp_limit {
            functions.truncate(limit);
        }
        return functions;
    }

    Vec::new()
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
    let functions = collect_target_functions(cli, binary);
    if functions.is_empty() && cli.address.is_some() {
        let addr = cli.address.unwrap_or_default();
        let synthetic = vec![FunctionInfo {
            name: format!("sub_{:x}", addr),
            address: addr,
            size: 0,
            is_export: false,
            is_import: false,
        }];
        return run_with_functions(cli, binary, &synthetic, init_start);
    }
    run_with_functions(cli, binary, &functions, init_start)
}

fn run_with_functions(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    functions: &[FunctionInfo],
    init_start: std::time::Instant,
) -> io::Result<()> {
    let effective_no_header = cli.no_header || cli.ghidra_compat;
    let effective_json = cli.json || cli.benchmark;
    let config = RenderConfig {
        benchmark: cli.benchmark,
        ghidra_compat: cli.ghidra_compat,
        effective_no_warnings: cli.no_warnings || cli.ghidra_compat,
    };

    let use_worker_fanout = cli.decomp_all && functions.len() > 1;
    let mut results = if use_worker_fanout {
        let workers = resolve_worker_count(functions.len());
        if cli.verbose {
            eprintln!(
                "[*] Rust-only decomp-all worker fan-out/fan-in: workers={}, functions={}",
                workers,
                functions.len()
            );
        }
        run_worker_fanout_fanin(Arc::new(binary.clone()), functions, config, workers)
    } else {
        functions
            .iter()
            .map(|func| render_one_function(binary, func, config))
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
        let envelope = serde_json::json!({
            "_meta": {
                "tool": "fission",
                "version": env!("CARGO_PKG_VERSION"),
                "profile": cli.profile.as_deref().unwrap_or("balanced"),
                "engine": "rust-sleigh",
                "function_count": results.len(),
                "init_sec": 0.0,
                "total_decomp_sec": (total_decomp_secs * 1_000_000.0).round() / 1_000_000.0,
                "total_postprocess_sec": (total_postprocess_secs * 1_000_000.0).round() / 1_000_000.0,
                "wall_clock_sec": (init_start.elapsed().as_secs_f64() * 1_000_000.0).round() / 1_000_000.0,
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

fn run_worker_fanout_fanin(
    binary: Arc<LoadedBinary>,
    functions: &[FunctionInfo],
    config: RenderConfig,
    worker_count: usize,
) -> Vec<FunctionRenderResult> {
    let (task_tx, task_rx) = mpsc::channel::<FunctionInfo>();
    let task_rx = Arc::new(Mutex::new(task_rx));
    let (result_tx, result_rx) = mpsc::channel::<FunctionRenderResult>();

    let mut worker_handles = Vec::with_capacity(worker_count);
    for _ in 0..worker_count {
        let rx = Arc::clone(&task_rx);
        let tx = result_tx.clone();
        let binary = Arc::clone(&binary);
        worker_handles.push(thread::spawn(move || loop {
            let task = match rx.lock() {
                Ok(locked) => locked.recv(),
                Err(_) => return,
            };
            let func = match task {
                Ok(func) => func,
                Err(_) => return,
            };
            let rendered = render_one_function(binary.as_ref(), &func, config);
            if tx.send(rendered).is_err() {
                return;
            }
        }));
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

            if let Some(fallback) = make_assembly_fallback(
                binary,
                binary.inner().data.as_slice(),
                func,
                &error_text,
            ) {
                let mut entry = serde_json::json!({
                    "address": format!("0x{:x}", func.address),
                    "name": func.name,
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
