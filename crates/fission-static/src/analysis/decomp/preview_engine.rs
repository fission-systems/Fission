use fission_loader::loader::LoadedBinary;
use fission_pcode::{
    MlilPreviewOptions, PcodeFunction, PcodeOpcode, PcodeOptimizer, PcodeOptimizerConfig,
    PreviewCallParamRule, PreviewTypeContext, render_mlil_preview_with_context,
};
use fission_signatures::WIN_API_DB;
use fission_signatures::win_types::WindowsStructures;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewEngineMode {
    Legacy,
    MlilPreview,
    Auto,
}

impl PreviewEngineMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            PreviewEngineMode::Legacy => "legacy",
            PreviewEngineMode::MlilPreview => "mlil_preview",
            PreviewEngineMode::Auto => "auto",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewSelection {
    pub preview_code: Option<String>,
    pub engine_used: PreviewEngineMode,
    pub fell_back: bool,
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PreviewWorkerRequest {
    pub pcode_json: String,
    pub address: u64,
    pub name: String,
    pub options: MlilPreviewOptions,
    pub type_context: PreviewTypeContext,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PreviewWorkerResponse {
    pub success: bool,
    pub code: Option<String>,
    pub error: Option<String>,
}

const PREVIEW_WORKER_BIN_NAME: &str = "fission_preview_worker";
const PREVIEW_WORKER_TIMEOUT_CAP_MS: u64 = 10_000;
const PREVIEW_WORKER_TIMEOUT_MARGIN_MS: u64 = 1_000;
const PREVIEW_WORKER_MIN_TIMEOUT_MS: u64 = 1_000;

fn is_type_failure_for_preview_rescue(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.contains("duplicate variablepiece")
        || lower.contains("ptrsub")
        || lower.contains("non structured pointer type")
        || lower.contains("struct")
}

pub trait PreviewSource {
    fn get_pcode_json(&mut self, address: u64) -> fission_core::Result<String>;
}

impl PreviewSource for fission_ffi::DecompilerNative {
    fn get_pcode_json(&mut self, address: u64) -> fission_core::Result<String> {
        self.get_pcode(address)
    }
}

impl PreviewSource for crate::analysis::decomp::CachingDecompiler {
    fn get_pcode_json(&mut self, address: u64) -> fission_core::Result<String> {
        self.inner_mut().get_pcode(address)
    }
}

fn pcode_total_ops(pcode: &PcodeFunction) -> usize {
    pcode.blocks.iter().map(|block| block.ops.len()).sum()
}

fn max_multiequal_fanin(pcode: &PcodeFunction) -> usize {
    pcode
        .blocks
        .iter()
        .flat_map(|block| block.ops.iter())
        .filter(|op| op.opcode == PcodeOpcode::MultiEqual)
        .map(|op| op.inputs.len())
        .max()
        .unwrap_or(0)
}

fn contains_indirect_control_flow(pcode: &PcodeFunction) -> bool {
    pcode
        .blocks
        .iter()
        .flat_map(|block| block.ops.iter())
        .any(|op| matches!(op.opcode, PcodeOpcode::CallInd | PcodeOpcode::BranchInd))
}

fn preview_diag_stage(address: u64, stage: &str, start: Instant) {
    if std::env::var_os("FISSION_PREVIEW_DIAG").is_some() {
        eprintln!(
            "[PREVIEW-DIAG] fn=0x{address:x} stage={stage} elapsed_ms={:.1}",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
}

pub fn auto_mlil_eligible(binary: &LoadedBinary, pcode: &PcodeFunction) -> bool {
    binary.is_64bit
        && binary.format.to_ascii_uppercase().starts_with("PE")
        && pcode.blocks.len() <= 12
        && pcode_total_ops(pcode) <= 600
        && !contains_indirect_control_flow(pcode)
        && max_multiequal_fanin(pcode) <= 4
}

fn sanitize_preview_symbol_name(name: &str) -> String {
    let mut sanitized = name.trim().to_string();
    if let Some((_, tail)) = sanitized.rsplit_once('!') {
        sanitized = tail.trim().to_string();
    }
    if let Some(stripped) = sanitized.strip_prefix("__imp_") {
        sanitized = stripped.trim().to_string();
    }
    for suffix in [" [import]", " [export]"] {
        if let Some(stripped) = sanitized.strip_suffix(suffix) {
            sanitized = stripped.trim_end().to_string();
        }
    }
    sanitized
}

fn build_preview_type_context(binary: &LoadedBinary) -> PreviewTypeContext {
    let structures = WindowsStructures::new();
    let mut call_targets = HashMap::new();
    for func in &binary.functions {
        if func.address == 0 || func.name.is_empty() {
            continue;
        }
        call_targets
            .entry(func.address)
            .or_insert_with(|| sanitize_preview_symbol_name(&func.name));
    }
    for (addr, name) in &binary.inner().iat_symbols {
        if *addr == 0 || name.is_empty() {
            continue;
        }
        call_targets
            .entry(*addr)
            .or_insert_with(|| sanitize_preview_symbol_name(name));
    }
    for (addr, name) in &binary.inner().global_symbols {
        if *addr == 0 || name.is_empty() {
            continue;
        }
        call_targets
            .entry(*addr)
            .or_insert_with(|| sanitize_preview_symbol_name(name));
    }

    let mut call_param_rules = Vec::new();
    for sig in WIN_API_DB.iter() {
        for (arg_index, param) in sig.params.iter().enumerate() {
            let Some(struct_name) = resolve_preview_struct_name(&param.type_name, &structures)
            else {
                continue;
            };
            let Some(struct_def) = structures.get(&struct_name) else {
                continue;
            };
            if struct_def.size_64 == 0 {
                continue;
            }
            call_param_rules.push(PreviewCallParamRule {
                callee_name: sig.name.clone(),
                arg_index,
                pointer_alias: param.type_name.clone(),
                pointee_alias: struct_name,
                pointer_size: 8,
                pointee_sizes: vec![struct_def.size_64 as u32],
            });
        }
    }

    PreviewTypeContext {
        call_targets,
        call_param_rules,
    }
}

fn preview_worker_timeout_ms(timeout_ms: Option<u64>) -> u64 {
    let configured = timeout_ms.unwrap_or_else(|| {
        fission_core::config::Config::default()
            .decompiler
            .timeout_ms
    });
    configured
        .saturating_sub(PREVIEW_WORKER_TIMEOUT_MARGIN_MS)
        .clamp(PREVIEW_WORKER_MIN_TIMEOUT_MS, PREVIEW_WORKER_TIMEOUT_CAP_MS)
}

fn should_use_preview_worker(
    binary: &LoadedBinary,
    pcode: &PcodeFunction,
    enforce_auto_gate: bool,
) -> bool {
    if enforce_auto_gate {
        return false;
    }
    binary.is_64bit
        && binary.format.to_ascii_uppercase().starts_with("PE")
        && !auto_mlil_eligible(binary, pcode)
}

fn resolve_preview_worker_path() -> Option<std::path::PathBuf> {
    if let Ok(path) = std::env::var("FISSION_PREVIEW_WORKER") {
        let path = std::path::PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let candidate = dir.join(format!(
        "{PREVIEW_WORKER_BIN_NAME}{}",
        std::env::consts::EXE_SUFFIX
    ));
    candidate.is_file().then_some(candidate)
}

fn preview_diag_event(address: u64, stage: &str, detail: impl AsRef<str>) {
    if std::env::var_os("FISSION_PREVIEW_DIAG").is_some() {
        eprintln!(
            "[PREVIEW-DIAG] fn=0x{address:x} stage={stage} {}",
            detail.as_ref()
        );
    }
}

fn execute_preview_worker_request(
    request: &PreviewWorkerRequest,
    timeout_ms: u64,
) -> Result<String, String> {
    let Some(worker_path) = resolve_preview_worker_path() else {
        return Err("preview worker unavailable".to_string());
    };

    preview_diag_event(
        request.address,
        "worker_spawn",
        format!("path={}", worker_path.display()),
    );

    let mut child = Command::new(&worker_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("mlil-preview worker spawn failed: {e}"))?;

    let request_json = serde_json::to_vec(request)
        .map_err(|e| format!("mlil-preview worker request serialization failed: {e}"))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "mlil-preview worker stdin unavailable".to_string())?;
    stdin
        .write_all(&request_json)
        .map_err(|e| format!("mlil-preview worker stdin write failed: {e}"))?;
    drop(stdin);

    let start = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("mlil-preview worker wait failed: {e}"))?
        {
            preview_diag_event(
                request.address,
                "worker_exit",
                format!(
                    "status={status} elapsed_ms={:.1}",
                    start.elapsed().as_secs_f64() * 1000.0
                ),
            );
            break;
        }
        if start.elapsed() >= Duration::from_millis(timeout_ms) {
            preview_diag_event(
                request.address,
                "worker_timeout",
                format!("budget_ms={timeout_ms}"),
            );
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "preview_timeout: mlil-preview worker timed out after {timeout_ms}ms"
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }

    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_string(&mut stdout)
            .map_err(|e| format!("mlil-preview worker stdout read failed: {e}"))?;
    }

    let response: PreviewWorkerResponse = serde_json::from_str(&stdout)
        .map_err(|e| format!("mlil-preview worker response parse failed: {e}"))?;

    if response.success {
        response
            .code
            .ok_or_else(|| "mlil-preview worker returned success without code".to_string())
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "mlil-preview worker failed without error".to_string()))
    }
}

