use crate::render::{nir_diag_event, render_nir_request};
use crate::types::{NirWorkerRequest, NirWorkerResponse};
use std::fmt;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const NIR_WORKER_BIN_NAME: &str = "fission_nir_worker";
const LEGACY_PREVIEW_WORKER_BIN_NAME: &str = "fission_preview_worker";
const NIR_WORKER_MIN_TIMEOUT_MS: u64 = 1_000;

/// Upper bound on serialized NIR worker requests transferred over stdin.
pub const MAX_WORKER_REQUEST_BYTES: usize = 64 * 1024 * 1024;

/// Upper bound on serialized NIR worker responses accepted from stdout.
pub const MAX_WORKER_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

/// Upper bound on captured stderr diagnostics from a worker process.
pub(crate) const MAX_WORKER_STDERR_BYTES: usize = 64 * 1024;

/// Maximum time the supervisor waits for pipe EOF after the direct child exits.
const POST_EXIT_EOF_GRACE_MS: u64 = 250;

static NEXT_WORKER_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn nir_worker_timeout_ms(timeout_ms: Option<u64>) -> u64 {
    let configured =
        timeout_ms.unwrap_or_else(|| fission_core::config::CONFIG.decompiler.timeout_ms);
    if configured == 0 {
        return 0;
    }
    configured.max(NIR_WORKER_MIN_TIMEOUT_MS)
}

