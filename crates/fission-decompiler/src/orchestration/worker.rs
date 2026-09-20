use crate::render::{nir_diag_event, render_nir_request};
use crate::types::{NirWorkerRequest, NirWorkerResponse};
use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::thread::{self, JoinHandle};
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

fn drain_worker_stdout(
    mut pipe: impl Read + Send + 'static,
) -> JoinHandle<std::io::Result<String>> {
    thread::spawn(move || {
        let mut stdout = String::new();
        pipe.read_to_string(&mut stdout).map(|_| stdout)
    })
}

fn join_worker_stdout(reader: JoinHandle<std::io::Result<String>>) -> Result<String, String> {
    reader
        .join()
        .map_err(|_| "Fission NIR worker stdout reader panicked".to_string())?
        .map_err(|e| format!("Fission NIR worker stdout read failed: {e}"))
}

fn abort_worker(child: &mut Child, reader: JoinHandle<std::io::Result<String>>) {
    let _ = child.kill();
    let _ = child.wait();
    let _ = reader.join();
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

    let request_json = serde_json::to_vec(request)
        .map_err(|e| format!("Fission NIR worker request serialization failed: {e}"))?;

    let mut child = Command::new(&worker_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("Fission NIR worker spawn failed: {e}"))?;

    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Fission NIR worker stdout unavailable".to_string());
        }
    };
    // Drain stdout while the child is running. Waiting for process exit before
    // reading is a pipe deadlock for large JSON responses: the child can block
    // in write(2) once the OS pipe buffer fills, so it never reaches exit.
    let stdout_reader = drain_worker_stdout(stdout);

    let mut stdin = match child.stdin.take() {
        Some(stdin) => stdin,
        None => {
            abort_worker(&mut child, stdout_reader);
            return Err("Fission NIR worker stdin unavailable".to_string());
        }
    };
    if let Err(error) = stdin.write_all(&request_json) {
        let message = format!("Fission NIR worker stdin write failed: {error}");
        drop(stdin);
        abort_worker(&mut child, stdout_reader);
        return Err(message);
    }
    drop(stdin);

    let start = Instant::now();
    let exit_status = loop {
        let status = match child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                let message = format!("Fission NIR worker wait failed: {error}");
                abort_worker(&mut child, stdout_reader);
                return Err(message);
            }
        };
        if let Some(status) = status {
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
            abort_worker(&mut child, stdout_reader);
            return Err(format!(
                "nir_timeout: Fission NIR worker timed out after {timeout_ms}ms"
            ));
        }
        thread::sleep(Duration::from_millis(10));
    };

    let stdout = join_worker_stdout(stdout_reader)?;

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

#[cfg(test)]
mod tests {
    use super::{drain_worker_stdout, join_worker_stdout};
    use std::process::{Command, Stdio};

    #[cfg(unix)]
    #[test]
    fn drains_stdout_larger_than_a_pipe_buffer_before_joining() {
        let mut child = Command::new("sh")
            .args([
                "-c",
                "i=0; while [ $i -lt 131072 ]; do printf x; i=$((i + 1)); done",
            ])
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn stdout fixture");
        let stdout = child.stdout.take().expect("stdout pipe");
        let reader = drain_worker_stdout(stdout);

        let status = child.wait().expect("wait stdout fixture");
        assert!(status.success());
        let output = join_worker_stdout(reader).expect("drain stdout fixture");
        assert_eq!(output.len(), 131_072);
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
