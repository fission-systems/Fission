use crate::analysis::decomp::FactStore;
use fission_loader::loader::LoadedBinary;
use fission_pcode::{
    MlilPreviewOptions, PcodeFunction, PcodeOpcode, PcodeOptimizer, PcodeOptimizerConfig,
    PreviewBuildStats, PreviewTypeContext, render_mlil_preview_with_context,
    take_last_preview_build_stats,
};
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
    pub build_stats: Option<PreviewBuildStats>,
    pub engine_used: PreviewEngineMode,
    pub fell_back: bool,
    pub fallback_reason: Option<String>,
    pub fallback_kind: Option<&'static str>,
    pub preview_surface: Option<PreviewSurfaceKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewRoutingDecision {
    pub engine_used: PreviewEngineMode,
    pub fell_back: bool,
    pub fallback_reason: Option<String>,
    pub fallback_kind: Option<&'static str>,
    pub preview_surface: Option<PreviewSurfaceKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewSurfaceKind {
    Structured,
    Unstructured,
}

pub struct PreviewRoutingResolver;

impl PreviewRoutingResolver {
    pub fn from_selection(selection: &PreviewSelection) -> PreviewRoutingDecision {
        PreviewRoutingDecision {
            engine_used: selection.engine_used,
            fell_back: selection.fell_back,
            fallback_reason: selection.fallback_reason.clone(),
            fallback_kind: selection.fallback_kind,
            preview_surface: selection.preview_surface,
        }
    }

    pub fn legacy_mode() -> PreviewSelection {
        PreviewSelection {
            preview_code: None,
            build_stats: None,
            engine_used: PreviewEngineMode::Legacy,
            fell_back: false,
            fallback_reason: None,
            fallback_kind: None,
            preview_surface: None,
        }
    }

    pub fn preview_success(
        code: String,
        build_stats: Option<PreviewBuildStats>,
        fell_back: bool,
        fallback_reason: Option<String>,
    ) -> PreviewSelection {
        PreviewSelection {
            preview_surface: Some(classify_preview_surface(&code)),
            preview_code: Some(code),
            build_stats,
            engine_used: PreviewEngineMode::MlilPreview,
            fell_back,
            fallback_kind: extract_fallback_kind(fallback_reason.as_deref()),
            fallback_reason,
        }
    }

    pub fn preview_fallback(reason: impl AsRef<str>) -> PreviewSelection {
        let fallback_reason = classified_preview_error(reason.as_ref());
        PreviewSelection {
            preview_code: None,
            build_stats: None,
            engine_used: PreviewEngineMode::Legacy,
            fell_back: true,
            fallback_kind: extract_fallback_kind(Some(fallback_reason.as_str())),
            fallback_reason: Some(fallback_reason),
            preview_surface: None,
        }
    }

    pub fn native_failure(error: &str) -> PreviewRoutingDecision {
        let kind = classify_native_failure_kind(error);
        PreviewRoutingDecision {
            engine_used: PreviewEngineMode::Legacy,
            fell_back: true,
            fallback_reason: Some(fallback_reason_with_kind(kind, error)),
            fallback_kind: Some(kind),
            preview_surface: None,
        }
    }
}

impl PreviewSelection {
    pub fn routing_decision(&self) -> PreviewRoutingDecision {
        PreviewRoutingResolver::from_selection(self)
    }
}

fn extract_fallback_kind(reason: Option<&str>) -> Option<&'static str> {
    let reason = reason?;
    let prefix = reason.split(':').next()?.trim().to_ascii_lowercase();
    match prefix.as_str() {
        "preview_timeout" => Some("preview_timeout"),
        "preview_unsupported" => Some("preview_unsupported"),
        "native_pcode_failure" => Some("native_pcode_failure"),
        "legacy_fallback" => Some("legacy_fallback"),
        "assembly_fallback" => Some("assembly_fallback"),
        _ => None,
    }
}

