//! Out-of-process external SDK worker runner with deny-by-default capability
//! authorization, bounded stdio JSON-RPC framing, timeouts, and honest OS
//! confinement reporting (#1173).

use crate::security::{
    AuthorizationError, CapabilitySet, GrantId, PluginCapability, PluginTrustTier,
    SessionAuthorizer, SessionId,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

/// Resource and framing bounds enforced around an external worker process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalWorkerLimits {
    pub request_timeout: Duration,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    pub cpu_time_limit_secs: Option<u64>,
    pub max_open_files: Option<u64>,
    pub max_file_size_bytes: Option<u64>,
    pub address_space_limit_bytes: Option<u64>,
}

impl Default for ExternalWorkerLimits {
    fn default() -> Self {
        Self {
            request_timeout: Duration::from_secs(5),
            max_request_bytes: 256 * 1024,
            max_response_bytes: 256 * 1024,
            cpu_time_limit_secs: Some(30),
            max_open_files: Some(128),
            max_file_size_bytes: Some(0),
            address_space_limit_bytes: None,
        }
    }
}

/// Explicit report of what confinement the host enforced (and what it could not).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsConfinementReport {
    pub platform: String,
    pub clean_environment: bool,
    pub stdio_only_transport: bool,
    pub enforced_rlimits: Vec<String>,
    pub kernel_sandbox_active: bool,
    pub unsupported_protections: Vec<String>,
}

impl OsConfinementReport {
    #[must_use]
    pub fn for_limits(limits: &ExternalWorkerLimits) -> Self {
        #[allow(unused_mut)]
        let mut enforced_rlimits = Vec::new();
        #[cfg(unix)]
        {
            if let Some(cpu) = limits.cpu_time_limit_secs {
                enforced_rlimits.push(format!("RLIMIT_CPU={cpu}s"));
            }
            if let Some(nofile) = limits.max_open_files {
                enforced_rlimits.push(format!("RLIMIT_NOFILE={nofile}"));
            }
            if let Some(fsize) = limits.max_file_size_bytes {
                enforced_rlimits.push(format!("RLIMIT_FSIZE={fsize}"));
            }
            if let Some(as_bytes) = limits.address_space_limit_bytes {
                enforced_rlimits.push(format!("RLIMIT_AS={as_bytes}"));
            }
        }
        #[cfg(not(unix))]
        let _ = limits;

        let mut unsupported_protections = vec![
            "kernel mandatory access control sandbox (seccomp/Landlock/Seatbelt) not active"
                .to_string(),
        ];
        if !cfg!(unix) {
            unsupported_protections
                .push("POSIX setrlimit enforcement unavailable on this OS".into());
        }

        Self {
            platform: std::env::consts::OS.to_string(),
            clean_environment: true,
            stdio_only_transport: true,
            enforced_rlimits,
            kernel_sandbox_active: false,
            unsupported_protections,
        }
    }
}

/// Launch configuration for an isolated external worker.
#[derive(Debug, Clone)]
pub struct ExternalWorkerConfig {
    pub plugin_id: String,
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub working_dir: PathBuf,
    pub allowed_env: BTreeMap<String, String>,
    pub capabilities: CapabilitySet,
    pub limits: ExternalWorkerLimits,
    /// If `true`, fail closed when kernel-level MAC sandboxing is unavailable.
    pub require_kernel_sandbox: bool,
}

impl ExternalWorkerConfig {
    #[must_use]
    pub fn new(plugin_id: impl Into<String>, executable: impl Into<PathBuf>) -> Self {
        let executable = executable.into();
        let working_dir = executable
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        Self {
            plugin_id: plugin_id.into(),
            executable,
            args: Vec::new(),
            working_dir,
            allowed_env: BTreeMap::new(),
            capabilities: CapabilitySet::none(),
            limits: ExternalWorkerLimits::default(),
            require_kernel_sandbox: false,
        }
    }
}

use std::path::Path;

/// Structured host operation requested by an external worker over stdio RPC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum HostOperation {
    ReadAnalysis { query: String },
    EditProject { target: String, value: String },
    MutateLiveExecution { action: String, address: u64 },
}

impl HostOperation {
    #[must_use]
    pub const fn required_capability(&self) -> PluginCapability {
        match self {
            Self::ReadAnalysis { .. } => PluginCapability::ReadAnalysis,
            Self::EditProject { .. } => PluginCapability::EditProject,
            Self::MutateLiveExecution { .. } => PluginCapability::MutateLiveExecution,
        }
    }
}

