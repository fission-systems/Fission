use crate::render::{nir_diag_event, render_nir_request};
use crate::types::{NirWorkerRequest, NirWorkerResponse};
use std::fmt;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const NIR_WORKER_BIN_NAME: &str = "fission_nir_worker";
const LEGACY_PREVIEW_WORKER_BIN_NAME: &str = "fission_preview_worker";
const NIR_WORKER_MIN_TIMEOUT_MS: u64 = 1_000;

/// Upper bound on serialized NIR worker requests transferred over stdin.
pub const MAX_WORKER_REQUEST_BYTES: usize = 64 * 1024 * 1024;

/// Upper bound on serialized NIR worker responses accepted from stdout.
pub const MAX_WORKER_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

/// Upper bound on captured stderr diagnostics from a worker process. stderr
/// beyond this is still drained (so the worker never blocks or takes SIGPIPE
/// on a full diagnostics pipe) but discarded and counted.
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
    Cancelled,
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

/// Bounded stderr capture: keeps the first `limit` bytes, counts the rest.
struct StderrCapture {
    bytes: Vec<u8>,
    limit: usize,
    dropped: usize,
}

impl StderrCapture {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
            dropped: 0,
        }
    }

    fn push(&mut self, chunk: &[u8]) {
        let room = self.limit.saturating_sub(self.bytes.len());
        let keep = room.min(chunk.len());
        self.bytes.extend_from_slice(&chunk[..keep]);
        self.dropped = self.dropped.saturating_add(chunk.len() - keep);
    }

    /// `; stderr=...` suffix for failure messages (empty when nothing was written).
    fn note(&self) -> String {
        let text = String::from_utf8_lossy(&self.bytes);
        let trimmed = text.trim();
        match (trimmed.is_empty(), self.dropped) {
            (true, 0) => String::new(),
            (_, 0) => format!("; stderr={trimmed}"),
            (_, dropped) => {
                format!("; stderr={trimmed} [stderr truncated: {dropped} further bytes discarded]")
            }
        }
    }
}

/// Everything one worker process run produced, before protocol validation.
/// Only constructed once the direct child has been reaped, every pipe has
/// reached EOF and the worker's process tree has been terminated.
struct WorkerRun {
    exit_status: ExitStatus,
    stdout: Vec<u8>,
    stderr: StderrCapture,
    /// `None` when the whole request reached the worker's stdin.
    stdin_error: Option<String>,
}

fn timeout_failure(transfer_pending: bool, timeout_ms: u64) -> WorkerFailure {
    if transfer_pending {
        WorkerFailure::new(
            WorkerFailureKind::TransferTimeout,
            format!(
                "nir_timeout: Fission NIR worker request transfer timed out after {timeout_ms}ms"
            ),
        )
    } else {
        WorkerFailure::new(
            WorkerFailureKind::ExecutionTimeout,
            format!("nir_timeout: Fission NIR worker timed out after {timeout_ms}ms"),
        )
    }
}

fn cancelled_failure() -> WorkerFailure {
    WorkerFailure::new(
        WorkerFailureKind::Cancelled,
        "nir_cancelled: Fission NIR worker cancelled",
    )
}

fn check_worker_cancellation(transfer_pending: bool, timeout_ms: u64) -> Result<(), WorkerFailure> {
    match fission_core::cancellation::check_current_cancellation() {
        Ok(()) => Ok(()),
        Err(fission_core::cancellation::CancellationReason::Cancelled) => Err(cancelled_failure()),
        Err(fission_core::cancellation::CancellationReason::TimedOut) => {
            Err(timeout_failure(transfer_pending, timeout_ms))
        }
    }
}

fn response_too_large(limit: usize, observed: usize) -> WorkerFailure {
    WorkerFailure::new(
        WorkerFailureKind::ProtocolFailure,
        format!(
            "Fission NIR worker response exceeded {limit} byte limit (observed >= {observed} bytes)"
        ),
    )
}

fn delayed_eof(open: &[&str], grace: Duration) -> WorkerFailure {
    WorkerFailure::new(
        WorkerFailureKind::CleanupFailure,
        format!(
            "Fission NIR worker delayed EOF: {} still open {}ms after the worker exited \
             (a descendant inherited the pipe); worker process tree killed",
            open.join("/"),
            grace.as_millis()
        ),
    )
}

fn spawn_failure(error: std::io::Error) -> WorkerFailure {
    WorkerFailure::new(
        WorkerFailureKind::CrashedWorker,
        format!("Fission NIR worker spawn failed: {error}"),
    )
}

fn wait_failure(error: std::io::Error) -> WorkerFailure {
    WorkerFailure::new(
        WorkerFailureKind::CrashedWorker,
        format!("Fission NIR worker wait failed: {error}"),
    )
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

    check_worker_cancellation(false, config.timeout_ms)?;

    // One absolute deadline covers spawn, request transfer and execution, so a
    // worker that never drains stdin cannot stall outside the watchdog.
    let started_at = Instant::now();
    let config_deadline =
        (config.timeout_ms > 0).then(|| started_at + Duration::from_millis(config.timeout_ms));
    let scope_deadline =
        fission_core::cancellation::current_cancellation_token().and_then(|t| t.deadline());
    let deadline = match (config_deadline, scope_deadline) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (Some(a), None) => Some(a),
        (None, Some(b)) => Some(b),
        (None, None) => None,
    };

    let run = supervise::run(config, &request_json, request.address, started_at, deadline)?;
    finalize_worker_result(request, sent_request_id, run)
}

fn finalize_worker_result(
    request: &NirWorkerRequest,
    sent_request_id: u64,
    run: WorkerRun,
) -> Result<
    (
        String,
        Option<crate::NirBuildStats>,
        Option<crate::NirHintStats>,
    ),
    WorkerFailure,