fn resolve_preview_struct_name(type_name: &str, structures: &WindowsStructures) -> Option<String> {
    if type_name.contains('*') {
        return None;
    }
    for prefix in ["LP", "P"] {
        let Some(candidate) = type_name.strip_prefix(prefix) else {
            continue;
        };
        if structures.get(candidate).is_some() {
            return Some(candidate.to_string());
        }
    }
    None
}

fn render_preview_request(request: &PreviewWorkerRequest) -> Result<String, String> {
    let parse_start = Instant::now();
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let _ = std::fs::write(
            format!("/tmp/fission_preview_{:x}.json", request.address),
            &request.pcode_json,
        );
    }
    let mut pcode = PcodeFunction::from_json(&request.pcode_json)
        .map_err(|e| format!("mlil-preview pcode parse failed: {e}"))?;
    preview_diag_stage(request.address, "parse_pcode_done", parse_start);
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let mut debug_dump = String::new();
        debug_dump.push_str(&format!(
            "[mlil-preview] function=0x{:x} blocks={} ops={}\n",
            request.address,
            pcode.blocks.len(),
            pcode.blocks.iter().map(|b| b.ops.len()).sum::<usize>()
        ));
        eprintln!(
            "[mlil-preview] function=0x{:x} blocks={} ops={}",
            request.address,
            pcode.blocks.len(),
            pcode.blocks.iter().map(|b| b.ops.len()).sum::<usize>()
        );
        for block in &pcode.blocks {
            let term = block
                .ops
                .last()
                .map(|op| format!("{:?}@0x{:x}", op.opcode, op.address))
                .unwrap_or_else(|| "<none>".to_string());
            debug_dump.push_str(&format!(
                "[mlil-preview] block 0x{:x} ops={} term={}\n",
                block.start_address,
                block.ops.len(),
                term
            ));
            eprintln!(
                "[mlil-preview] block 0x{:x} ops={} term={}",
                block.start_address,
                block.ops.len(),
                term
            );
        }
        let _ = std::fs::write(
            format!("/tmp/fission_preview_{:x}.log", request.address),
            debug_dump,
        );
    }
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("/tmp/fission_preview_{:x}.log", request.address))
            .and_then(|mut f| {
                std::io::Write::write_all(&mut f, b"[mlil-preview] stage=before_optimize\n")
            });
    }
    let mut optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());
    let optimize_start = Instant::now();
    let optimize_result = catch_unwind(AssertUnwindSafe(|| optimizer.optimize(&mut pcode)));
    preview_diag_stage(request.address, "optimize_pcode_done", optimize_start);
    if optimize_result.is_err() && std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("/tmp/fission_preview_{:x}.log", request.address))
            .and_then(|mut f| {
                std::io::Write::write_all(
                    &mut f,
                    b"[mlil-preview] stage=optimize_panicked_using_raw_pcode\n",
                )
            });
    }
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("/tmp/fission_preview_{:x}.log", request.address))
            .and_then(|mut f| {
                std::io::Write::write_all(&mut f, b"[mlil-preview] stage=after_optimize\n")
            });
    }
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("/tmp/fission_preview_{:x}.log", request.address))
            .and_then(|mut f| {
                std::io::Write::write_all(&mut f, b"[mlil-preview] stage=before_render\n")
            });
    }
    let render_start = Instant::now();
    match render_mlil_preview_with_context(
        &pcode,
        &request.name,
        request.address,
        &request.options,
        Some(&request.type_context),
    ) {
        Ok(code) => {
            preview_diag_stage(request.address, "render_preview_done", render_start);
            if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(format!("/tmp/fission_preview_{:x}.log", request.address))
                    .and_then(|mut f| {
                        std::io::Write::write_all(&mut f, b"[mlil-preview] stage=render_ok\n")
                    });
            }
            Ok(code)
        }
        Err(err) => {
            preview_diag_stage(request.address, "render_preview_error", render_start);
            if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(format!("/tmp/fission_preview_{:x}.log", request.address))
                    .and_then(|mut f| {
                        std::io::Write::write_all(
                            &mut f,
                            format!("[mlil-preview] stage=render_error err={err}\n").as_bytes(),
                        )
                    });
            }
            Err(format!("mlil-preview unavailable: {err}"))
        }
    }
}