/// Host handler invoked only after [`SessionAuthorizer`] authorizes the request.
pub trait HostOperationHandler {
    fn handle(&mut self, op: &HostOperation) -> Result<serde_json::Value, String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerError {
    UnsupportedIsolation(String),
    SpawnFailed(String),
    PayloadTooLarge { limit: usize, actual: usize },
    Timeout(Duration),
    Cancelled,
    Crashed(String),
    ProtocolError(String),
    WorkerReturnedError(String),
    Authorization(AuthorizationError),
}

impl fmt::Display for WorkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedIsolation(msg) => write!(f, "unsupported OS isolation: {msg}"),
            Self::SpawnFailed(msg) => write!(f, "failed to spawn external worker: {msg}"),
            Self::PayloadTooLarge { limit, actual } => write!(
                f,
                "worker IPC payload ({actual} bytes) exceeded limit ({limit} bytes)"
            ),
            Self::Timeout(dur) => write!(f, "worker request timed out after {dur:?}"),
            Self::Cancelled => write!(f, "worker session cancelled"),
            Self::Crashed(msg) => write!(f, "external worker terminated unexpectedly: {msg}"),
            Self::ProtocolError(msg) => write!(f, "external worker protocol error: {msg}"),
            Self::WorkerReturnedError(msg) => write!(f, "external worker error: {msg}"),
            Self::Authorization(err) => write!(f, "authorization error: {err}"),
        }
    }
}

impl std::error::Error for WorkerError {}

#[derive(Serialize)]
struct HostRequestEnvelope<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    id: u64,
    session_id: u64,
    method: &'a str,
    params: &'a serde_json::Value,
}

#[derive(Serialize)]
struct HostResultEnvelope<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    id: u64,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WorkerMessage {
    HostCall {
        id: u64,
        #[serde(default)]
        claimed_session: Option<u64>,
        operation: HostOperation,
    },
    Response {
        id: u64,
        ok: bool,
        #[serde(default)]
        result: Option<serde_json::Value>,
        #[serde(default)]
        error: Option<String>,
    },
}

enum ReaderEvent {
    Line(String),
    PayloadTooLarge(usize),
    Eof,
    IoError(String),
}

fn spawn_bounded_reader(
    mut stdout: ChildStdout,
    max_line_bytes: usize,
) -> (Receiver<ReaderEvent>, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        let mut buf: Vec<u8> = Vec::with_capacity(4096);
        let mut chunk = [0u8; 4096];
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) => {
                    if !buf.is_empty() {
                        if buf.len() > max_line_bytes {
                            let _ = tx.send(ReaderEvent::PayloadTooLarge(buf.len()));
                        } else if let Ok(line) = String::from_utf8(std::mem::take(&mut buf)) {
                            let _ = tx.send(ReaderEvent::Line(line));
                        } else {
                            let _ = tx.send(ReaderEvent::IoError("non-UTF-8 worker output".into()));
                        }
                    }
                    let _ = tx.send(ReaderEvent::Eof);
                    break;
                }
                Ok(n) => {
                    for &b in &chunk[..n] {
                        if b == b'\n' {
                            let line_bytes = std::mem::take(&mut buf);
                            match String::from_utf8(line_bytes) {
                                Ok(line) => {
                                    if tx.send(ReaderEvent::Line(line)).is_err() {
                                        return;
                                    }
                                }
                                Err(_) => {
                                    let _ = tx.send(ReaderEvent::IoError(
                                        "non-UTF-8 worker output".into(),
                                    ));
                                    return;
                                }
                            }
                        } else {
                            if buf.len() >= max_line_bytes {
                                let _ = tx.send(ReaderEvent::PayloadTooLarge(buf.len() + 1));
                                return;
                            }
                            buf.push(b);
                        }
                    }
                }
                Err(err) => {
                    let _ = tx.send(ReaderEvent::IoError(err.to_string()));
                    break;
                }
            }
        }
    });
    (rx, handle)
}