fn classify_preview_surface(code: &str) -> PreviewSurfaceKind {
    let has_goto = code.contains("goto ");
    let has_label = code.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.ends_with(':') && !trimmed.starts_with("case ") && !trimmed.starts_with("default:")
    });
    if has_goto || has_label {
        PreviewSurfaceKind::Unstructured
    } else {
        PreviewSurfaceKind::Structured
    }
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
    pub build_stats: Option<PreviewBuildStats>,
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

#[cfg(test)]
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

fn build_preview_type_context_from_facts(
    binary: &LoadedBinary,
    fact_store: &FactStore,
    address: u64,
) -> PreviewTypeContext {
    crate::analysis::decomp::preview_context::build_preview_type_context(
        binary, fact_store, address,
    )
}

fn make_preview_request(
    pcode_json: &str,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    type_context: PreviewTypeContext,
) -> PreviewWorkerRequest {
    PreviewWorkerRequest {
        pcode_json: pcode_json.to_string(),
        address,
        name: name.to_string(),
        options: MlilPreviewOptions::from_loaded_binary(binary),
        type_context,
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
) -> Result<(String, Option<PreviewBuildStats>), String> {
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
    let exit_status = loop {
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
            break status;
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
    };

    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_string(&mut stdout)
            .map_err(|e| format!("mlil-preview worker stdout read failed: {e}"))?;
    }

    if stdout.trim().is_empty() {
        return Err(format!(
            "mlil-preview worker exited with status {exit_status} without JSON response"
        ));
    }

    let response: PreviewWorkerResponse = serde_json::from_str(&stdout)
        .map_err(|e| format!("mlil-preview worker response parse failed: {e}"))?;

    if response.success {
        let PreviewWorkerResponse {
            code, build_stats, ..
        } = response;
        code.map(|code| (code, build_stats))
            .ok_or_else(|| "mlil-preview worker returned success without code".to_string())
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "mlil-preview worker failed without error".to_string()))
    }
}

fn render_preview_request(
    request: &PreviewWorkerRequest,
) -> Result<(String, Option<PreviewBuildStats>), String> {
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
            let build_stats = take_last_preview_build_stats();
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
            Ok((code, build_stats))
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
        Ok((code, build_stats)) => PreviewWorkerResponse {
            success: true,
            code: Some(code),
            build_stats,
            error: None,
        },
        Err(error) => PreviewWorkerResponse {
            success: false,
            code: None,
            build_stats: None,
            error: Some(error),
        },
    }
}

fn render_preview_from_json_with_type_context(
    pcode_json: &str,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    enforce_auto_gate: bool,
    timeout_ms: Option<u64>,
    type_context: PreviewTypeContext,
) -> Result<Option<(String, Option<PreviewBuildStats>)>, String> {
    let parse_start = Instant::now();
    let pcode = PcodeFunction::from_json(pcode_json)
        .map_err(|e| format!("mlil-preview pcode parse failed: {e}"))?;
    preview_diag_stage(address, "parse_pcode_done", parse_start);
    if enforce_auto_gate && !auto_mlil_eligible(binary, &pcode) {
        return Ok(None);
    }

    let request = make_preview_request(pcode_json, binary, address, name, type_context);

    if should_use_preview_worker(binary, &pcode, enforce_auto_gate) {
        let worker_timeout_ms = preview_worker_timeout_ms(timeout_ms);
        match execute_preview_worker_request(&request, worker_timeout_ms) {
            Ok((code, build_stats)) => {
                preview_diag_event(
                    address,
                    "worker_render_done",
                    format!("budget_ms={worker_timeout_ms}"),
                );
                return Ok(Some((code, build_stats)));
            }
            Err(err) if err == "preview worker unavailable" => {
                preview_diag_event(address, "worker_unavailable", "falling back to in-process");
            }
            Err(err) => return Err(err),
        }
    }

    match render_preview_request(&request) {
        Ok(result) => Ok(Some(result)),
        Err(err) => Err(err),
    }
}