pub fn execute_preview_worker(request: &PreviewWorkerRequest) -> PreviewWorkerResponse {
    match render_preview_request(request) {
        Ok(code) => PreviewWorkerResponse {
            success: true,
            code: Some(code),
            error: None,
        },
        Err(error) => PreviewWorkerResponse {
            success: false,
            code: None,
            error: Some(error),
        },
    }
}

fn render_preview_from_json(
    pcode_json: &str,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    enforce_auto_gate: bool,
    timeout_ms: Option<u64>,
) -> Result<Option<String>, String> {
    let parse_start = Instant::now();
    let pcode = PcodeFunction::from_json(pcode_json)
        .map_err(|e| format!("mlil-preview pcode parse failed: {e}"))?;
    preview_diag_stage(address, "parse_pcode_done", parse_start);
    if enforce_auto_gate && !auto_mlil_eligible(binary, &pcode) {
        return Ok(None);
    }

    let request = PreviewWorkerRequest {
        pcode_json: pcode_json.to_string(),
        address,
        name: name.to_string(),
        options: MlilPreviewOptions::from_loaded_binary(binary),
        type_context: build_preview_type_context(binary),
    };

    if should_use_preview_worker(binary, &pcode, enforce_auto_gate) {
        let worker_timeout_ms = preview_worker_timeout_ms(timeout_ms);
        match execute_preview_worker_request(&request, worker_timeout_ms) {
            Ok(code) => {
                preview_diag_event(
                    address,
                    "worker_render_done",
                    format!("budget_ms={worker_timeout_ms}"),
                );
                return Ok(Some(code));
            }
            Err(err) if err == "preview worker unavailable" => {
                preview_diag_event(address, "worker_unavailable", "falling back to in-process");
            }
            Err(err) => return Err(err),
        }
    }

    match render_preview_request(&request) {
        Ok(code) => Ok(Some(code)),
        Err(err) => Err(err),
    }
}