#[cfg(unix)]
unsafe fn apply_rlimit(resource: libc::c_int, soft: u64) -> std::io::Result<()> {
    // SAFETY: Caller invokes this inside `pre_exec`; `getrlimit`/`setrlimit` are
    // async-signal-safe and `cur` is initialized cleanly by `getrlimit` on success.
    unsafe {
        let mut cur: libc::rlimit = std::mem::zeroed();
        if libc::getrlimit(resource, &mut cur) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let target = (soft as libc::rlim_t).min(cur.rlim_max);
        let lim = libc::rlimit {
            rlim_cur: target,
            rlim_max: target,
        };
        if libc::setrlimit(resource, &lim) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}

/// Active host-controlled session for one [`PluginTrustTier::IsolatedExternalWorker`].
///
/// Dropping or cancelling the session immediately revokes its [`SessionId`] grants
/// and terminates and reaps the child worker process.
pub struct ExternalWorkerSession {
    session_id: SessionId,
    grant_id: GrantId,
    authorizer: Arc<Mutex<SessionAuthorizer>>,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    rx: Receiver<ReaderEvent>,
    reader_thread: Option<JoinHandle<()>>,
    limits: ExternalWorkerLimits,
    confinement: OsConfinementReport,
    next_request_id: u64,
    cancelled: bool,
}

impl ExternalWorkerSession {
    pub fn spawn(
        config: ExternalWorkerConfig,
        authorizer: Arc<Mutex<SessionAuthorizer>>,
    ) -> Result<Self, WorkerError> {
        let confinement = OsConfinementReport::for_limits(&config.limits);
        if config.require_kernel_sandbox && !confinement.kernel_sandbox_active {
            return Err(WorkerError::UnsupportedIsolation(
                confinement.unsupported_protections.join("; "),
            ));
        }

        let (session_id, grant_id) = {
            let mut auth = authorizer
                .lock()
                .map_err(|_| WorkerError::Crashed("authorizer lock poisoned".into()))?;
            let sid = auth.new_session();
            let gid = auth
                .issue_grant(
                    sid,
                    config.plugin_id.clone(),
                    PluginTrustTier::IsolatedExternalWorker,
                    config.capabilities.clone(),
                )
                .map_err(WorkerError::Authorization)?;
            (sid, gid)
        };

        let mut cmd = Command::new(&config.executable);
        cmd.args(&config.args)
            .current_dir(&config.working_dir)
            .env_clear()
            .envs(&config.allowed_env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        #[cfg(unix)]
        {
            let limits = config.limits.clone();
            // SAFETY: only async-signal-safe `getrlimit`/`setrlimit` syscalls execute pre-exec.
            unsafe {
                cmd.pre_exec(move || {
                    if let Some(cpu) = limits.cpu_time_limit_secs {
                        apply_rlimit(libc::RLIMIT_CPU, cpu)?;
                    }
                    if let Some(nofile) = limits.max_open_files {
                        apply_rlimit(libc::RLIMIT_NOFILE, nofile)?;
                    }
                    if let Some(fsize) = limits.max_file_size_bytes {
                        apply_rlimit(libc::RLIMIT_FSIZE, fsize)?;
                    }
                    if let Some(as_bytes) = limits.address_space_limit_bytes {
                        apply_rlimit(libc::RLIMIT_AS, as_bytes)?;
                    }
                    Ok(())
                });
            }
        }

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(err) => {
                if let Ok(mut auth) = authorizer.lock() {
                    auth.revoke_session(session_id);
                }
                return Err(WorkerError::SpawnFailed(err.to_string()));
            }
        };

        let stdin = child.stdin.take();
        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            if let Ok(mut auth) = authorizer.lock() {
                auth.revoke_session(session_id);
            }
            return Err(WorkerError::SpawnFailed(
                "worker stdout pipe missing".into(),
            ));
        };

        let (rx, reader_thread) = spawn_bounded_reader(stdout, config.limits.max_response_bytes);

        Ok(Self {
            session_id,
            grant_id,
            authorizer,
            child: Some(child),
            stdin,
            rx,
            reader_thread: Some(reader_thread),
            limits: config.limits,
            confinement,
            next_request_id: 1,
            cancelled: false,
        })
    }

    #[must_use]
    pub fn session_id(&self) -> SessionId {
        self.session_id
    }

    #[must_use]
    pub fn grant_id(&self) -> GrantId {
        self.grant_id
    }

    #[must_use]
    pub fn confinement_report(&self) -> &OsConfinementReport {
        &self.confinement
    }

    /// Terminate the worker process immediately and revoke all session capability grants.
    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.terminate_and_revoke();
    }

    fn terminate_and_revoke(&mut self) {
        if let Ok(mut auth) = self.authorizer.lock() {
            auth.revoke_session(self.session_id);
        }
        drop(self.stdin.take());
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(handle) = self.reader_thread.take() {
            let _ = handle.join();
        }
    }

    fn write_json_line<T: Serialize>(&mut self, payload: &T) -> Result<(), WorkerError> {
        let mut line =
            serde_json::to_vec(payload).map_err(|e| WorkerError::ProtocolError(e.to_string()))?;
        line.push(b'\n');
        if line.len() > self.limits.max_request_bytes {
            return Err(WorkerError::PayloadTooLarge {
                limit: self.limits.max_request_bytes,
                actual: line.len(),
            });
        }
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| WorkerError::Crashed("worker stdin closed".into()))?;
        stdin
            .write_all(&line)
            .and_then(|()| stdin.flush())
            .map_err(|e| WorkerError::Crashed(e.to_string()))
    }

    /// Invoke one bounded RPC method and mediate any nested host callbacks against
    /// this session's active capability grant.
    pub fn invoke(
        &mut self,
        method: &str,
        params: serde_json::Value,
        handler: &mut dyn HostOperationHandler,
    ) -> Result<serde_json::Value, WorkerError> {
        if self.cancelled {
            return Err(WorkerError::Cancelled);
        }
        if self.child.is_none() {
            return Err(WorkerError::Crashed(
                "worker process already stopped".into(),
            ));
        }

        let req_id = self.next_request_id;
        self.next_request_id += 1;

        let req = HostRequestEnvelope {
            kind: "request",
            id: req_id,
            session_id: self.session_id.0,
            method,
            params: &params,
        };
        if let Err(err) = self.write_json_line(&req) {
            self.terminate_and_revoke();
            return Err(err);
        }

        let deadline = Instant::now() + self.limits.request_timeout;
        loop {
            let now = Instant::now();
            let Some(remaining) = deadline.checked_duration_since(now) else {
                self.terminate_and_revoke();
                return Err(WorkerError::Timeout(self.limits.request_timeout));
            };

            match self.rx.recv_timeout(remaining) {
                Ok(ReaderEvent::PayloadTooLarge(actual)) => {
                    self.terminate_and_revoke();
                    return Err(WorkerError::PayloadTooLarge {
                        limit: self.limits.max_response_bytes,
                        actual,
                    });
                }
                Ok(ReaderEvent::Eof) => {
                    self.terminate_and_revoke();
                    return Err(WorkerError::Crashed(
                        "worker closed stdout before response".into(),
                    ));
                }
                Ok(ReaderEvent::IoError(msg)) => {
                    self.terminate_and_revoke();
                    return Err(WorkerError::Crashed(msg));
                }
                Ok(ReaderEvent::Line(line)) => {
                    let msg: WorkerMessage = serde_json::from_str(&line).map_err(|e| {
                        self.terminate_and_revoke();
                        WorkerError::ProtocolError(format!("invalid JSON from worker: {e}"))
                    })?;
                    match msg {
                        WorkerMessage::HostCall {
                            id,
                            claimed_session,
                            operation,
                        } => {
                            let effective_session =
                                claimed_session.map(SessionId).unwrap_or(self.session_id);
                            let auth_res = self
                                .authorizer
                                .lock()
                                .map_err(|_| WorkerError::Crashed("authorizer poisoned".into()))?
                                .authorize(
                                    self.grant_id,
                                    effective_session,
                                    operation.required_capability(),
                                );

                            let reply = match auth_res {
                                Ok(()) => match handler.handle(&operation) {
                                    Ok(value) => HostResultEnvelope {
                                        kind: "host_result",
                                        id,
                                        ok: true,
                                        result: Some(value),
                                        code: None,
                                        error: None,
                                    },
                                    Err(err) => HostResultEnvelope {
                                        kind: "host_result",
                                        id,
                                        ok: false,
                                        result: None,
                                        code: Some("handler_error"),
                                        error: Some(err),
                                    },
                                },
                                Err(auth_err) => HostResultEnvelope {
                                    kind: "host_result",
                                    id,
                                    ok: false,
                                    result: None,
                                    code: Some(auth_err.code()),
                                    error: Some(auth_err.to_string()),
                                },
                            };
                            if let Err(err) = self.write_json_line(&reply) {
                                self.terminate_and_revoke();
                                return Err(err);
                            }
                        }
                        WorkerMessage::Response {
                            id,
                            ok,
                            result,
                            error,
                        } => {
                            if id != req_id {
                                self.terminate_and_revoke();
                                return Err(WorkerError::ProtocolError(format!(
                                    "unexpected response id {id} (expected {req_id})"
                                )));
                            }
                            if ok {
                                return Ok(result.unwrap_or(serde_json::Value::Null));
                            }
                            return Err(WorkerError::WorkerReturnedError(
                                error.unwrap_or_else(|| "unspecified worker failure".into()),
                            ));
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    self.terminate_and_revoke();
                    return Err(WorkerError::Timeout(self.limits.request_timeout));
                }
                Err(RecvTimeoutError::Disconnected) => {
                    self.terminate_and_revoke();
                    return Err(WorkerError::Crashed(
                        "worker reader thread disconnected".into(),
                    ));
                }
            }
        }
    }
}

impl Drop for ExternalWorkerSession {
    fn drop(&mut self) {
        self.terminate_and_revoke();
    }
}