fn classify_preview_failure(reason: &str) -> &'static str {
    let lower = reason.to_ascii_lowercase();
    if lower.contains("preview_timeout") || lower.contains("worker timed out") {
        "preview_timeout"
    } else {
        "preview_unsupported"
    }
}

fn classified_preview_error(reason: &str) -> String {
    fallback_reason_with_kind(classify_preview_failure(reason), reason)
}

pub fn fallback_reason_with_kind(kind: &str, detail: impl AsRef<str>) -> String {
    format!("{kind}: {}", detail.as_ref())
}

pub fn classify_native_failure_kind(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("preview_timeout") {
        "preview_timeout"
    } else if lower.contains("could not find op at target address")
        || lower.contains("ghidra lowlevelerror")
    {
        "native_pcode_failure"
    } else {
        "legacy_fallback"
    }
}

pub fn native_failure_routing_decision(error: &str) -> PreviewRoutingDecision {
    PreviewRoutingResolver::native_failure(error)
}

pub fn select_preview_output<S: PreviewSource>(
    source: &mut S,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    mode: PreviewEngineMode,
    timeout_ms: Option<u64>,
) -> Result<PreviewSelection, String> {
    let fact_store = FactStore::from_binary(binary);
    select_preview_output_with_facts(source, binary, &fact_store, address, name, mode, timeout_ms)
}