fn classify_preview_failure(reason: &str) -> &'static str {
    let lower = reason.to_ascii_lowercase();
    if lower.contains("preview_timeout") || lower.contains("worker timed out") {
        "preview_timeout"
    } else if lower.contains("unsupported architecture") || lower.contains("supports pe x64 only") {
        "unsupported_arch"
    } else if lower.contains("unsupported branch target") {
        "unsupported_cfg_branch_target"
    } else if lower.contains("unsupported region shape") {
        "unsupported_cfg_region_shape"
    } else if lower.contains("unsupported phi join") {
        "unsupported_cfg_phi_join"
    } else if lower.contains("unsupported indirect call region") {
        "unsupported_cfg_indirect_call_region"
    } else if lower.contains("unsupported control flow") {
        "unsupported_cfg"
    } else if lower.contains("multiequal") {
        "unsupported_expr_multiequal"
    } else if lower.contains("unsupported address materialization") {
        "unsupported_expr_address_materialization"
    } else if lower.contains("unsupported indirect value source") {
        "unsupported_expr_indirect_value_source"
    } else if lower.contains("unsupported piece/subpiece shape") {
        "unsupported_expr_piece_shape"
    } else if lower.contains("unsupported ptr arithmetic shape") {
        "unsupported_expr_ptr_arithmetic"
    } else if lower.contains("unsupported memory-backed varnode") {
        "unsupported_expr_memory_backed_varnode"
    } else if lower.contains("value lowering failed on varnode") {
        "unsupported_expr_varnode_lowering"
    } else if lower.contains("loop") || lower.contains("dowhile") || lower.contains("while") {
        "unsupported_loop_shape"
    } else if lower.contains("switch") {
        "unsupported_switch_shape"
    } else if lower.contains("ptr") || lower.contains("load") || lower.contains("store") {
        "unsupported_memory_pattern"
    } else if lower.contains("multiequal") || lower.contains("phi") {
        "unsupported_phi_merge"
    } else if lower.contains("call") {
        "unsupported_call_boundary"
    } else {
        "unsupported_expr"
    }
}