fn resolve_nir_worker_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("FISSION_NIR_WORKER") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(path) = std::env::var("FISSION_PREVIEW_WORKER") {
        let path = PathBuf::from(path);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkerFailureKind {
    Unavailable,
    TransferTimeout,
    ExecutionTimeout,
    WriteFailed,
    CrashedWorker,
    ProtocolFailure,
    CleanupFailure,
    WorkerRejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkerFailure {
    pub(crate) kind: WorkerFailureKind,
    pub(crate) message: String,
}

impl WorkerFailure {
    fn new(kind: WorkerFailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for WorkerFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct WorkerRunConfig {
    pub(crate) worker_path: PathBuf,
    pub(crate) timeout_ms: u64,
    pub(crate) max_request_bytes: usize,
    pub(crate) max_response_bytes: usize,
    pub(crate) max_stderr_bytes: usize,
    pub(crate) eof_grace: Duration,
}

impl WorkerRunConfig {
    fn with_path(worker_path: PathBuf, timeout_ms: u64) -> Self {
        Self {
            worker_path,
            timeout_ms,
            max_request_bytes: MAX_WORKER_REQUEST_BYTES,
            max_response_bytes: MAX_WORKER_RESPONSE_BYTES,
            max_stderr_bytes: MAX_WORKER_STDERR_BYTES,
            eof_grace: Duration::from_millis(POST_EXIT_EOF_GRACE_MS),
        }
    }
}

enum PipeReadOutcome {
    Ok(Vec<u8>),
    TooLarge { limit: usize, read: usize },
    Io(std::io::Error),
}

fn read_pipe_bounded(mut pipe: impl Read, limit: usize) -> PipeReadOutcome {
    let mut out = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) => return PipeReadOutcome::Ok(out),
            Ok(n) => {
                let next_len = out.len().saturating_add(n);
                if next_len > limit {
                    return PipeReadOutcome::TooLarge {
                        limit,
                        read: next_len,
                    };
                }
                out.extend_from_slice(&chunk[..n]);
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(err) => return PipeReadOutcome::Io(err),
        }
    }
}

fn spawn_pipe_reader(
    pipe: impl Read + Send + 'static,
    limit: usize,
) -> (Receiver<PipeReadOutcome>, JoinHandle<()>) {
    let (tx, rx) = mpsc::sync_channel(1);
    let handle = thread::spawn(move || {
        let outcome = read_pipe_bounded(pipe, limit);
        let _ = tx.send(outcome);
    });
    (rx, handle)
}

fn spawn_stdin_writer(
    mut pipe: impl Write + Send + 'static,
    payload: Vec<u8>,
) -> (Receiver<std::io::Result<()>>, JoinHandle<()>) {
    let (tx, rx) = mpsc::sync_channel(1);
    let handle = thread::spawn(move || {
        let result = pipe.write_all(&payload).and_then(|()| pipe.flush());
        drop(pipe);
        let _ = tx.send(result);
    });
    (rx, handle)
}

/// Owned child-process lifecycle guard.
///
/// Ensures any supervised worker child is killed and reaped on early return,
/// timeout, or panic.
///
/// Platform/descendant limitation: OS pipes (`stdin`, `stdout`, `stderr`) do not
/// signal EOF when a direct child exits if a spawned descendant still holds an
/// inherited descriptor open. Rather than hanging forever in `JoinHandle::join`,
/// cleanup closes/drops channels, kills and reaps the direct child, waits up to
/// a bounded grace period for worker I/O threads to observe EOF/EPIPE, joins
/// finished handles, and detaches any thread still blocked inside kernel I/O on
/// a leaked descriptor.
struct WorkerProcessGuard {
    child: Option<Child>,
    stdin_handle: Option<JoinHandle<()>>,
    stdout_handle: Option<JoinHandle<()>>,
    stderr_handle: Option<JoinHandle<()>>,
}

impl WorkerProcessGuard {
    fn new(child: Child) -> Self {
        Self {
            child: Some(child),
            stdin_handle: None,
            stdout_handle: None,
            stderr_handle: None,
        }
    }

    fn take_stdout(&mut self) -> Result<std::process::ChildStdout, WorkerFailure> {
        self.child
            .as_mut()
            .and_then(|c| c.stdout.take())
            .ok_or_else(|| {
                WorkerFailure::new(
                    WorkerFailureKind::ProtocolFailure,
                    "Fission NIR worker stdout unavailable",
                )
            })
    }

    fn take_stderr(&mut self) -> Result<std::process::ChildStderr, WorkerFailure> {
        self.child
            .as_mut()
            .and_then(|c| c.stderr.take())
            .ok_or_else(|| {
                WorkerFailure::new(
                    WorkerFailureKind::ProtocolFailure,
                    "Fission NIR worker stderr unavailable",
                )
            })
    }

    fn take_stdin(&mut self) -> Result<std::process::ChildStdin, WorkerFailure> {
        self.child
            .as_mut()
            .and_then(|c| c.stdin.take())
            .ok_or_else(|| {
                WorkerFailure::new(
                    WorkerFailureKind::WriteFailed,
                    "Fission NIR worker stdin unavailable",
                )
            })
    }

    fn try_wait(&mut self) -> Result<Option<ExitStatus>, WorkerFailure> {
        let Some(child) = self.child.as_mut() else {
            return Ok(None);
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                self.child = None;
                Ok(Some(status))
            }
            Ok(None) => Ok(None),
            Err(error) => Err(WorkerFailure::new(
                WorkerFailureKind::CrashedWorker,
                format!("Fission NIR worker wait failed: {error}"),
            )),
        }
    }

    fn join_finished_handles(&mut self) {
        for slot in [
            &mut self.stdin_handle,
            &mut self.stdout_handle,
            &mut self.stderr_handle,
        ] {
            if slot.as_ref().is_some_and(JoinHandle::is_finished)
                && let Some(handle) = slot.take()
            {
                let _ = handle.join();
            }
        }
    }

    fn abort_and_reap(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        // Give reader/writer threads a brief beat to observe kernel EPIPE/EOF
        // once the direct child dies, without hanging on leaked grandchild FDs.
        let grace_end = Instant::now() + Duration::from_millis(50);
        loop {
            self.join_finished_handles();
            let all_joined = self.stdin_handle.is_none()
                && self.stdout_handle.is_none()
                && self.stderr_handle.is_none();
            if all_joined || Instant::now() >= grace_end {
                break;
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
}

impl Drop for WorkerProcessGuard {
    fn drop(&mut self) {
        self.abort_and_reap();
    }
}

fn recv_until<T>(rx: &Receiver<T>, deadline: Instant) -> Result<T, RecvTimeoutError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    rx.recv_timeout(remaining)
}

fn stderr_note(outcome: Option<&PipeReadOutcome>) -> String {
    if let Some(PipeReadOutcome::Ok(bytes)) = outcome {
        let text = String::from_utf8_lossy(bytes);
        let trimmed = text.trim();
        if !trimmed.is_empty() {
            return format!("; stderr={trimmed}");
        }
    }
    String::new()
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
        return Err(
            WorkerFailure::new(WorkerFailureKind::Unavailable, "nir worker unavailable")
                .to_string(),
        );
    };
    let config = WorkerRunConfig::with_path(worker_path, timeout_ms);
    execute_nir_worker_request_with_config(request, &config).map_err(|failure| failure.message)
}

pub(crate) fn execute_nir_worker_request_with_config(
    request: &NirWorkerRequest,
    config: &WorkerRunConfig,
) -> Result<
    (
        String,
        Option<crate::NirBuildStats>,
        Option<crate::NirHintStats>,
    ),
    WorkerFailure,
> {
    nir_diag_event(
        request.address,
        "worker_spawn",
        format!("path={}", config.worker_path.display()),
    );

    // Assign monotonic request identity if the caller left it unset and verify
    // serialized size before starting a subprocess.
    let sent_request_id = request
        .request_id
        .unwrap_or_else(|| NEXT_WORKER_REQUEST_ID.fetch_add(1, Ordering::Relaxed));
    let mut outbound = request.clone();
    outbound.request_id = Some(sent_request_id);
    let request_json = serde_json::to_vec(&outbound).map_err(|e| {
        WorkerFailure::new(
            WorkerFailureKind::ProtocolFailure,
            format!("Fission NIR worker request serialization failed: {e}"),
        )
    })?;
    if request_json.len() > config.max_request_bytes {
        return Err(WorkerFailure::new(
            WorkerFailureKind::ProtocolFailure,
            format!(
                "Fission NIR worker request exceeded {} byte limit ({} bytes)",
                config.max_request_bytes,
                request_json.len()
            ),
        ));
    }

    // Start the single overall deadline before spawn and request transfer so a
    // wedged stdin consumer cannot stall outside the watchdog.
    let started_at = Instant::now();
    let deadline =
        (config.timeout_ms > 0).then(|| started_at + Duration::from_millis(config.timeout_ms));

    let child = Command::new(&config.worker_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            WorkerFailure::new(
                WorkerFailureKind::CrashedWorker,
                format!("Fission NIR worker spawn failed: {e}"),
            )
        })?;
    let mut guard = WorkerProcessGuard::new(child);

    // Drain stdout and stderr concurrently alongside non-blocking stdin transfer.
    let stdout = guard.take_stdout()?;
    let (stdout_rx, stdout_handle) = spawn_pipe_reader(stdout, config.max_response_bytes);
    guard.stdout_handle = Some(stdout_handle);

    let stderr = guard.take_stderr()?;
    let (stderr_rx, stderr_handle) = spawn_pipe_reader(stderr, config.max_stderr_bytes);
    guard.stderr_handle = Some(stderr_handle);

    let stdin = guard.take_stdin()?;
    let (stdin_rx, stdin_handle) = spawn_stdin_writer(stdin, request_json);
    guard.stdin_handle = Some(stdin_handle);

    let mut stdin_done = false;
    let mut stdin_error: Option<String> = None;

    let exit_status = loop {
        if !stdin_done {
            match stdin_rx.try_recv() {
                Ok(Ok(())) => stdin_done = true,
                Ok(Err(error)) => {
                    stdin_done = true;
                    stdin_error = Some(error.to_string());
                }
                Err(TryRecvError::Disconnected) => {
                    stdin_done = true;
                    stdin_error = Some("writer thread exited unexpectedly".to_string());
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        // Fail fast if stdout already overflowed the response limit while the child is still running.
        match stdout_rx.try_recv() {
            Ok(PipeReadOutcome::TooLarge { limit, read }) => {
                guard.abort_and_reap();
                return Err(WorkerFailure::new(
                    WorkerFailureKind::ProtocolFailure,
                    format!(
                        "Fission NIR worker response exceeded {limit} byte limit (observed >= {read} bytes)"
                    ),
                ));
            }
            Ok(PipeReadOutcome::Io(err)) => {
                guard.abort_and_reap();
                return Err(WorkerFailure::new(
                    WorkerFailureKind::ProtocolFailure,
                    format!("Fission NIR worker stdout read failed: {err}"),
                ));
            }
            Ok(PipeReadOutcome::Ok(bytes)) => {
                // Keep output available only after exit_status completes: re-send locally by storing?
                // Wait: if stdout closed cleanly before exit_status, wait briefly for exit_status
                // rather than re-reading an empty receiver! Handle below via option.
                let status = wait_for_exit_after_stdout_eof(
                    &mut guard,
                    &stdin_rx,
                    &mut stdin_done,
                    &mut stdin_error,
                    deadline,
                    started_at,
                    request.address,
                    config.timeout_ms,
                )?;
                let stderr_outcome =
                    recv_until(&stderr_rx, post_exit_deadline(deadline, config.eof_grace));
                let stdin_cleanup_ok = wait_for_stdin_cleanup(
                    &stdin_rx,
                    &mut stdin_done,
                    &mut stdin_error,
                    post_exit_deadline(deadline, config.eof_grace),
                );
                guard.join_finished_handles();
                if !stdin_cleanup_ok {
                    return Err(WorkerFailure::new(
                        WorkerFailureKind::CleanupFailure,
                        format!(
                            "Fission NIR worker cleanup failed: stdin writer stalled after exit status {status}"
                        ),
                    ));
                }
                if let Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) =
                    stderr_outcome
                {
                    return Err(WorkerFailure::new(
                        WorkerFailureKind::CleanupFailure,
                        format!(
                            "Fission NIR worker delayed EOF on stderr after exit status {status}"
                        ),
                    ));
                }
                return finalize_worker_result(
                    request,
                    sent_request_id,
                    status,
                    PipeReadOutcome::Ok(bytes),
                    stderr_outcome.ok(),
                    stdin_error,
                );
            }
            Err(TryRecvError::Disconnected) => {
                guard.abort_and_reap();
                return Err(WorkerFailure::new(
                    WorkerFailureKind::ProtocolFailure,
                    "Fission NIR worker stdout reader disconnected",
                ));
            }
            Err(TryRecvError::Empty) => {}
        }

        if let Some(status) = guard.try_wait()? {
            nir_diag_event(
                request.address,
                "worker_exit",
                format!(
                    "status={status} elapsed_ms={:.1}",
                    started_at.elapsed().as_secs_f64() * 1000.0
                ),
            );
            break status;
        }

        if let Some(dl) = deadline
            && Instant::now() >= dl
        {
            nir_diag_event(
                request.address,
                "worker_timeout",
                format!("budget_ms={}", config.timeout_ms),
            );
            guard.abort_and_reap();
            let failure = if !stdin_done {
                WorkerFailure::new(
                    WorkerFailureKind::TransferTimeout,
                    format!(
                        "nir_timeout: Fission NIR worker request transfer timed out after {}ms",
                        config.timeout_ms
                    ),
                )
            } else {
                WorkerFailure::new(
                    WorkerFailureKind::ExecutionTimeout,
                    format!(
                        "nir_timeout: Fission NIR worker timed out after {}ms",
                        config.timeout_ms
                    ),
                )
            };
            return Err(failure);
        }

        thread::sleep(Duration::from_millis(5));
    };

    let pipe_deadline = post_exit_deadline(deadline, config.eof_grace);
    let stdout_outcome = recv_until(&stdout_rx, pipe_deadline).map_err(|_| {
        guard.abort_and_reap();
        WorkerFailure::new(
            WorkerFailureKind::CleanupFailure,
            format!("Fission NIR worker delayed EOF after exit status {exit_status}"),
        )
    })?;
    let stderr_outcome = recv_until(&stderr_rx, pipe_deadline).map_err(|_| {
        guard.abort_and_reap();
        WorkerFailure::new(
            WorkerFailureKind::CleanupFailure,
            format!("Fission NIR worker delayed EOF on stderr after exit status {exit_status}"),
        )
    })?;
    if !wait_for_stdin_cleanup(&stdin_rx, &mut stdin_done, &mut stdin_error, pipe_deadline) {
        guard.abort_and_reap();
        return Err(WorkerFailure::new(
            WorkerFailureKind::CleanupFailure,
            format!(
                "Fission NIR worker cleanup failed: stdin writer stalled after exit status {exit_status}"
            ),
        ));
    }
    guard.join_finished_handles();

    finalize_worker_result(
        request,
        sent_request_id,
        exit_status,
        stdout_outcome,
        Some(stderr_outcome),
        stdin_error,
    )
}

fn post_exit_deadline(deadline: Option<Instant>, grace: Duration) -> Instant {
    let capped = Instant::now() + grace;
    match deadline {
        Some(dl) => dl
            .min(capped)
            .max(Instant::now() + Duration::from_millis(25)),
        None => capped,
    }
}

fn wait_for_exit_after_stdout_eof(
    guard: &mut WorkerProcessGuard,
    stdin_rx: &Receiver<std::io::Result<()>>,
    stdin_done: &mut bool,
    stdin_error: &mut Option<String>,
    deadline: Option<Instant>,
    started_at: Instant,
    address: u64,
    timeout_ms: u64,
) -> Result<ExitStatus, WorkerFailure> {
    loop {
        if !*stdin_done {
            match stdin_rx.try_recv() {
                Ok(Ok(())) => *stdin_done = true,
                Ok(Err(error)) => {
                    *stdin_done = true;
                    *stdin_error = Some(error.to_string());
                }
                Err(TryRecvError::Disconnected) => {
                    *stdin_done = true;
                    *stdin_error = Some("writer thread exited unexpectedly".to_string());
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(status) = guard.try_wait()? {
            nir_diag_event(
                address,
                "worker_exit",
                format!(
                    "status={status} elapsed_ms={:.1}",
                    started_at.elapsed().as_secs_f64() * 1000.0
                ),
            );
            return Ok(status);
        }
        if let Some(dl) = deadline
            && Instant::now() >= dl
        {
            guard.abort_and_reap();
            return Err(WorkerFailure::new(
                if !*stdin_done {
                    WorkerFailureKind::TransferTimeout
                } else {
                    WorkerFailureKind::ExecutionTimeout
                },
                format!(
                    "nir_timeout: Fission NIR worker timed out after {timeout_ms}ms after closing stdout"
                ),
            ));
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn wait_for_stdin_cleanup(
    stdin_rx: &Receiver<std::io::Result<()>>,
    stdin_done: &mut bool,
    stdin_error: &mut Option<String>,
    deadline: Instant,
) -> bool {
    if *stdin_done {
        return true;
    }
    match recv_until(stdin_rx, deadline) {
        Ok(Ok(())) => {
            *stdin_done = true;
            true
        }
        Ok(Err(err)) => {
            *stdin_done = true;
            *stdin_error = Some(err.to_string());
            true
        }
        Err(RecvTimeoutError::Disconnected) => {
            *stdin_done = true;
            *stdin_error = Some("writer thread exited unexpectedly".to_string());
            true
        }
        Err(RecvTimeoutError::Timeout) => false,
    }
}

fn finalize_worker_result(
    request: &NirWorkerRequest,
    sent_request_id: u64,
    exit_status: ExitStatus,
    stdout_outcome: PipeReadOutcome,
    stderr_outcome: Option<PipeReadOutcome>,
    stdin_error: Option<String>,
) -> Result<
    (
        String,
        Option<crate::NirBuildStats>,
        Option<crate::NirHintStats>,
    ),
    WorkerFailure,
> {
    let stderr_suffix = stderr_note(stderr_outcome.as_ref());
    let stdout_bytes = match stdout_outcome {
        PipeReadOutcome::Ok(bytes) => bytes,
        PipeReadOutcome::TooLarge { limit, read } => {
            return Err(WorkerFailure::new(
                WorkerFailureKind::ProtocolFailure,
                format!(
                    "Fission NIR worker response exceeded {limit} byte limit (observed >= {read} bytes){stderr_suffix}"
                ),
            ));
        }
        PipeReadOutcome::Io(err) => {
            return Err(WorkerFailure::new(
                WorkerFailureKind::ProtocolFailure,
                format!("Fission NIR worker stdout read failed: {err}{stderr_suffix}"),
            ));
        }
    };

    let stdout_text = String::from_utf8(stdout_bytes).map_err(|e| {
        WorkerFailure::new(
            WorkerFailureKind::ProtocolFailure,
            format!("Fission NIR worker response parse failed: invalid UTF-8: {e}{stderr_suffix}"),
        )
    })?;

    if stdout_text.trim().is_empty() {
        let stdin_suffix = stdin_error
            .as_deref()
            .map(|e| format!("; stdin write failed: {e}"))
            .unwrap_or_default();
        let kind = if exit_status.success() {
            WorkerFailureKind::ProtocolFailure
        } else {
            WorkerFailureKind::CrashedWorker
        };
        return Err(WorkerFailure::new(
            kind,
            format!(
                "Fission NIR worker exited with status {exit_status} without JSON response{stdin_suffix}{stderr_suffix}"
            ),
        ));
    }

    let response: NirWorkerResponse = serde_json::from_str(&stdout_text).map_err(|e| {
        let kind = if exit_status.success() {
            WorkerFailureKind::ProtocolFailure
        } else {
            WorkerFailureKind::CrashedWorker
        };
        WorkerFailure::new(
            kind,
            format!(
                "Fission NIR worker response parse failed (status {exit_status}): {e}{stderr_suffix}"
            ),
        )
    })?;

    if let Some(got_request_id) = response.request_id
        && got_request_id != sent_request_id
    {
        return Err(WorkerFailure::new(
            WorkerFailureKind::ProtocolFailure,
            format!(
                "Fission NIR worker response request identity mismatch: expected request_id {sent_request_id}, got {got_request_id}"
            ),
        ));
    }
    if let Some(got_address) = response.address
        && got_address != request.address
    {
        return Err(WorkerFailure::new(
            WorkerFailureKind::ProtocolFailure,
            format!(
                "Fission NIR worker response request identity mismatch: expected address 0x{:x}, got 0x{:x}",
                request.address, got_address
            ),
        ));
    }

    if response.success {
        if !exit_status.success() {
            return Err(WorkerFailure::new(
                WorkerFailureKind::CrashedWorker,
                format!(
                    "Fission NIR worker exited with non-zero status {exit_status} after response{stderr_suffix}"
                ),
            ));
        }
        let NirWorkerResponse {
            code,
            build_stats,
            hint_stats,
            ..
        } = response;
        code.map(|code| (code, build_stats, hint_stats))
            .ok_or_else(|| {
                WorkerFailure::new(
                    WorkerFailureKind::ProtocolFailure,
                    "Fission NIR worker returned success without code",
                )
            })
    } else if !matches!(exit_status.code(), Some(0) | Some(2)) {
        Err(WorkerFailure::new(
            WorkerFailureKind::CrashedWorker,
            format!(
                "Fission NIR worker crashed with status {exit_status}: {}{stderr_suffix}",
                response
                    .error
                    .unwrap_or_else(|| "worker failed without error".to_string())
            ),
        ))
    } else {
        Err(WorkerFailure::new(
            WorkerFailureKind::WorkerRejected,
            response
                .error
                .unwrap_or_else(|| "Fission NIR worker failed without error".to_string()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_WORKER_STDERR_BYTES, PipeReadOutcome, WorkerFailureKind, WorkerRunConfig,
        execute_nir_worker_request_with_config, spawn_pipe_reader,
    };
    use crate::types::NirWorkerRequest;
    use crate::{NirRenderOptions, NirTypeContext};
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};

    #[cfg(unix)]
    struct TempScriptDir {
        dir: PathBuf,
    }

    #[cfg(unix)]
    impl TempScriptDir {
        fn new(prefix: &str) -> Self {
            static SEQ: AtomicU64 = AtomicU64::new(0);
            let dir = std::env::temp_dir().join(format!(
                "fission_worker_test_{prefix}_{}_{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&dir).expect("create temp dir");
            Self { dir }
        }

        fn write_executable(&self, name: &str, body: &str) -> PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let path = self.dir.join(name);
            std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write script");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("chmod +x");
            path
        }
    }

    #[cfg(unix)]
    impl Drop for TempScriptDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn sample_request(pcode_len: usize) -> NirWorkerRequest {
        NirWorkerRequest {
            request_id: None,
            pcode_json: "a".repeat(pcode_len),
            address: 0x14000_1000,
            name: "test_fn".to_string(),
            options: NirRenderOptions::default(),
            type_context: NirTypeContext::default(),
        }
    }

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
        let (rx, handle) = spawn_pipe_reader(stdout, 1024 * 1024);

        let status = child.wait().expect("wait stdout fixture");
        assert!(status.success());
        let outcome = rx.recv().expect("reader outcome");
        handle.join().expect("reader join");
        match outcome {
            PipeReadOutcome::Ok(bytes) => assert_eq!(bytes.len(), 131_072),
            PipeReadOutcome::TooLarge { .. } => panic!("unexpected size failure"),
            PipeReadOutcome::Io(err) => panic!("unexpected io error: {err}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_oversized_request_before_spawning_worker() {
        let tmp = TempScriptDir::new("req_limit");
        let marker = tmp.dir.join("spawned.marker");
        let script = tmp.write_executable(
            "marker.sh",
            &format!("touch '{}'\nexit 0", marker.display()),
        );
        let mut config = WorkerRunConfig::with_path(script, 5_000);
        config.max_request_bytes = 512;

        let err = execute_nir_worker_request_with_config(&sample_request(1024), &config)
            .expect_err("oversized request must fail");
        assert_eq!(err.kind, WorkerFailureKind::ProtocolFailure);
        assert!(err.message.contains("exceeded"));
        assert!(!marker.exists(), "worker must not be spawned");
    }

    #[cfg(unix)]
    #[test]
    fn transfer_timeout_breaks_blocked_large_stdin_write() {
        let tmp = TempScriptDir::new("transfer_timeout");
        // Sleeps without reading stdin; a > pipe-buffer request blocks in write_all
        // unless transfer runs asynchronously under the overall deadline.
        let script = tmp.write_executable("block_stdin.sh", "sleep 5");
        let config = WorkerRunConfig::with_path(script, 120);

        let started = Instant::now();
        let err = execute_nir_worker_request_with_config(&sample_request(256 * 1024), &config)
            .expect_err("blocked stdin write must time out");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "watchdog took too long: {:?}",
            started.elapsed()
        );
        assert_eq!(err.kind, WorkerFailureKind::TransferTimeout);
        assert!(err.message.contains("nir_timeout"));
    }

    #[cfg(unix)]
    #[test]
    fn execution_timeout_kills_slow_worker_after_transfer() {
        let tmp = TempScriptDir::new("exec_timeout");
        let script = tmp.write_executable("slow_exec.sh", "cat >/dev/null\nsleep 5");
        let config = WorkerRunConfig::with_path(script, 120);

        let started = Instant::now();
        let err = execute_nir_worker_request_with_config(&sample_request(16), &config)
            .expect_err("slow worker must time out");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "watchdog took too long: {:?}",
            started.elapsed()
        );
        assert_eq!(err.kind, WorkerFailureKind::ExecutionTimeout);
        assert!(err.message.contains("nir_timeout"));
    }

    #[cfg(unix)]
    #[test]
    fn mid_transfer_child_exit_reports_crashed_worker_with_stderr() {
        let tmp = TempScriptDir::new("mid_exit");
        let script = tmp.write_executable("die_early.sh", "echo 'boom' >&2\nexit 1");
        let config = WorkerRunConfig::with_path(script, 5_000);

        let err = execute_nir_worker_request_with_config(&sample_request(256 * 1024), &config)
            .expect_err("early exit must fail");
        assert_eq!(err.kind, WorkerFailureKind::CrashedWorker);
        assert!(err.message.contains("boom"), "{}", err.message);
    }

    #[cfg(unix)]
    #[test]
    fn oversized_worker_response_is_rejected_without_oom() {
        let tmp = TempScriptDir::new("resp_limit");
        let script = tmp.write_executable(
            "flood.sh",
            "cat >/dev/null\ndd if=/dev/zero bs=4096 count=64 2>/dev/null | tr '\\000' 'x'",
        );
        let mut config = WorkerRunConfig::with_path(script, 5_000);
        config.max_response_bytes = 32 * 1024;
        config.max_stderr_bytes = MAX_WORKER_STDERR_BYTES;

        let err = execute_nir_worker_request_with_config(&sample_request(16), &config)
            .expect_err("oversized stdout must be rejected");
        assert_eq!(err.kind, WorkerFailureKind::ProtocolFailure);
        assert!(err.message.contains("exceeded"));
    }

    #[cfg(unix)]
    #[test]
    fn malformed_json_and_valid_json_then_nonzero_exit_do_not_publish_success() {
        let tmp = TempScriptDir::new("bad_json_or_exit");

        let truncated = tmp.write_executable(
            "truncated.sh",
            "cat >/dev/null\nprintf '{\"success\":true,\"code\":\"int f()'\nexit 0",
        );
        let err = execute_nir_worker_request_with_config(
            &sample_request(16),
            &WorkerRunConfig::with_path(truncated, 5_000),
        )
        .expect_err("truncated JSON must fail");
        assert_eq!(err.kind, WorkerFailureKind::ProtocolFailure);

        // Valid-looking response followed by non-zero exit must never pass as success.
        let nonzero_after_ok = tmp.write_executable(
            "ok_then_crash.sh",
            "cat >/dev/null\necho 'post-response panic' >&2\nprintf '{\"success\":true,\"code\":\"int f() { return 0; }\"}\\n'\nexit 1",
        );
        let err = execute_nir_worker_request_with_config(
            &sample_request(16),
            &WorkerRunConfig::with_path(nonzero_after_ok, 5_000),
        )
        .expect_err("valid JSON followed by non-zero exit must be rejected");
        assert_eq!(err.kind, WorkerFailureKind::CrashedWorker);
        assert!(
            err.message.contains("post-response panic"),
            "{}",
            err.message
        );

        // Semantic worker rejection (`exit 2` with `success:false`) preserves error reason.
        let semantic_err = tmp.write_executable(
            "semantic_err.sh",
            "cat >/dev/null\nprintf '{\"success\":false,\"error\":\"nir_unsupported: test reason\"}\\n'\nexit 2",
        );
        let err = execute_nir_worker_request_with_config(
            &sample_request(16),
            &WorkerRunConfig::with_path(semantic_err, 5_000),
        )
        .expect_err("error response must return failure");
        assert_eq!(err.kind, WorkerFailureKind::WorkerRejected);
        assert_eq!(err.message, "nir_unsupported: test reason");
    }

    #[cfg(unix)]
    #[test]
    fn request_identity_mismatch_is_rejected_and_matching_identity_succeeds() {
        let tmp = TempScriptDir::new("identity");

        let wrong_req_id = tmp.write_executable(
            "wrong_id.sh",
            "cat >/dev/null\nprintf '{\"request_id\":999999,\"address\":5368713216,\"success\":true,\"code\":\"ok\"}\\n'",
        );
        let mut req = sample_request(16);
        req.request_id = Some(42);
        let err = execute_nir_worker_request_with_config(
            &req,
            &WorkerRunConfig::with_path(wrong_req_id, 5_000),
        )
        .expect_err("mismatched request_id must be rejected");
        assert_eq!(err.kind, WorkerFailureKind::ProtocolFailure);
        assert!(err.message.contains("request identity mismatch"));

        let wrong_addr = tmp.write_executable(
            "wrong_addr.sh",
            "cat >/dev/null\nprintf '{\"request_id\":42,\"address\":1234,\"success\":true,\"code\":\"ok\"}\\n'",
        );
        let err = execute_nir_worker_request_with_config(
            &req,
            &WorkerRunConfig::with_path(wrong_addr, 5_000),
        )
        .expect_err("mismatched address must be rejected");
        assert_eq!(err.kind, WorkerFailureKind::ProtocolFailure);
        assert!(err.message.contains("request identity mismatch"));

        let ok_script = tmp.write_executable(
            "ok.sh",
            "cat >/dev/null\nprintf '{\"request_id\":42,\"address\":5368713216,\"success\":true,\"code\":\"int test_fn() { return 0; }\"}\\n'",
        );
        let (code, _, _) = execute_nir_worker_request_with_config(
            &req,
            &WorkerRunConfig::with_path(ok_script, 5_000),
        )
        .expect("matching request identity and exit 0 must succeed");
        assert_eq!(code, "int test_fn() { return 0; }");
    }

    #[cfg(unix)]
    #[test]
    fn delayed_eof_from_descendant_returns_cleanup_failure_without_hanging() {
        let tmp = TempScriptDir::new("delayed_eof");
        // Child spawns a background descendant holding stdout open, emits JSON, then exits 0.
        let script = tmp.write_executable(
            "delayed_eof.sh",
            "cat >/dev/null\n(sleep 4) &\nprintf '{\"success\":true,\"code\":\"int f(){}\"}\\n'\nexit 0",
        );
        let mut config = WorkerRunConfig::with_path(script, 5_000);
        config.eof_grace = Duration::from_millis(80);

        let started = Instant::now();
        let err = execute_nir_worker_request_with_config(&sample_request(16), &config)
            .expect_err("delayed EOF must fail bounded cleanup");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "supervisor hung waiting for delayed EOF: {:?}",
            started.elapsed()
        );
        assert_eq!(err.kind, WorkerFailureKind::CleanupFailure);
        assert!(err.message.contains("delayed EOF"));
    }

    #[test]
    fn execute_nir_worker_echoes_request_identity_and_taxonomy_recognizes_new_failures() {
        let mut req = sample_request(16);
        req.request_id = Some(77);
        let resp = super::execute_nir_worker(&req);
        assert!(!resp.success);
        assert_eq!(resp.request_id, Some(77));
        assert_eq!(resp.address, Some(req.address));
        assert_eq!(
            crate::classify_nir_failure_refined(
                "Fission NIR worker exited with non-zero status exit status: 1 after response"
            ),
            "nir_worker_failure"
        );
        assert_eq!(
            crate::classify_nir_failure_refined(
                "Fission NIR worker delayed EOF after exit status: 0"
            ),
            "nir_worker_failure"
        );
    }
}

pub fn execute_nir_worker(request: &NirWorkerRequest) -> NirWorkerResponse {
    match render_nir_request(request) {
        Ok((code, build_stats, hint_stats)) => NirWorkerResponse {
            request_id: request.request_id,
            address: Some(request.address),
            success: true,
            code: Some(code),
            build_stats,
            hint_stats,
            error: None,
        },
        Err(error) => NirWorkerResponse {
            request_id: request.request_id,
            address: Some(request.address),
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