> {
    let WorkerRun {
        exit_status,
        stdout,
        stderr,
        stdin_error,
    } = run;
    let stderr_suffix = stderr.note();
    let stdin_suffix = stdin_error
        .as_deref()
        .map(|e| format!("; stdin write failed: {e}"))
        .unwrap_or_default();
    let failed_exit_kind = || {
        if exit_status.success() {
            WorkerFailureKind::ProtocolFailure
        } else {
            WorkerFailureKind::CrashedWorker
        }
    };

    let stdout_text = String::from_utf8(stdout).map_err(|e| {
        WorkerFailure::new(
            failed_exit_kind(),
            format!(
                "Fission NIR worker response parse failed (status {exit_status}): invalid UTF-8: {e}{stderr_suffix}"
            ),
        )
    })?;

    if stdout_text.trim().is_empty() {
        return Err(WorkerFailure::new(
            failed_exit_kind(),
            format!(
                "Fission NIR worker exited with status {exit_status} without JSON response{stdin_suffix}{stderr_suffix}"
            ),
        ));
    }

    let response: NirWorkerResponse = serde_json::from_str(&stdout_text).map_err(|e| {
        WorkerFailure::new(
            failed_exit_kind(),
            format!(
                "Fission NIR worker response parse failed (status {exit_status}): {e}{stderr_suffix}"
            ),
        )
    })?;

    // Identity, when present, must match on every response.
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
        // Publishing requires, in order: a clean exit, the whole request
        // delivered (a worker that stopped reading cannot have analysed it),
        // and an answer that names the request it answers.
        if !exit_status.success() {
            return Err(WorkerFailure::new(
                WorkerFailureKind::CrashedWorker,
                format!(
                    "Fission NIR worker exited with non-zero status {exit_status} after response{stderr_suffix}"
                ),
            ));
        }
        if let Some(error) = stdin_error {
            return Err(WorkerFailure::new(
                WorkerFailureKind::ProtocolFailure,
                format!(
                    "Fission NIR worker stdin write failed: worker answered success before consuming the whole request ({error}){stderr_suffix}"
                ),
            ));
        }
        if response.request_id.is_none() || response.address.is_none() {
            return Err(WorkerFailure::new(
                WorkerFailureKind::ProtocolFailure,
                format!(
                    "Fission NIR worker response request identity mismatch: success response must carry request_id {sent_request_id} and address 0x{:x}",
                    request.address
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
        // A rejection may legitimately carry no identity: the worker could not
        // parse the request (or it exceeded the worker's own size limit).
        Err(WorkerFailure::new(
            WorkerFailureKind::WorkerRejected,
            response
                .error
                .unwrap_or_else(|| "Fission NIR worker failed without error".to_string()),
        ))
    }
}

/// Worker process supervision: spawn, bounded concurrent pipe I/O under one
/// deadline, and teardown of the worker's whole process tree.
///
/// Invariant (both platforms): when `run` returns -- success, failure or
/// panic unwinding through it -- the direct child has been reaped, every
/// process still in the worker's process group (Unix) / job (Windows) has
/// been killed, and every pipe end the supervisor owned has been closed.
/// Unix additionally spawns no threads at all, so nothing can stay blocked
/// on a pipe that a surviving descendant keeps open.
///
/// Platform limitations, by design rather than by omission:
/// - Unix: a descendant that leaves the worker's process group (`setsid`,
///   `setpgid`) escapes the group kill. The supervisor still returns on time
///   and still closes its own pipe ends, but that escaped process is not
///   killed. Containing it needs cgroups / a subreaper, both host-global.
/// - Unix: the worker is in its own process group, so a terminal Ctrl-C (or
///   the host dying without unwinding) does not signal it. The orphan then
///   sees EOF/EPIPE on its pipes and exits when it next touches them.
/// - Windows: the worker is assigned to a kill-on-close Job Object right
///   after `spawn`; a descendant created in that window (the shipped worker
///   creates none) is outside the job. If the job cannot be created or
///   assigned, only the direct child is killed and I/O threads that stay
///   blocked past the cleanup grace are detached; the failure says so.
mod supervise {
    #[cfg(unix)]
    pub(super) use unix::run;

    #[cfg(not(unix))]
    pub(super) use threaded::run;

    #[cfg(unix)]
    mod unix {
        use super::super::{
            StderrCapture, WorkerFailure, WorkerFailureKind, WorkerRun, WorkerRunConfig,
            delayed_eof, response_too_large, spawn_failure, timeout_failure, wait_failure,
        };
        use crate::render::nir_diag_event;
        use std::io::{self, ErrorKind, Read, Write};
        use std::os::fd::{AsRawFd, RawFd};
        use std::os::unix::process::CommandExt;
        use std::process::{Child, Command, ExitStatus, Stdio};
        use std::time::{Duration, Instant};

        /// Upper bound on one poll(2) sleep while the worker is alive: exit is
        /// observed by polling waitid, not by a SIGCHLD handler (the host's
        /// signal disposition is not ours to change).
        const POLL_TICK: Duration = Duration::from_millis(5);
        const IO_CHUNK: usize = 64 * 1024;

        /// Owns the worker child and its process group (pgid == child pid).
        struct ProcessGroup {
            child: Child,
            pgid: libc::pid_t,
            reaped: Option<ExitStatus>,
        }

        impl ProcessGroup {
            /// Has the leader exited? Checked WITHOUT reaping it (`WNOWAIT`): an
            /// unreaped leader keeps its pid -- and therefore the group id --
            /// reserved, so the later `killpg` cannot hit a recycled group.
            fn leader_exited(&self) -> io::Result<bool> {
                loop {
                    // SAFETY: an all-zero siginfo_t is a valid plain C struct.
                    let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
                    // SAFETY: waitid on our own unreaped child with a valid out-pointer.
                    let rc = unsafe {
                        libc::waitid(
                            libc::P_PID,
                            self.pgid as libc::id_t,
                            &mut info,
                            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                        )
                    };
                    if rc == 0 {
                        // POSIX: with WNOHANG and no state change, si_signo is 0.
                        return Ok(info.si_signo != 0);
                    }
                    let error = io::Error::last_os_error();
                    if error.kind() != ErrorKind::Interrupted {
                        return Err(error);
                    }
                }
            }

            /// Kill every member of the worker's process group, then reap the
            /// leader. Idempotent; bounded (SIGKILL cannot be caught).
            fn terminate(&mut self) -> io::Result<ExitStatus> {
                if let Some(status) = self.reaped {
                    return Ok(status);
                }
                // The leader is not reaped yet, so `pgid` still names our group.
                // SAFETY: plain signal delivery to a process group we created.
                unsafe {
                    libc::killpg(self.pgid, libc::SIGKILL);
                }
                // The leader itself may have left its group; kill it directly too.
                let _ = self.child.kill();
                let status = self.child.wait()?;
                self.reaped = Some(status);
                Ok(status)
            }
        }

        impl Drop for ProcessGroup {
            fn drop(&mut self) {
                let _ = self.terminate();
            }
        }

        fn set_nonblocking(fd: RawFd) -> io::Result<()> {
            // SAFETY: fcntl on a descriptor this process owns.
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: as above; only O_NONBLOCK is added to our end of the pipe.
            if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        fn poll_ms(timeout: Duration) -> libc::c_int {
            // Round up so a sub-millisecond remainder does not spin.
            let ms = timeout.as_micros().div_ceil(1000);
            libc::c_int::try_from(ms).unwrap_or(libc::c_int::MAX)
        }

        enum Drain {
            Open,
            Eof,
        }

        /// Read everything currently available. `sink` returns `Err` to abort.
        fn drain(
            pipe: &mut impl Read,
            buf: &mut [u8],
            mut sink: impl FnMut(&[u8]) -> Result<(), WorkerFailure>,
        ) -> Result<Drain, WorkerFailure> {
            loop {
                match pipe.read(buf) {
                    Ok(0) => return Ok(Drain::Eof),
                    Ok(n) => sink(&buf[..n])?,
                    Err(e) if e.kind() == ErrorKind::WouldBlock => return Ok(Drain::Open),
                    Err(e) if e.kind() == ErrorKind::Interrupted => {}
                    Err(e) => {
                        return Err(WorkerFailure::new(
                            WorkerFailureKind::ProtocolFailure,
                            format!("Fission NIR worker stdout read failed: {e}"),
                        ));
                    }
                }
            }
        }

        pub(in super::super) fn run(
            config: &WorkerRunConfig,
            request: &[u8],
            address: u64,
            started_at: Instant,
            deadline: Option<Instant>,
        ) -> Result<WorkerRun, WorkerFailure> {
            let mut child = Command::new(&config.worker_path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                // Lets the worker detect a supervisor that died before the
                // worker armed its watchdog (`exit_when_supervisor_dies`).
                .env(
                    super::super::WORKER_PARENT_PID_ENV,
                    std::process::id().to_string(),
                )
                // New process group led by the worker: every descendant that
                // does not explicitly leave it dies with one killpg.
                .process_group(0)
                .spawn()
                .map_err(spawn_failure)?;
            let pipes = (child.stdin.take(), child.stdout.take(), child.stderr.take());
            let pgid = libc::pid_t::try_from(child.id())
                .map_err(|_| spawn_failure(io::Error::other("worker pid does not fit pid_t")))?;
            let mut group = ProcessGroup {
                child,
                pgid,
                reaped: None,
            };
            let (Some(stdin), Some(stdout), Some(stderr)) = pipes else {
                return Err(WorkerFailure::new(
                    WorkerFailureKind::ProtocolFailure,
                    "Fission NIR worker stdout unavailable",
                ));
            };
            for fd in [stdin.as_raw_fd(), stdout.as_raw_fd(), stderr.as_raw_fd()] {
                set_nonblocking(fd).map_err(|e| {
                    WorkerFailure::new(
                        WorkerFailureKind::WriteFailed,
                        format!("Fission NIR worker stdin unavailable: O_NONBLOCK: {e}"),
                    )
                })?;
            }

            let mut stdin = (!request.is_empty()).then_some(stdin);
            let mut written = 0usize;
            let mut stdin_error = None;
            let mut stdout = Some(stdout);
            let mut response = Vec::new();
            let mut stderr = Some(stderr);
            let mut diagnostics = StderrCapture::new(config.max_stderr_bytes);
            let mut exited_at: Option<Instant> = None;
            let mut buf = vec![0u8; IO_CHUNK];

            loop {
                if exited_at.is_none() && group.leader_exited().map_err(wait_failure)? {
                    exited_at = Some(Instant::now());
                    nir_diag_event(
                        address,
                        "worker_exit",
                        format!(
                            "elapsed_ms={:.1}",
                            started_at.elapsed().as_secs_f64() * 1000.0
                        ),
                    );
                }
                if exited_at.is_some() && stdin.is_none() && stdout.is_none() && stderr.is_none() {
                    break;
                }

                let now = Instant::now();
                let wake_at = match exited_at {
                    // After exit the deadline no longer applies; the bounded
                    // EOF grace does (cleanup has its own budget).
                    Some(at) => {
                        let limit = at + config.eof_grace;
                        if now >= limit {
                            let open: Vec<&str> = [
                                stdin.as_ref().map(|_| "stdin"),
                                stdout.as_ref().map(|_| "stdout"),
                                stderr.as_ref().map(|_| "stderr"),
                            ]
                            .into_iter()
                            .flatten()
                            .collect();
                            return Err(delayed_eof(&open, config.eof_grace));
                        }
                        Some(limit)
                    }
                    None => {
                        super::super::check_worker_cancellation(
                            stdin.is_some(),
                            config.timeout_ms,
                        )?;
                        if let Some(dl) = deadline
                            && now >= dl
                        {
                            nir_diag_event(
                                address,
                                "worker_timeout",
                                format!("budget_ms={}", config.timeout_ms),
                            );
                            return Err(timeout_failure(stdin.is_some(), config.timeout_ms));
                        }
                        deadline
                    }
                };
                let sleep = wake_at.map_or(POLL_TICK, |at| {
                    POLL_TICK.min(at.saturating_duration_since(now))
                });

                let mut fds = [libc::pollfd {
                    fd: -1,
                    events: 0,
                    revents: 0,
                }; 3];
                if let Some(pipe) = &stdin {
                    fds[0] = libc::pollfd {
                        fd: pipe.as_raw_fd(),
                        events: libc::POLLOUT,
                        revents: 0,
                    };
                }
                if let Some(pipe) = &stdout {
                    fds[1] = libc::pollfd {
                        fd: pipe.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    };
                }
                if let Some(pipe) = &stderr {
                    fds[2] = libc::pollfd {
                        fd: pipe.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    };
                }
                // Negative fds are ignored by poll(2), so closed pipes need no compaction.
                // SAFETY: `fds` is a valid array of 3 pollfd structs.
                let rc = unsafe { libc::poll(fds.as_mut_ptr(), 3, poll_ms(sleep)) };
                if rc < 0 {
                    let error = io::Error::last_os_error();
                    if error.kind() == ErrorKind::Interrupted {
                        continue;
                    }
                    return Err(WorkerFailure::new(
                        WorkerFailureKind::ProtocolFailure,
                        format!("Fission NIR worker stdout read failed: poll: {error}"),
                    ));
                }

                if fds[0].revents != 0
                    && let Some(pipe) = stdin.as_mut()
                {
                    loop {
                        let end = request.len().min(written + IO_CHUNK);
                        match pipe.write(&request[written..end]) {
                            Ok(n) => {
                                written += n;
                                if written == request.len() {
                                    // Close our end: the worker sees EOF.
                                    stdin = None;
                                    break;
                                }
                            }
                            Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                            Err(e) if e.kind() == ErrorKind::Interrupted => {}
                            Err(e) => {
                                // EPIPE: the worker closed stdin early. Recorded,
                                // judged against the response in finalize.
                                stdin_error = Some(format!(
                                    "{e} after {written} of {} request bytes",
                                    request.len()
                                ));
                                stdin = None;
                                break;
                            }
                        }
                    }
                }

                if fds[1].revents != 0
                    && let Some(pipe) = stdout.as_mut()
                {
                    let limit = config.max_response_bytes;
                    let state = drain(pipe, &mut buf, |chunk| {
                        let next = response.len().saturating_add(chunk.len());
                        if next > limit {
                            // Fail fast: never buffer past the limit.
                            return Err(response_too_large(limit, next));
                        }
                        response.extend_from_slice(chunk);
                        Ok(())
                    })?;
                    if let Drain::Eof = state {
                        stdout = None;
                    }
                }

                if fds[2].revents != 0
                    && let Some(pipe) = stderr.as_mut()
                {
                    // Diagnostics: a read error just ends the capture.
                    match drain(pipe, &mut buf, |chunk| {
                        diagnostics.push(chunk);
                        Ok(())
                    }) {
                        Ok(Drain::Open) => {}
                        Ok(Drain::Eof) | Err(_) => stderr = None,
                    }
                }
            }

            // Leader exited and every pipe reached EOF. Kill any group member
            // that is still around (it no longer holds our pipes) and reap.
            let exit_status = group.terminate().map_err(wait_failure)?;
            Ok(WorkerRun {
                exit_status,
                stdout: response,
                stderr: diagnostics,
                stdin_error,
            })
        }
    }

    #[cfg(not(unix))]
    mod threaded {
        use super::super::{
            StderrCapture, WorkerFailure, WorkerFailureKind, WorkerRun, WorkerRunConfig,
            delayed_eof, response_too_large, spawn_failure, timeout_failure, wait_failure,
        };
        use crate::render::nir_diag_event;
        use std::io::{self, ErrorKind, Read, Write};
        use std::process::{Child, Command, ExitStatus, Stdio};
        use std::sync::mpsc::{self, Receiver, TryRecvError};
        use std::thread::{self, JoinHandle};
        use std::time::{Duration, Instant};

        const POLL_TICK: Duration = Duration::from_millis(5);

        enum StdoutOutcome {
            Ok(Vec<u8>),
            TooLarge { limit: usize, observed: usize },
            Io(io::Error),
        }

        fn spawn_stdout_reader(
            mut pipe: impl Read + Send + 'static,
            limit: usize,
        ) -> (Receiver<StdoutOutcome>, JoinHandle<()>) {
            let (tx, rx) = mpsc::sync_channel(1);
            let handle = thread::spawn(move || {
                let mut out = Vec::new();
                let mut chunk = [0u8; 16 * 1024];
                let outcome = loop {
                    match pipe.read(&mut chunk) {
                        Ok(0) => break StdoutOutcome::Ok(out),
                        Ok(n) => {
                            let next = out.len().saturating_add(n);
                            if next > limit {
                                break StdoutOutcome::TooLarge {
                                    limit,
                                    observed: next,
                                };
                            }
                            out.extend_from_slice(&chunk[..n]);
                        }
                        Err(e) if e.kind() == ErrorKind::Interrupted => {}
                        Err(e) => break StdoutOutcome::Io(e),
                    }
                };
                let _ = tx.send(outcome);
            });
            (rx, handle)
        }

        fn spawn_stderr_reader(
            mut pipe: impl Read + Send + 'static,
            limit: usize,
        ) -> (Receiver<StderrCapture>, JoinHandle<()>) {
            let (tx, rx) = mpsc::sync_channel(1);
            let handle = thread::spawn(move || {
                let mut capture = StderrCapture::new(limit);
                let mut chunk = [0u8; 16 * 1024];
                loop {
                    match pipe.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => capture.push(&chunk[..n]),
                        Err(e) if e.kind() == ErrorKind::Interrupted => {}
                        Err(_) => break,
                    }
                }
                let _ = tx.send(capture);
            });
            (rx, handle)
        }

        fn spawn_stdin_writer(
            mut pipe: impl Write + Send + 'static,
            payload: Vec<u8>,
        ) -> (Receiver<io::Result<()>>, JoinHandle<()>) {
            let (tx, rx) = mpsc::sync_channel(1);
            let handle = thread::spawn(move || {
                let result = pipe.write_all(&payload).and_then(|()| pipe.flush());
                drop(pipe);
                let _ = tx.send(result);
            });
            (rx, handle)
        }

        /// Owns the worker child, its Job Object (Windows) and the I/O threads.
        struct ProcessTree {
            child: Child,
            #[cfg(windows)]
            job: Option<job::Job>,
            reaped: Option<ExitStatus>,
            threads: Vec<JoinHandle<()>>,
            cleanup_grace: Duration,
        }

        impl ProcessTree {
            fn kill_tree(&mut self) {
                #[cfg(windows)]
                if let Some(job) = &self.job {
                    job.terminate();
                }
                let _ = self.child.kill();
            }

            fn terminate(&mut self) -> io::Result<ExitStatus> {
                self.kill_tree();
                let status = match self.reaped {
                    Some(status) => status,
                    None => {
                        let status = self.child.wait()?;
                        self.reaped = Some(status);
                        status
                    }
                };
                // With the whole tree dead, every pipe peer is closed and the
                // I/O threads finish promptly; wait for them, bounded.
                let end = Instant::now() + self.cleanup_grace;
                while self.threads.iter().any(|h| !h.is_finished()) && Instant::now() < end {
                    thread::sleep(Duration::from_millis(2));
                }
                for handle in std::mem::take(&mut self.threads) {
                    if handle.is_finished() {
                        let _ = handle.join();
                    }
                    // else: blocked on a pipe held outside the job -- detached
                    // (documented limitation; see `supervise`).
                }
                Ok(status)
            }
        }

        impl Drop for ProcessTree {
            fn drop(&mut self) {
                let _ = self.terminate();
            }
        }

        pub(in super::super) fn run(
            config: &WorkerRunConfig,
            request: &[u8],
            address: u64,
            started_at: Instant,
            deadline: Option<Instant>,
        ) -> Result<WorkerRun, WorkerFailure> {
            let mut child = Command::new(&config.worker_path)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(spawn_failure)?;
            #[cfg(windows)]
            let job = job::Job::contain(&child).ok();
            let pipes = (child.stdin.take(), child.stdout.take(), child.stderr.take());
            let mut tree = ProcessTree {
                child,
                #[cfg(windows)]
                job,
                reaped: None,
                threads: Vec::with_capacity(3),
                cleanup_grace: config.eof_grace,
            };
            let (Some(stdin), Some(stdout), Some(stderr)) = pipes else {
                return Err(WorkerFailure::new(
                    WorkerFailureKind::ProtocolFailure,
                    "Fission NIR worker stdout unavailable",
                ));
            };
            let (stdout_rx, handle) = spawn_stdout_reader(stdout, config.max_response_bytes);
            tree.threads.push(handle);
            let (stderr_rx, handle) = spawn_stderr_reader(stderr, config.max_stderr_bytes);
            tree.threads.push(handle);
            let (stdin_rx, handle) = spawn_stdin_writer(stdin, request.to_vec());
            tree.threads.push(handle);

            let mut stdin_result: Option<Option<String>> = None;
            let mut stdout_bytes: Option<Vec<u8>> = None;
            let mut diagnostics: Option<StderrCapture> = None;
            let mut exited_at: Option<Instant> = None;

            loop {
                if stdin_result.is_none() {
                    match stdin_rx.try_recv() {
                        Ok(Ok(())) => stdin_result = Some(None),
                        Ok(Err(e)) => {
                            stdin_result =
                                Some(Some(format!("{e} ({} request bytes)", request.len())))
                        }
                        Err(TryRecvError::Disconnected) => {
                            stdin_result = Some(Some("writer thread exited unexpectedly".into()))
                        }
                        Err(TryRecvError::Empty) => {}
                    }
                }
                if stdout_bytes.is_none() {
                    match stdout_rx.try_recv() {
                        Ok(StdoutOutcome::Ok(bytes)) => stdout_bytes = Some(bytes),
                        Ok(StdoutOutcome::TooLarge { limit, observed }) => {
                            return Err(response_too_large(limit, observed));
                        }
                        Ok(StdoutOutcome::Io(e)) => {
                            return Err(WorkerFailure::new(
                                WorkerFailureKind::ProtocolFailure,
                                format!("Fission NIR worker stdout read failed: {e}"),
                            ));
                        }
                        Err(TryRecvError::Disconnected) => {
                            return Err(WorkerFailure::new(
                                WorkerFailureKind::ProtocolFailure,
                                "Fission NIR worker stdout read failed: reader disconnected",
                            ));
                        }
                        Err(TryRecvError::Empty) => {}
                    }
                }
                if diagnostics.is_none() {
                    match stderr_rx.try_recv() {
                        Ok(capture) => diagnostics = Some(capture),
                        Err(TryRecvError::Disconnected) => {
                            diagnostics = Some(StderrCapture::new(0))
                        }
                        Err(TryRecvError::Empty) => {}
                    }
                }
                if exited_at.is_none()
                    && let Some(status) = tree.child.try_wait().map_err(wait_failure)?
                {
                    tree.reaped = Some(status);
                    exited_at = Some(Instant::now());
                    nir_diag_event(
                        address,
                        "worker_exit",
                        format!(
                            "status={status} elapsed_ms={:.1}",
                            started_at.elapsed().as_secs_f64() * 1000.0
                        ),
                    );
                }
                if exited_at.is_some()
                    && stdin_result.is_some()
                    && stdout_bytes.is_some()
                    && diagnostics.is_some()
                {
                    break;
                }
                let now = Instant::now();
                match exited_at {
                    Some(at) if now >= at + config.eof_grace => {
                        let open: Vec<&str> = [
                            stdin_result.is_none().then_some("stdin"),
                            stdout_bytes.is_none().then_some("stdout"),
                            diagnostics.is_none().then_some("stderr"),
                        ]
                        .into_iter()
                        .flatten()
                        .collect();
                        return Err(delayed_eof(&open, config.eof_grace));
                    }
                    Some(_) => {}
                    None => {
                        super::super::check_worker_cancellation(
                            stdin_result.is_none(),
                            config.timeout_ms,
                        )?;
                        if let Some(dl) = deadline
                            && now >= dl
                        {
                            nir_diag_event(
                                address,
                                "worker_timeout",
                                format!("budget_ms={}", config.timeout_ms),
                            );
                            return Err(timeout_failure(stdin_result.is_none(), config.timeout_ms));
                        }
                    }
                }
                thread::sleep(POLL_TICK);
            }

            let exit_status = tree.terminate().map_err(wait_failure)?;
            Ok(WorkerRun {
                exit_status,
                stdout: stdout_bytes.unwrap_or_default(),
                stderr: diagnostics.unwrap_or_else(|| StderrCapture::new(0)),
                stdin_error: stdin_result.flatten(),
            })
        }

        /// Windows Job Object: every process the worker creates joins the job
        /// (no breakaway is granted), so one TerminateJobObject -- or closing
        /// the last handle, via KILL_ON_JOB_CLOSE -- ends the whole tree and
        /// with it every inherited pipe handle.
        #[cfg(windows)]
        mod job {
            use std::io;
            use std::os::windows::io::AsRawHandle;
            use std::process::Child;
            use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
            use windows_sys::Win32::System::JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject, TerminateJobObject,
            };

            pub(super) struct Job(HANDLE);

            impl Job {
                pub(super) fn contain(child: &Child) -> io::Result<Self> {
                    // SAFETY: null attributes/name create an anonymous job.
                    let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
                    if handle.is_null() {
                        return Err(io::Error::last_os_error());
                    }
                    let job = Self(handle);
                    let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                    // SAFETY: `info` is a valid JOBOBJECT_EXTENDED_LIMIT_INFORMATION
                    // of the size passed, for the matching information class.
                    let ok = unsafe {
                        SetInformationJobObject(
                            job.0,
                            JobObjectExtendedLimitInformation,
                            (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                        )
                    };
                    if ok == 0 {
                        return Err(io::Error::last_os_error());
                    }
                    // SAFETY: both handles are valid and owned for this call.
                    let ok = unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle()) };
                    if ok == 0 {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(job)
                }

                pub(super) fn terminate(&self) {
                    // SAFETY: valid job handle owned by `self`.
                    unsafe {
                        TerminateJobObject(self.0, 1);
                    }
                }
            }

            impl Drop for Job {
                fn drop(&mut self) {
                    // SAFETY: closing the handle we created; KILL_ON_JOB_CLOSE
                    // ends any process still in the job.
                    unsafe {
                        CloseHandle(self.0);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_WORKER_STDERR_BYTES, WorkerFailureKind, WorkerRunConfig,
        execute_nir_worker_request_with_config,
    };
    use crate::types::NirWorkerRequest;
    use crate::{NirRenderOptions, NirTypeContext};
    use std::path::PathBuf;
    use std::process::Command;
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

        /// Write a fixture worker and execute it once through a no-op guard.
        /// The first exec of a new executable is expensive on macOS (it is
        /// scanned: measured 1.0 s alone, up to 3.2 s with 15 in parallel),
        /// which would otherwise be charged to the supervisor's timeouts.
        fn write_executable(&self, name: &str, body: &str) -> PathBuf {
            use std::os::unix::fs::PermissionsExt;
            let path = self.dir.join(name);
            std::fs::write(
                &path,
                format!("#!/bin/sh\n[ -n \"$FISSION_WORKER_FIXTURE_WARM\" ] && exit 0\n{body}\n"),
            )
            .expect("write script");
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("chmod +x");
            let status = Command::new(&path)
                .env("FISSION_WORKER_FIXTURE_WARM", "1")
                .status()
                .expect("warm fixture");
            assert!(status.success(), "warm-up of {name} failed");
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

    /// Classic pipe deadlock shape: the worker writes a response larger than
    /// a pipe buffer *before* reading a request larger than a pipe buffer.
    /// Only concurrent transfer and drain can complete it.
    #[cfg(unix)]
    #[test]
    fn large_response_before_reading_large_request_does_not_deadlock() {
        let tmp = TempScriptDir::new("both_pipes_full");
        let chunk = "x".repeat(64);
        let script = tmp.write_executable(
            "eager.sh",
            &format!(
                "printf '{{\"request_id\":42,\"address\":5368713216,\"success\":true,\"code\":\"'\n\
                 i=0\nwhile [ $i -lt 3200 ]; do printf '%s' '{chunk}'; i=$((i + 1)); done\n\
                 printf '\"}}'\ncat >/dev/null"
            ),
        );
        let mut req = sample_request(1024 * 1024);
        req.request_id = Some(42);
        let (code, _, _) = execute_nir_worker_request_with_config(
            &req,
            &WorkerRunConfig::with_path(script, 20_000),
        )
        .expect("concurrent transfer and drain must not deadlock");
        assert_eq!(code.len(), 3200 * 64);
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

    /// Process-wide resource probes. Counts are only meaningful in a process
    /// that runs nothing else, so the tests below re-execute this test binary
    /// on a single test (`run_isolated`).
    #[cfg(unix)]
    mod resources {
        pub(super) fn open_fd_count() -> usize {
            std::fs::read_dir("/dev/fd").expect("list /dev/fd").count()
        }

        #[cfg(target_os = "linux")]
        pub(super) fn thread_count() -> usize {
            let status = std::fs::read_to_string("/proc/self/status").expect("read status");
            status
                .lines()
                .find_map(|line| line.strip_prefix("Threads:"))
                .and_then(|value| value.trim().parse().ok())
                .expect("Threads: line")
        }

        #[cfg(target_os = "macos")]
        pub(super) fn thread_count() -> usize {
            // SAFETY: an all-zero proc_taskinfo is a valid value of a plain C struct.
            let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
            let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
            // SAFETY: `info` is a writable buffer of exactly `size` bytes.
            let written = unsafe {
                libc::proc_pidinfo(
                    libc::getpid(),
                    libc::PROC_PIDTASKINFO,
                    0,
                    (&mut info as *mut libc::proc_taskinfo).cast(),
                    size,
                )
            };
            assert_eq!(written, size, "proc_pidinfo(PROC_PIDTASKINFO) failed");
            info.pti_threadnum as usize
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        pub(super) fn thread_count() -> usize {
            0
        }

        /// True while `pid` exists and is not a zombie awaiting its reaper.
        pub(super) fn process_alive(pid: i32) -> bool {
            // SAFETY: signal 0 performs only an existence/permission check.
            if unsafe { libc::kill(pid, 0) } != 0 {
                return false;
            }
            #[cfg(target_os = "linux")]
            if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat"))
                && let Some(state) = stat
                    .rsplit(')')
                    .next()
                    .and_then(|rest| rest.split_whitespace().next())
            {
                return state != "Z";
            }
            true
        }

        /// True if this process has any child, live or unreaped.
        pub(super) fn has_children() -> bool {
            let mut status = 0;
            // SAFETY: non-blocking status query over this process's own children.
            let rc = unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) };
            rc != -1
        }

        pub(super) fn kill(pid: i32) {
            // SAFETY: plain signal delivery to a pid this test recorded itself.
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
        }
    }

    #[cfg(unix)]
    const RESOURCE_PROBE_ENV: &str = "FISSION_WORKER_RESOURCE_PROBE";

    /// Re-run test `name` alone in a fresh copy of this test binary, so the
    /// thread/fd/child counts it takes are not disturbed by parallel tests.
    #[cfg(unix)]
    fn run_isolated(name: &str) {
        let test_name = format!(
            "{}::{name}",
            module_path!()
                .split_once("::")
                .map_or(module_path!(), |(_, rest)| rest)
        );
        let exe = std::env::current_exe().expect("current test binary");
        let out = Command::new(exe)
            .args([&test_name, "--exact", "--nocapture", "--test-threads=1"])
            .env(RESOURCE_PROBE_ENV, "1")
            .output()
            .expect("re-run test binary");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        eprintln!("--- isolated {test_name} ---\n{stdout}{stderr}");
        assert!(
            out.status.success(),
            "isolated probe {test_name} failed:\n{stdout}\n{stderr}"
        );
        assert!(
            stdout.contains("1 passed"),
            "isolated probe {test_name} did not run exactly one test:\n{stdout}"
        );
    }

    #[cfg(unix)]
    fn read_pid(path: &std::path::Path) -> Option<i32> {
        std::fs::read_to_string(path).ok()?.trim().parse().ok()
    }

    #[cfg(unix)]
    fn recorded_pids(dir: &std::path::Path, prefix: &str) -> Vec<i32> {
        let mut pids: Vec<i32> = std::fs::read_dir(dir)
            .expect("list fixture dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
            .filter_map(|entry| read_pid(&entry.path()))
            .collect();
        pids.sort_unstable();
        pids
    }

    #[cfg(unix)]
    fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
        let end = Instant::now() + limit;
        loop {
            if done() {
                return true;
            }
            if Instant::now() >= end {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Repeated misbehaving workers whose descendant inherits the IPC pipes
    /// and outlives them: the supervisor must return within its budget AND
    /// leave no thread, descriptor, child or pipe-holding descendant behind.
    #[cfg(unix)]
    #[test]
    fn repeated_pipe_holding_descendants_leave_no_threads_fds_or_processes() {
        if std::env::var_os(RESOURCE_PROBE_ENV).is_none() {
            run_isolated("repeated_pipe_holding_descendants_leave_no_threads_fds_or_processes");
            return;
        }
        // FISSION_WORKER_RESOURCE_ROUNDS raises the count for manual measurement.
        let rounds: usize = std::env::var("FISSION_WORKER_RESOURCE_ROUNDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(12);
        let tmp = TempScriptDir::new("resource_probe");
        let ok_json = r#"{"request_id":42,"address":5368713216,"success":true,"code":"int f(){}"}"#;

        // Warm-up: a well-behaved worker, so one-time lazy state is not counted.
        let ok = tmp.write_executable("ok.sh", &format!("cat >/dev/null\nprintf '{ok_json}'"));
        let mut req = sample_request(16);
        req.request_id = Some(42);
        execute_nir_worker_request_with_config(&req, &WorkerRunConfig::with_path(ok, 5_000))
            .expect("warm-up worker succeeds");

        // Each descendant records its pid in holder_<worker pid>.pid.
        let d = tmp.dir.display();
        // 0: worker never reads and never exits; the descendant holds stdin,
        //    stdout and stderr open (blocked transfer).
        // 1: worker answers and exits 0; the descendant keeps stdout/stderr.
        // 2: worker exits 3 at once; the descendant keeps every pipe.
        let scenarios = [
            (
                tmp.write_executable(
                    "hang.sh",
                    &format!("sleep 30 <&0 &\necho $! > '{d}/holder_'$$.pid\nsleep 30"),
                ),
                1024 * 1024,
                WorkerFailureKind::TransferTimeout,
            ),
            (
                tmp.write_executable(
                    "answer_then_exit.sh",
                    &format!(
                        "cat >/dev/null\nsleep 30 &\necho $! > '{d}/holder_'$$.pid\nprintf '{ok_json}'\nexit 0"
                    ),
                ),
                16,
                WorkerFailureKind::CleanupFailure,
            ),
            (
                tmp.write_executable(
                    "exit_at_once.sh",
                    &format!("sleep 30 <&0 &\necho $! > '{d}/holder_'$$.pid\nexit 3"),
                ),
                1024 * 1024,
                WorkerFailureKind::CleanupFailure,
            ),
        ];

        let base_threads = resources::thread_count();
        let base_fds = resources::open_fd_count();
        let mut worst = Duration::ZERO;
        for round in 0..rounds {
            let (script, request_len, expected) = &scenarios[round % scenarios.len()];
            let mut config = WorkerRunConfig::with_path(script.clone(), 300);
            config.eof_grace = Duration::from_millis(100);
            let mut req = sample_request(*request_len);
            req.request_id = Some(42);

            let started = Instant::now();
            let err = execute_nir_worker_request_with_config(&req, &config)
                .expect_err("misbehaving worker must not publish success");
            let elapsed = started.elapsed();
            worst = worst.max(elapsed);
            assert_eq!(err.kind, *expected, "round {round}: {}", err.message);
            assert!(
                elapsed < Duration::from_secs(2),
                "round {round}: supervisor took {elapsed:?}"
            );
        }
        let holders = recorded_pids(&tmp.dir, "holder_");
        assert_eq!(
            holders.len(),
            rounds,
            "every fixture recorded its descendant"
        );

        let threads_ok = wait_until(Duration::from_secs(2), || {
            resources::thread_count() <= base_threads
        });
        let fds_ok = wait_until(Duration::from_secs(2), || {
            resources::open_fd_count() <= base_fds
        });
        let holders_dead = wait_until(Duration::from_secs(3), || {
            holders.iter().all(|&pid| !resources::process_alive(pid))
        });
        let after_threads = resources::thread_count();
        let after_fds = resources::open_fd_count();
        let alive = holders
            .iter()
            .filter(|&&pid| resources::process_alive(pid))
            .count();
        let children = resources::has_children();
        println!(
            "resource probe: rounds={rounds} threads {base_threads}->{after_threads} \
             fds {base_fds}->{after_fds} pipe_holders_alive={alive}/{rounds} \
             unreaped_children={children} worst_call={worst:?}"
        );
        for &pid in &holders {
            resources::kill(pid);
        }
        assert!(
            threads_ok,
            "threads leaked: {base_threads} -> {after_threads}"
        );
        assert!(fds_ok, "descriptors leaked: {base_fds} -> {after_fds}");
        assert!(holders_dead, "{alive} pipe-holding descendants survived");
        assert!(!children, "worker children left unreaped");
    }

    /// A descendant that leaves the worker's process group (setsid) cannot be
    /// killed by a group signal. The supervisor must still return on time and
    /// reclaim every thread and descriptor it owns; the escaped process itself
    /// is a documented limitation and is killed here by the test.
    #[cfg(unix)]
    #[test]
    fn descendant_escaping_the_process_group_still_leaves_no_threads_or_fds() {
        if std::env::var_os(RESOURCE_PROBE_ENV).is_none() {
            run_isolated("descendant_escaping_the_process_group_still_leaves_no_threads_or_fds");
            return;
        }
        if Command::new("perl")
            .args(["-MPOSIX", "-e", "exit 0"])
            .status()
            .map_or(true, |s| !s.success())
        {
            println!("SKIP: perl with POSIX unavailable");
            return;
        }
        const ROUNDS: usize = 4;
        let tmp = TempScriptDir::new("escape_probe");
        let base_threads = resources::thread_count();
        let base_fds = resources::open_fd_count();
        let script = tmp.write_executable(
            "escape.sh",
            &format!(
                "perl -MPOSIX -e 'POSIX::setsid(); sleep 30' <&0 &\necho $! > '{}/escaped_'$$.pid\nsleep 30",
                tmp.dir.display()
            ),
        );
        for round in 0..ROUNDS {
            let mut config = WorkerRunConfig::with_path(script.clone(), 300);
            config.eof_grace = Duration::from_millis(100);
            let started = Instant::now();
            let err = execute_nir_worker_request_with_config(&sample_request(1024 * 1024), &config)
                .expect_err("hung worker must time out");
            assert_eq!(
                err.kind,
                WorkerFailureKind::TransferTimeout,
                "round {round}: {}",
                err.message
            );
            assert!(started.elapsed() < Duration::from_secs(2));
        }
        let escaped = recorded_pids(&tmp.dir, "escaped_");
        let threads_ok = wait_until(Duration::from_secs(2), || {
            resources::thread_count() <= base_threads
        });
        let fds_ok = wait_until(Duration::from_secs(2), || {
            resources::open_fd_count() <= base_fds
        });
        let after_threads = resources::thread_count();
        let after_fds = resources::open_fd_count();
        let alive = escaped
            .iter()
            .filter(|&&pid| resources::process_alive(pid))
            .count();
        println!(
            "escape probe: rounds={ROUNDS} threads {base_threads}->{after_threads} \
             fds {base_fds}->{after_fds} escaped_alive={alive}/{}",
            escaped.len()
        );
        for &pid in &escaped {
            resources::kill(pid);
        }
        assert!(
            threads_ok,
            "threads leaked: {base_threads} -> {after_threads}"
        );
        assert!(fds_ok, "descriptors leaked: {base_fds} -> {after_fds}");
    }

    /// stderr is diagnostics: the capture is bounded, but the pipe must keep
    /// draining, or a chatty worker blocks/SIGPIPEs on a full stderr pipe.
    #[cfg(unix)]
    #[test]
    fn stderr_beyond_capture_limit_is_drained_not_fatal() {
        let tmp = TempScriptDir::new("stderr_flood");
        let line = "x".repeat(63);
        let script = tmp.write_executable(
            "chatty.sh",
            &format!(
                "cat >/dev/null\ni=0\nwhile [ $i -lt 2048 ]; do echo '{line}' >&2; i=$((i + 1)); done\n\
                 printf '{{\"request_id\":42,\"address\":5368713216,\"success\":true,\"code\":\"ok\"}}'"
            ),
        );
        let mut config = WorkerRunConfig::with_path(script, 10_000);
        config.max_stderr_bytes = 4096;
        let mut req = sample_request(16);
        req.request_id = Some(42);
        let (code, _, _) = execute_nir_worker_request_with_config(&req, &config)
            .expect("stderr overflow must not fail an otherwise valid worker");
        assert_eq!(code, "ok");

        // On failure, the bounded stderr prefix is still reported.
        let crash = tmp.write_executable(
            "chatty_crash.sh",
            &format!(
                "cat >/dev/null\necho 'first diagnostic' >&2\ni=0\nwhile [ $i -lt 512 ]; do echo '{line}' >&2; i=$((i + 1)); done\nexit 1"
            ),
        );
        let mut config = WorkerRunConfig::with_path(crash, 10_000);
        config.max_stderr_bytes = 1024;
        let err =
            execute_nir_worker_request_with_config(&req, &config).expect_err("crash must fail");
        assert_eq!(
            err.kind,
            WorkerFailureKind::CrashedWorker,
            "{}",
            err.message
        );
        assert!(err.message.contains("first diagnostic"), "{}", err.message);
        assert!(err.message.contains("truncated"), "{}", err.message);
        assert!(err.message.len() < 4096, "stderr in message is unbounded");
    }

    /// A success response must name the request it answers.
    #[cfg(unix)]
    #[test]
    fn success_response_without_request_identity_is_not_published() {
        let tmp = TempScriptDir::new("no_identity");
        let script = tmp.write_executable(
            "anonymous.sh",
            "cat >/dev/null\nprintf '{\"success\":true,\"code\":\"int f(){}\"}'",
        );
        let err = execute_nir_worker_request_with_config(
            &sample_request(16),
            &WorkerRunConfig::with_path(script, 5_000),
        )
        .expect_err("anonymous success must be rejected");
        assert_eq!(
            err.kind,
            WorkerFailureKind::ProtocolFailure,
            "{}",
            err.message
        );
        assert!(err.message.contains("identity"), "{}", err.message);
    }

    /// A worker that stopped reading the request cannot have analysed it, so a
    /// success answer (even with the right identity) must not be published.
    #[cfg(unix)]
    #[test]
    fn success_after_partial_request_consumption_is_not_published() {
        let tmp = TempScriptDir::new("partial_request");
        let script = tmp.write_executable(
            "partial.sh",
            "head -c 16 >/dev/null\nprintf '{\"request_id\":42,\"address\":5368713216,\"success\":true,\"code\":\"ok\"}'\nexit 0",
        );
        let mut req = sample_request(1024 * 1024);
        req.request_id = Some(42);
        let err = execute_nir_worker_request_with_config(
            &req,
            &WorkerRunConfig::with_path(script, 5_000),
        )
        .expect_err("success over an unread request must be rejected");
        assert_eq!(
            err.kind,
            WorkerFailureKind::ProtocolFailure,
            "{}",
            err.message
        );
        assert!(err.message.contains("request"), "{}", err.message);
    }

    #[cfg(unix)]
    const SPIN_PIDFILE_ENV: &str = "FISSION_WORKER_TEST_SPIN_PIDFILE";
    #[cfg(unix)]
    const SUPERVISOR_SCRIPT_ENV: &str = "FISSION_WORKER_TEST_SUPERVISOR_SCRIPT";

    /// The worker leads its own process group, so terminal Ctrl-C and host
    /// death do not signal it. A CPU-bound worker that never touches its
    /// pipes must still notice that its supervisor is gone and exit.
    ///
    /// Three roles of this one test, chosen by environment:
    /// - outer: spawns a supervisor process, SIGKILLs it, watches the worker;
    /// - supervisor: runs the real supervisor against a spinning worker;
    /// - worker: calls the worker binaries' startup hook, then spins.
    #[cfg(unix)]
    #[test]
    fn cpu_bound_worker_exits_when_its_supervisor_is_killed() {
        const NAME: &str = "cpu_bound_worker_exits_when_its_supervisor_is_killed";
        if let Some(pidfile) = std::env::var_os(SPIN_PIDFILE_ENV) {
            super::exit_when_supervisor_dies();
            std::fs::write(&pidfile, std::process::id().to_string()).expect("write pidfile");
            let mut x = 0u64;
            loop {
                x = std::hint::black_box(x.wrapping_add(1));
            }
        }
        if let Some(script) = std::env::var_os(SUPERVISOR_SCRIPT_ENV) {
            let _ = execute_nir_worker_request_with_config(
                &sample_request(16),
                &WorkerRunConfig::with_path(PathBuf::from(script), 120_000),
            );
            panic!("supervisor returned before it was killed");
        }

        let tmp = TempScriptDir::new("orphan");
        let pidfile = tmp.dir.join("worker.pid");
        let exe = std::env::current_exe().expect("test binary");
        let test_name = format!(
            "{}::{NAME}",
            module_path!()
                .split_once("::")
                .map_or(module_path!(), |(_, rest)| rest)
        );
        let script = tmp.write_executable(
            "spin_worker.sh",
            &format!(
                "{SPIN_PIDFILE_ENV}='{}' exec '{}' '{test_name}' --exact --nocapture --test-threads=1",
                pidfile.display(),
                exe.display()
            ),
        );
        let mut supervisor = Command::new(&exe)
            .args([&test_name, "--exact", "--nocapture", "--test-threads=1"])
            .env(SUPERVISOR_SCRIPT_ENV, &script)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn supervisor");
        let started = wait_until(Duration::from_secs(20), || read_pid(&pidfile).is_some());
        let worker = read_pid(&pidfile);
        if !started || worker.is_none() {
            let _ = supervisor.kill();
            let _ = supervisor.wait();
            panic!("spinning worker never started");
        }
        let worker = worker.unwrap_or_default();
        assert!(
            resources::process_alive(worker),
            "worker alive before the kill"
        );

        supervisor.kill().expect("SIGKILL supervisor");
        supervisor.wait().expect("reap supervisor");
        let killed_at = Instant::now();
        let gone = wait_until(Duration::from_secs(3), || !resources::process_alive(worker));
        let elapsed = killed_at.elapsed();
        println!("orphan probe: worker {worker} gone={gone} after {elapsed:?}");
        if !gone {
            resources::kill(worker);
        }
        assert!(
            gone,
            "CPU-bound worker outlived its SIGKILLed supervisor by > 3 s"
        );
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

    #[cfg(unix)]
    #[test]
    fn cancelling_active_token_kills_slow_worker_subprocess_promptly() {
        let tmp = TempScriptDir::new("cancel_worker");
        let pid_file = tmp.dir.join("worker.pid");
        let script = tmp.write_executable(
            "slow_worker.sh",
            &format!(
                "echo $$ > \"{}\"\nsleep 30\necho '{{\"success\":true}}' ",
                pid_file.display()
            ),
        );
        let config = WorkerRunConfig::with_path(script, 10_000);
        let req = sample_request(16);
        let token = fission_core::CancellationToken::new();
        let token_canceller = token.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            token_canceller.cancel();
        });

        let started = Instant::now();
        let err = fission_core::cancellation::with_cancellation_scope(token, || {
            execute_nir_worker_request_with_config(&req, &config)
        })
        .expect_err("cancelled worker must fail");
        let elapsed = started.elapsed();

        assert_eq!(err.kind, WorkerFailureKind::Cancelled);
        assert!(err.message.contains("nir_cancelled"), "{}", err.message);
        assert!(
            elapsed < Duration::from_secs(3),
            "expected prompt cancellation kill, took {elapsed:?}"
        );
        if let Ok(pid_text) = std::fs::read_to_string(&pid_file)
            && let Ok(pid) = pid_text.trim().parse::<i32>()
        {
            assert!(
                !wait_until(Duration::from_millis(10), || false)
                    && wait_until(Duration::from_secs(2), || {
                        unsafe { libc::kill(pid, 0) != 0 }
                    }),
                "cancelled worker pid {pid} should be reaped"
            );
        }
    }
}

/// Environment variable through which the supervisor tells a worker its pid,
/// so the worker can detect a supervisor that died before the worker started.
pub const WORKER_PARENT_PID_ENV: &str = "FISSION_WORKER_PARENT_PID";

/// Exit code of a worker that found its supervisor gone (nobody reads it).
#[cfg(unix)]
const ORPHANED_WORKER_EXIT_CODE: libc::c_int = 75;

/// Called first thing by the worker binaries: exit as soon as the supervising
/// process is gone.
///
/// The supervisor puts each worker in its own process group (see
/// `supervise`), so terminal Ctrl-C and host death no longer reach it as a
/// signal; a CPU-bound worker that never touches its pipes would otherwise
/// keep running as an orphan. Unix: a watchdog thread polls `getppid()` every
/// 100 ms and `_exit`s once the parent changes (re-parenting to init or a
/// subreaper is the only way it changes); Linux additionally asks the kernel
/// for SIGKILL on parent death (`PR_SET_PDEATHSIG`; the spawning supervisor
/// thread blocks until the worker is reaped, so thread-exit semantics do not
/// fire early). Windows needs nothing here: the worker is in a
/// kill-on-close Job Object whose only handle the supervisor holds, so the
/// supervisor's death closes it and the job kills the worker.
pub fn exit_when_supervisor_dies() {
    #[cfg(unix)]
    {
        // SAFETY: getppid has no preconditions and cannot fail.
        let current = unsafe { libc::getppid() };
        let expected = std::env::var(WORKER_PARENT_PID_ENV)
            .ok()
            .and_then(|pid| pid.parse::<libc::pid_t>().ok())
            .unwrap_or(current);
        #[cfg(target_os = "linux")]
        // SAFETY: PR_SET_PDEATHSIG takes a signal number; no pointers involved.
        unsafe {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
        }
        let orphaned = move || {
            // SAFETY: as above.
            unsafe { libc::getppid() != expected }
        };
        // Covers a supervisor that died before this point (including before
        // PR_SET_PDEATHSIG was armed).
        if orphaned() {
            // SAFETY: immediate process exit; nothing to unwind or flush.
            unsafe { libc::_exit(ORPHANED_WORKER_EXIT_CODE) };
        }
        let _ = std::thread::Builder::new()
            .name("supervisor-watchdog".into())
            .spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_millis(100));
                    if orphaned() {
                        // SAFETY: as above. `_exit`, not `exit`: another thread
                        // may hold the stdout lock or be mid-allocation.
                        unsafe { libc::_exit(ORPHANED_WORKER_EXIT_CODE) };
                    }
                }
            });
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