fn classified_preview_error(reason: &str) -> String {
    format!("{}: {}", classify_preview_failure(reason), reason)
}

pub fn select_preview_output<S: PreviewSource>(
    source: &mut S,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    mode: PreviewEngineMode,
    timeout_ms: Option<u64>,
) -> Result<PreviewSelection, String> {
    let diag = std::env::var_os("FISSION_PREVIEW_DIAG").is_some();
    match mode {
        PreviewEngineMode::Legacy => Ok(PreviewSelection {
            preview_code: None,
            engine_used: PreviewEngineMode::Legacy,
            fell_back: false,
            fallback_reason: None,
        }),
        PreviewEngineMode::MlilPreview => {
            let pcode_start = Instant::now();
            if diag {
                eprintln!("[PREVIEW-DIAG] get_pcode start: fn=0x{address:x} mode=mlil_preview");
            }
            let pcode_json = source.get_pcode_json(address).map_err(|e| e.to_string())?;
            if diag {
                eprintln!(
                    "[PREVIEW-DIAG] get_pcode done: fn=0x{address:x} mode=mlil_preview elapsed_ms={:.1}",
                    pcode_start.elapsed().as_secs_f64() * 1000.0
                );
            }
            match render_preview_from_json(&pcode_json, binary, address, name, false, timeout_ms) {
                Ok(Some(code)) => Ok(PreviewSelection {
                    preview_code: Some(code),
                    engine_used: PreviewEngineMode::MlilPreview,
                    fell_back: false,
                    fallback_reason: None,
                }),
                Ok(None) => Ok(PreviewSelection {
                    preview_code: None,
                    engine_used: PreviewEngineMode::Legacy,
                    fell_back: true,
                    fallback_reason: Some(classified_preview_error(
                        "mlil-preview skipped: function not supported by preview builder",
                    )),
                }),
                Err(err) => Ok(PreviewSelection {
                    preview_code: None,
                    engine_used: PreviewEngineMode::Legacy,
                    fell_back: true,
                    fallback_reason: Some(classified_preview_error(&err)),
                }),
            }
        }
        PreviewEngineMode::Auto => {
            let pcode_start = Instant::now();
            if diag {
                eprintln!("[PREVIEW-DIAG] get_pcode start: fn=0x{address:x} mode=auto");
            }
            let pcode_json = source.get_pcode_json(address).map_err(|e| e.to_string())?;
            if diag {
                eprintln!(
                    "[PREVIEW-DIAG] get_pcode done: fn=0x{address:x} mode=auto elapsed_ms={:.1}",
                    pcode_start.elapsed().as_secs_f64() * 1000.0
                );
            }
            match render_preview_from_json(&pcode_json, binary, address, name, true, timeout_ms) {
                Ok(Some(code)) => Ok(PreviewSelection {
                    preview_code: Some(code),
                    engine_used: PreviewEngineMode::MlilPreview,
                    fell_back: false,
                    fallback_reason: None,
                }),
                Ok(None) => Ok(PreviewSelection {
                    preview_code: None,
                    engine_used: PreviewEngineMode::Legacy,
                    fell_back: false,
                    fallback_reason: None,
                }),
                Err(err) => Ok(PreviewSelection {
                    preview_code: None,
                    engine_used: PreviewEngineMode::Legacy,
                    fell_back: true,
                    fallback_reason: Some(classified_preview_error(&err)),
                }),
            }
        }
    }
}

