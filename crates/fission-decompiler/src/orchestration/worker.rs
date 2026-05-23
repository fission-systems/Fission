use crate::render::{nir_diag_event, render_nir_request};
use crate::types::{NirWorkerRequest, NirWorkerResponse};
use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const NIR_WORKER_BIN_NAME: &str = "fission_nir_worker";
const LEGACY_PREVIEW_WORKER_BIN_NAME: &str = "fission_preview_worker";
const NIR_WORKER_TIMEOUT_CAP_MS: u64 = 10_000;
const NIR_WORKER_TIMEOUT_MARGIN_MS: u64 = 1_000;
const NIR_WORKER_MIN_TIMEOUT_MS: u64 = 1_000;

pub(crate) fn nir_worker_timeout_ms(timeout_ms: Option<u64>) -> u64 {
    let configured = timeout_ms.unwrap_or_else(|| {
        fission_core::config::Config::default()
            .decompiler
            .timeout_ms
    });
    configured
        .saturating_sub(NIR_WORKER_TIMEOUT_MARGIN_MS)
        .clamp(NIR_WORKER_MIN_TIMEOUT_MS, NIR_WORKER_TIMEOUT_CAP_MS)
}

fn resolve_nir_worker_path() -> Option<std::path::PathBuf> {
    if let Ok(path) = std::env::var("FISSION_NIR_WORKER") {
        let path = std::path::PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(path) = std::env::var("FISSION_PREVIEW_WORKER") {
        let path = std::path::PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let nir_candidate = dir.join(format!(
        "{NIR_WORKER_BIN_NAME}{}",
        std::env::consts::EXE_SUFFIX
    ));
    if nir_candidate.is_file() {
        return Some(nir_candidate);
    }
    let compat_candidate = dir.join(format!(
        "{LEGACY_PREVIEW_WORKER_BIN_NAME}{}",
        std::env::consts::EXE_SUFFIX
    ));
    compat_candidate.is_file().then_some(compat_candidate)
}

pub(crate) fn execute_nir_worker_request(
    request: &NirWorkerRequest,
    timeout_ms: u64,
) -> Result<
    (
        String,
        Option<crate::NirBuildStats>,
        Option<crate::NirHintStats>,
    ),
    String,
> {
    let Some(worker_path) = resolve_nir_worker_path() else {
        return Err("nir worker unavailable".to_string());
    };

    nir_diag_event(
        request.address,
        "worker_spawn",
        format!("path={}", worker_path.display()),
    );

    let mut child = Command::new(&worker_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("Fission NIR worker spawn failed: {e}"))?;

    let request_json = serde_json::to_vec(request)
        .map_err(|e| format!("Fission NIR worker request serialization failed: {e}"))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Fission NIR worker stdin unavailable".to_string())?;
    stdin
        .write_all(&request_json)
        .map_err(|e| format!("Fission NIR worker stdin write failed: {e}"))?;
    drop(stdin);

    let start = Instant::now();
    let exit_status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("Fission NIR worker wait failed: {e}"))?
        {
            nir_diag_event(
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
            nir_diag_event(
                request.address,
                "worker_timeout",
                format!("budget_ms={timeout_ms}"),
            );
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "nir_timeout: Fission NIR worker timed out after {timeout_ms}ms"
            ));
        }
        thread::sleep(Duration::from_millis(10));
    };

    let mut stdout = String::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_string(&mut stdout)
            .map_err(|e| format!("Fission NIR worker stdout read failed: {e}"))?;
    }

    if stdout.trim().is_empty() {
        return Err(format!(
            "Fission NIR worker exited with status {exit_status} without JSON response"
        ));
    }

    let response: NirWorkerResponse = serde_json::from_str(&stdout)
        .map_err(|e| format!("Fission NIR worker response parse failed: {e}"))?;

    if response.success {
        let NirWorkerResponse {
            code,
            build_stats,
            hint_stats,
            ..
        } = response;
        code.map(|code| (code, build_stats, hint_stats))
            .ok_or_else(|| "Fission NIR worker returned success without code".to_string())
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "Fission NIR worker failed without error".to_string()))
    }
}

pub fn execute_nir_worker(request: &NirWorkerRequest) -> NirWorkerResponse {
    match render_nir_request(request) {
        Ok((code, build_stats, hint_stats)) => NirWorkerResponse {
            success: true,
            code: Some(code),
            build_stats,
            hint_stats,
            error: None,
        },
        Err(error) => NirWorkerResponse {
            success: false,
            code: None,
            build_stats: None,
            hint_stats: None,
            error: Some(error),
        },
    }
}

pub fn execute_preview_worker(request: &NirWorkerRequest) -> NirWorkerResponse {
    execute_nir_worker(request)
}

pub use crate::types::{PreviewWorkerRequest, PreviewWorkerResponse};