pub fn select_preview_output_with_facts<S: PreviewSource>(
    source: &mut S,
    binary: &LoadedBinary,
    fact_store: &FactStore,
    address: u64,
    name: &str,
    mode: PreviewEngineMode,
    timeout_ms: Option<u64>,
) -> Result<PreviewSelection, String> {
    let diag = std::env::var_os("FISSION_PREVIEW_DIAG").is_some();
    match mode {
        PreviewEngineMode::Legacy => Ok(PreviewRoutingResolver::legacy_mode()),
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
            let type_context = build_preview_type_context_from_facts(binary, fact_store, address);
            match render_preview_from_json_with_type_context(
                &pcode_json,
                binary,
                address,
                name,
                false,
                timeout_ms,
                type_context,
            ) {
                Ok(Some((code, build_stats))) => Ok(PreviewRoutingResolver::preview_success(
                    code,
                    build_stats,
                    false,
                    None,
                )),
                Ok(None) => Ok(PreviewRoutingResolver::preview_fallback(
                    "mlil-preview skipped: function not supported by preview builder",
                )),
                Err(err) => Ok(PreviewRoutingResolver::preview_fallback(&err)),
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
            let type_context = build_preview_type_context_from_facts(binary, fact_store, address);
            match render_preview_from_json_with_type_context(
                &pcode_json,
                binary,
                address,
                name,
                true,
                timeout_ms,
                type_context,
            ) {
                Ok(Some((code, build_stats))) => Ok(PreviewRoutingResolver::preview_success(
                    code,
                    build_stats,
                    false,
                    None,
                )),
                Ok(None) => Ok(PreviewRoutingResolver::preview_fallback(
                    "mlil-preview skipped: function not supported by preview builder",
                )),
                Err(err) => Ok(PreviewRoutingResolver::preview_fallback(&err)),
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
    let fact_store = FactStore::from_binary(binary);
    rescue_preview_output_with_facts(
        source,
        binary,
        &fact_store,
        address,
        name,
        error,
        timeout_ms,
    )
}

pub fn rescue_preview_output_with_facts<S: PreviewSource>(
    source: &mut S,
    binary: &LoadedBinary,
    fact_store: &FactStore,
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
    let type_context = build_preview_type_context_from_facts(binary, fact_store, address);
    match render_preview_from_json_with_type_context(
        &pcode_json,
        binary,
        address,
        name,
        false,
        timeout_ms,
        type_context,
    ) {
        Ok(Some((code, build_stats))) => Ok(Some(PreviewRoutingResolver::preview_success(
            code,
            build_stats,
            true,
            Some(format!(
                "legacy_fallback: legacy type failure rescued by mlil-preview: {error}"
            )),
        ))),
        Ok(None) => Ok(None),
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_core::common::types::FunctionInfo;
    use fission_loader::loader::types::{
        DwarfFunctionInfo, DwarfLocalVar, DwarfLocation, DwarfParamInfo,
    };
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};
    use fission_pcode::PreviewCallParamRule;
    use std::collections::HashMap;

    struct MockPreviewSource;

    impl PreviewSource for MockPreviewSource {
        fn get_pcode_json(&mut self, _address: u64) -> fission_core::Result<String> {
            Ok("{\"blocks\":[]}".to_string())
        }
    }

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
                function_hints: None,
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

    #[test]
    fn native_failure_routing_uses_taxonomy() {
        let decision = native_failure_routing_decision("Could not find op at target address");
        assert_eq!(decision.engine_used, PreviewEngineMode::Legacy);
        assert!(decision.fell_back);
        assert_eq!(
            decision.fallback_reason.as_deref(),
            Some("native_pcode_failure: Could not find op at target address")
        );
    }

    #[test]
    fn preview_selection_exposes_routing_decision() {
        let selection = PreviewSelection {
            preview_code: None,
            build_stats: None,
            engine_used: PreviewEngineMode::Legacy,
            fell_back: true,
            fallback_reason: Some("preview_timeout: worker timed out".to_string()),
            fallback_kind: Some("preview_timeout"),
            preview_surface: None,
        };
        let decision = selection.routing_decision();
        assert_eq!(decision.engine_used, PreviewEngineMode::Legacy);
        assert!(decision.fell_back);
        assert_eq!(decision.fallback_kind, Some("preview_timeout"));
        assert_eq!(
            decision.fallback_reason.as_deref(),
            Some("preview_timeout: worker timed out")
        );
    }

    #[test]
    fn preview_success_classifies_unstructured_surface() {
        let selection = PreviewRoutingResolver::preview_success(
            "label_1:\n  goto label_1;".to_string(),
            None,
            false,
            None,
        );
        assert_eq!(
            selection.preview_surface,
            Some(PreviewSurfaceKind::Unstructured)
        );
        assert_eq!(
            selection.routing_decision().preview_surface,
            Some(PreviewSurfaceKind::Unstructured)
        );
    }

    #[test]
    fn fact_store_names_drive_preview_call_targets() {
        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
            .format("PE")
            .is_64bit(true)
            .add_function(FunctionInfo {
                name: "sub_401000".to_string(),
                address: 0x401000,
                size: 0,
                is_export: false,
                is_import: false,
            })
            .build()
            .expect("build test binary");
        let mut facts = FactStore::from_binary(&binary);
        facts.ingest_name_fact(
            0x401000,
            "RenamedTarget".to_string(),
            crate::analysis::decomp::FactProvenance::StrongFid,
        );

        let context = build_preview_type_context_from_facts(&binary, &facts, 0x401000);
        assert_eq!(
            context.call_targets.get(&0x401000).map(String::as_str),
            Some("RenamedTarget")
        );
    }

    #[test]
    fn preview_context_builder_preserves_call_param_rules() {
        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
            .format("PE")
            .is_64bit(true)
            .build()
            .expect("build test binary");
        let facts = FactStore::from_binary(&binary);
        let context = build_preview_type_context_from_facts(&binary, &facts, 0);

        assert!(context.call_param_rules.iter().any(|rule| {
            rule.callee_name == "GetWindowRect"
                && !rule.pointer_alias.is_empty()
                && !rule.pointee_alias.is_empty()
                && !rule.pointee_sizes.is_empty()
        }));
    }

    #[test]
    fn make_preview_request_reuses_external_type_context() {
        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
            .format("PE")
            .is_64bit(true)
            .build()
            .expect("build test binary");
        let type_context = PreviewTypeContext {
            call_targets: HashMap::from([(0x401000, "KnownName".to_string())]),
            call_param_rules: vec![PreviewCallParamRule {
                callee_name: "MessageBoxW".to_string(),
                arg_index: 1,
                pointer_alias: "LPCWSTR".to_string(),
                pointee_alias: "WCHAR".to_string(),
                pointer_size: 8,
                pointee_sizes: vec![2],
            }],
            function_hints: None,
        };

        let request = make_preview_request("{}", &binary, 0x401000, "sub_401000", type_context);
        assert_eq!(
            request
                .type_context
                .call_targets
                .get(&0x401000)
                .map(String::as_str),
            Some("KnownName")
        );
    }

    #[test]
    fn sanitize_preview_symbol_name_strips_import_prefixes_and_suffixes() {
        assert_eq!(
            sanitize_preview_symbol_name("__imp_MessageBoxW"),
            "MessageBoxW"
        );
        assert_eq!(sanitize_preview_symbol_name("foo [import]"), "foo");
    }

    #[test]
    fn select_preview_output_wrapper_keeps_legacy_mode_behavior() {
        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
            .format("PE")
            .is_64bit(true)
            .build()
            .expect("build test binary");
        let mut source = MockPreviewSource;

        let selection = select_preview_output(
            &mut source,
            &binary,
            0x401000,
            "sub_401000",
            PreviewEngineMode::Legacy,
            None,
        )
        .expect("legacy preview selection");

        assert_eq!(selection.engine_used, PreviewEngineMode::Legacy);
        assert!(!selection.fell_back);
        assert!(selection.preview_code.is_none());
    }

    #[test]
    fn rescue_preview_output_with_facts_ignores_non_type_failures() {
        let binary = LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
            .format("PE")
            .is_64bit(true)
            .build()
            .expect("build test binary");
        let facts = FactStore::from_binary(&binary);
        let mut source = MockPreviewSource;

        let selection = rescue_preview_output_with_facts(
            &mut source,
            &binary,
            &facts,
            0x401000,
            "sub_401000",
            "some unrelated error",
            None,
        )
        .expect("rescue helper");

        assert!(selection.is_none());
    }

    #[test]
    fn preview_request_carries_function_scoped_hints_from_dwarf_facts() {
        let mut binary =
            LoadedBinaryBuilder::new("sample.exe".to_string(), DataBuffer::Heap(vec![]))
                .format("PE")
                .is_64bit(true)
                .add_function(FunctionInfo {
                    name: "sub_401000".to_string(),
                    address: 0x401000,
                    size: 0,
                    is_export: false,
                    is_import: false,
                })
                .build()
                .expect("build test binary");
        binary.dwarf_functions.insert(
            0x401000,
            DwarfFunctionInfo {
                address: 0x401000,
                name: "KnownName".to_string(),
                return_type: Some("BOOL".to_string()),
                params: vec![DwarfParamInfo {
                    name: "hwnd".to_string(),
                    type_name: "HWND".to_string(),
                    location: DwarfLocation::Register("RCX".to_string()),
                }],
                local_vars: vec![DwarfLocalVar {
                    name: "rect".to_string(),
                    type_name: "RECT".to_string(),
                    location: DwarfLocation::StackOffset(-0x20),
                }],
            },
        );
        let facts = FactStore::from_binary(&binary);
        let type_context = build_preview_type_context_from_facts(&binary, &facts, 0x401000);
        let request = make_preview_request("{}", &binary, 0x401000, "sub_401000", type_context);

        let hints = request
            .type_context
            .function_hints
            .as_ref()
            .expect("function-scoped preview hints");
        assert_eq!(hints.param_names, vec!["hwnd".to_string()]);
        assert_eq!(
            hints.stack_local_names.get(&-0x20).map(String::as_str),
            Some("rect")
        );
        assert_eq!(hints.return_type_name.as_deref(), Some("BOOL"));
    }
}