pub fn rescue_preview_output<S: PreviewSource>(
    source: &mut S,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    error: &str,
    timeout_ms: Option<u64>,
) -> Result<Option<PreviewSelection>, String> {
    if !is_type_failure_for_preview_rescue(error) {
        return Ok(None);
    }

    let diag = std::env::var_os("FISSION_PREVIEW_DIAG").is_some();
    let pcode_start = Instant::now();
    if diag {
        eprintln!("[PREVIEW-DIAG] get_pcode start: fn=0x{address:x} mode=rescue");
    }
    let pcode_json = source.get_pcode_json(address).map_err(|e| e.to_string())?;
    if diag {
        eprintln!(
            "[PREVIEW-DIAG] get_pcode done: fn=0x{address:x} mode=rescue elapsed_ms={:.1}",
            pcode_start.elapsed().as_secs_f64() * 1000.0
        );
    }
    match render_preview_from_json(&pcode_json, binary, address, name, false, timeout_ms) {
        Ok(Some(code)) => Ok(Some(PreviewSelection {
            preview_code: Some(code),
            engine_used: PreviewEngineMode::MlilPreview,
            fell_back: true,
            fallback_reason: Some(format!(
                "legacy type failure rescued by mlil-preview: {error}"
            )),
        })),
        Ok(None) => Ok(None),
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_worker_request_roundtrip() {
        let request = PreviewWorkerRequest {
            pcode_json: "{\"blocks\":[]}".to_string(),
            address: 0x1234,
            name: "sub_1234".to_string(),
            options: MlilPreviewOptions {
                pe_x64_only: true,
                is_64bit: true,
                pointer_size: 8,
                format: "PE".to_string(),
                image_base: 0x140000000,
                sections: vec![(0x140001000, 0x140002000)],
            },
            type_context: PreviewTypeContext {
                call_targets: HashMap::from([(0x140001234, "MessageBoxW".to_string())]),
                call_param_rules: vec![PreviewCallParamRule {
                    callee_name: "MessageBoxW".to_string(),
                    arg_index: 1,
                    pointer_alias: "LPCWSTR".to_string(),
                    pointee_alias: "WCHAR".to_string(),
                    pointer_size: 8,
                    pointee_sizes: vec![2],
                }],
            },
        };

        let encoded = serde_json::to_string(&request).expect("serialize worker request");
        let decoded: PreviewWorkerRequest =
            serde_json::from_str(&encoded).expect("deserialize worker request");

        assert_eq!(decoded.address, request.address);
        assert_eq!(decoded.name, request.name);
        assert_eq!(decoded.options, request.options);
        assert_eq!(decoded.type_context, request.type_context);
    }

    #[test]
    fn preview_worker_timeout_clamps() {
        assert_eq!(
            preview_worker_timeout_ms(Some(500)),
            PREVIEW_WORKER_MIN_TIMEOUT_MS
        );
        assert_eq!(
            preview_worker_timeout_ms(Some(30_000)),
            PREVIEW_WORKER_TIMEOUT_CAP_MS
        );
    }
}
