use crate::cli::oneshot::rust_decomp::record::{
    AbortedRenderKind, InfrastructureFailureKind, RenderConfig,
};
use crate::cli::oneshot::rust_decomp::{
    FunctionRenderResult, RenderOutcomeKind, make_aborted_render_result,
    make_infrastructure_error_result, make_internal_error_result, render_one_function_inner,
};
use fission_core::cancellation::{CancellationReason, CancellationToken, with_cancellation_scope};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::facts::FactStore;
use std::cmp::min;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_DECOMP_STACK_MB: usize = 32;
const DEFAULT_CANCEL_GRACE_MS: u64 = 50;
const DEFAULT_SHUTDOWN_GRACE_MS: u64 = 100;
const CANCEL_POLL_TICK: Duration = Duration::from_millis(25);

pub(crate) fn resolve_worker_count(total_functions: usize) -> usize {
    if total_functions <= 1 {
        return 1;
    }

    if let Ok(value) = std::env::var("FISSION_RUST_DECOMP_WORKERS") {
        if let Ok(parsed) = value.parse::<usize>() {
            return parsed.max(1).min(total_functions);
        }
    }

    // No upper cap beyond the machine's own core count: the queue is a
    // plain `Arc<Mutex<Receiver>>` each worker only holds briefly to pull
    // one task off before doing the actual (unlocked) decompile work, so
    // there's no contention reason to leave cores idle on larger machines
    // (see `run_worker_fanout_fanin` below).
    let cpu = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .max(1);
    min(total_functions, cpu)
}

pub(crate) fn resolve_decomp_stack_size_bytes() -> usize {
    let mb = std::env::var("FISSION_RUST_DECOMP_STACK_MB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_DECOMP_STACK_MB)
        .clamp(8, 256);
    mb * 1024 * 1024
}

fn render_timeout_message(timeout_ms: u64) -> String {
    format!("preview_timeout: Rust-Sleigh render timed out after {timeout_ms}ms")
}

/// Resolve the timeout used by the one-shot Rust-Sleigh renderer.
///
/// The CLI option remains authoritative, including `Some(0)` which explicitly
/// disables the watchdog.  Omitting the option must still use the same
/// configured default as the rest of the decompiler runtime; passing `None`
/// through here would make the worker join unbounded.
pub(crate) fn resolve_render_timeout_ms(requested: Option<u64>) -> Option<u64> {
    requested.or(Some(fission_core::CONFIG.decompiler.timeout_ms))
}

/// Resolve the deterministic work limits for a `decomp` run: built-in
/// defaults, then `[decompiler.work_limits]` from the configuration, then any
/// `--work-limit` flags (the last flag for a limit wins).
pub(crate) fn resolve_resource_limits(
    flags: &[crate::cli::args::WorkLimitFlag],
) -> fission_decompiler::midend_core::ir::ResourceLimits {
    let overrides =
        crate::cli::args::overrides_with_flags(fission_core::CONFIG.decompiler.work_limits, flags);
    fission_decompiler::midend_core::ir::ResourceLimits::DEFAULT.with_overrides(&overrides)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkerLifecyclePolicy {
    pub max_live_workers: usize,
    pub grace_period: Duration,
    pub shutdown_grace: Duration,
}

impl WorkerLifecyclePolicy {
    pub(crate) fn for_workers(worker_count: usize) -> Self {
        Self {
            max_live_workers: worker_count.max(1),
            grace_period: Duration::from_millis(DEFAULT_CANCEL_GRACE_MS),
            shutdown_grace: Duration::from_millis(DEFAULT_SHUTDOWN_GRACE_MS),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct WorkerLifecycleSnapshot {
    pub active_workers: usize,
    pub peak_active_workers: usize,
    pub reclaimed_workers: usize,
    pub quota_exhausted_tasks: usize,
}

struct DetachedWorker {
    handle: Option<thread::JoinHandle<()>>,
    exited: Arc<AtomicBool>,
}

struct WorkerTrackerState {
    active_workers: usize,
    peak_active_workers: usize,
    reclaimed_workers: usize,
    quota_exhausted_tasks: usize,
    detached: Vec<DetachedWorker>,
}

/// Coordinates live execution worker slots across normal completion,
/// cooperative grace-period reclamation, and unexited timed-out workers.
pub(crate) struct WorkerSlotTracker {
    max_live_workers: usize,
    state: Mutex<WorkerTrackerState>,
    exit_cv: Condvar,
}

impl WorkerSlotTracker {
    pub(crate) fn new(max_live_workers: usize) -> Arc<Self> {
        Arc::new(Self {
            max_live_workers: max_live_workers.max(1),
            state: Mutex::new(WorkerTrackerState {
                active_workers: 0,
                peak_active_workers: 0,
                reclaimed_workers: 0,
                quota_exhausted_tasks: 0,
                detached: Vec::new(),
            }),
            exit_cv: Condvar::new(),
        })
    }

    pub(crate) fn try_acquire_slot(self: &Arc<Self>) -> Option<WorkerSlotPermit> {
        self.reap_finished();
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.active_workers >= self.max_live_workers {
            state.quota_exhausted_tasks += 1;
            return None;
        }
        state.active_workers += 1;
        state.peak_active_workers = state.peak_active_workers.max(state.active_workers);
        Some(WorkerSlotPermit {
            tracker: Arc::clone(self),
            exited: Arc::new(AtomicBool::new(false)),
            armed: true,
        })
    }

    /// Wait up to `grace` for `handle` to exit cleanly and join it. If it is
    /// still running after `grace`, retain the handle in `detached` while
    /// keeping its slot accounted for in `active_workers` until it exits.
    pub(crate) fn abandon_with_grace(
        &self,
        handle: thread::JoinHandle<()>,
        exited: Arc<AtomicBool>,
        grace: Duration,
    ) -> bool {
        if grace > Duration::ZERO {
            let deadline = Instant::now() + grace;
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            while !exited.load(Ordering::Acquire) {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                let (guard, timeout_res) = self
                    .exit_cv
                    .wait_timeout(state, remaining)
                    .unwrap_or_else(|e| e.into_inner());
                state = guard;
                if timeout_res.timed_out() {
                    break;
                }
            }
        }
        if exited.load(Ordering::Acquire) {
            let _ = handle.join();
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.reclaimed_workers += 1;
            true
        } else {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if exited.load(Ordering::Acquire) {
                drop(state);
                let _ = handle.join();
                let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
                state.reclaimed_workers += 1;
                true
            } else {
                state.detached.push(DetachedWorker {
                    handle: Some(handle),
                    exited,
                });
                false
            }
        }
    }

    pub(crate) fn reap_finished(&self) -> usize {
        let mut ready = Vec::new();
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let mut remaining = Vec::with_capacity(state.detached.len());
            for worker in state.detached.drain(..) {
                if worker.exited.load(Ordering::Acquire) {
                    ready.push(worker);
                } else {
                    remaining.push(worker);
                }
            }
            state.detached = remaining;
        }
        let mut reaped = 0usize;
        for mut worker in ready {
            if let Some(handle) = worker.handle.take() {
                let _ = handle.join();
                reaped += 1;
            }
        }
        if reaped > 0 {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.reclaimed_workers += reaped;
        }
        reaped
    }

    pub(crate) fn shutdown_drain(&self, grace: Duration) -> WorkerLifecycleSnapshot {
        self.reap_finished();
        if grace > Duration::ZERO {
            let deadline = Instant::now() + grace;
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            while state.active_workers > 0 {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                let (guard, timeout_res) = self
                    .exit_cv
                    .wait_timeout(state, remaining)
                    .unwrap_or_else(|e| e.into_inner());
                state = guard;
                if timeout_res.timed_out() {
                    break;
                }
            }
        }
        self.reap_finished();
        self.snapshot()
    }

    pub(crate) fn snapshot(&self) -> WorkerLifecycleSnapshot {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        WorkerLifecycleSnapshot {
            active_workers: state.active_workers,
            peak_active_workers: state.peak_active_workers,
            reclaimed_workers: state.reclaimed_workers,
            quota_exhausted_tasks: state.quota_exhausted_tasks,
        }
    }
}

pub(crate) struct WorkerSlotPermit {
    tracker: Arc<WorkerSlotTracker>,
    exited: Arc<AtomicBool>,
    armed: bool,
}

impl WorkerSlotPermit {
    fn into_guard(mut self) -> WorkerSlotGuard {
        self.armed = false;
        WorkerSlotGuard {
            tracker: Arc::clone(&self.tracker),
            exited: Arc::clone(&self.exited),
        }
    }
}

impl Drop for WorkerSlotPermit {
    fn drop(&mut self) {
        if self.armed {
            let mut state = self.tracker.state.lock().unwrap_or_else(|e| e.into_inner());
            self.exited.store(true, Ordering::Release);
            state.active_workers = state.active_workers.saturating_sub(1);
            self.tracker.exit_cv.notify_all();
        }
    }
}

struct WorkerSlotGuard {
    tracker: Arc<WorkerSlotTracker>,
    exited: Arc<AtomicBool>,
}

impl Drop for WorkerSlotGuard {
    fn drop(&mut self) {
        let mut state = self.tracker.state.lock().unwrap_or_else(|e| e.into_inner());
        self.exited.store(true, Ordering::Release);
        state.active_workers = state.active_workers.saturating_sub(1);
        self.tracker.exit_cv.notify_all();
    }
}

#[cfg(test)]
pub(crate) fn render_one_function_on_large_stack(
    binary: Arc<fission_loader::loader::LoadedBinary>,
    facts: Arc<FactStore>,
    func: &FunctionInfo,
    config: RenderConfig,
    stack_size_bytes: usize,
) -> FunctionRenderResult {
    let tracker = WorkerSlotTracker::new(1);
    let policy = WorkerLifecyclePolicy::for_workers(1);
    let batch_token = CancellationToken::new();
    let func_owned = func.clone();
    let binary_for_render = Arc::clone(&binary);
    let result = render_on_large_stack_with_pool(
        binary,
        func,
        config,
        stack_size_bytes,
        &tracker,
        &batch_token,
        policy,
        move || {
            render_one_function_inner(
                binary_for_render.as_ref(),
                facts.as_ref(),
                &func_owned,
                config,
            )
        },
    );
    tracker.shutdown_drain(policy.shutdown_grace);
    result
}

#[cfg(test)]
fn render_on_large_stack_with<R>(
    binary: Arc<fission_loader::loader::LoadedBinary>,
    func: &FunctionInfo,
    config: RenderConfig,
    stack_size_bytes: usize,
    render: R,
) -> FunctionRenderResult
where
    R: FnOnce() -> FunctionRenderResult + Send + 'static,
{
    let tracker = WorkerSlotTracker::new(1);
    let policy = WorkerLifecyclePolicy::for_workers(1);
    let batch_token = CancellationToken::new();
    let result = render_on_large_stack_with_pool(
        binary,
        func,
        config,
        stack_size_bytes,
        &tracker,
        &batch_token,
        policy,
        render,
    );
    tracker.shutdown_drain(policy.shutdown_grace);
    result
}

fn recv_with_cancellation(
    result_rx: &mpsc::Receiver<FunctionRenderResult>,
    token: &CancellationToken,
) -> Result<FunctionRenderResult, RecvOrCancel> {
    loop {
        match token.check() {
            Err(CancellationReason::Cancelled) => return Err(RecvOrCancel::Cancelled),
            Err(CancellationReason::TimedOut) => return Err(RecvOrCancel::TimedOut),
            Ok(()) => {}
        }
        let wait = token
            .remaining()
            .map(|rem| rem.min(CANCEL_POLL_TICK))
            .unwrap_or(CANCEL_POLL_TICK);
        match result_rx.recv_timeout(wait) {
            Ok(result) => match token.check() {
                Ok(()) => return Ok(result),
                Err(CancellationReason::Cancelled) => return Err(RecvOrCancel::Cancelled),
                Err(CancellationReason::TimedOut) => return Err(RecvOrCancel::TimedOut),
            },
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => match token.check() {
                Ok(()) => return Err(RecvOrCancel::Disconnected),
                Err(CancellationReason::Cancelled) => return Err(RecvOrCancel::Cancelled),
                Err(CancellationReason::TimedOut) => return Err(RecvOrCancel::TimedOut),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecvOrCancel {
    TimedOut,
    Cancelled,
    Disconnected,
}

/// Run `render` for `func` on a dedicated large-stack thread, bounded by the
/// configured render timeout and worker slot tracker.
#[allow(clippy::too_many_arguments)]
fn render_on_large_stack_with_pool<R>(
    binary: Arc<fission_loader::loader::LoadedBinary>,
    func: &FunctionInfo,
    config: RenderConfig,
    stack_size_bytes: usize,
    tracker: &Arc<WorkerSlotTracker>,
    batch_token: &CancellationToken,
    policy: WorkerLifecyclePolicy,
    render: R,
) -> FunctionRenderResult
where
    R: FnOnce() -> FunctionRenderResult + Send + 'static,
{
    match batch_token.check() {
        Err(CancellationReason::Cancelled) => {
            return make_infrastructure_error_result(
                binary.as_ref(),
                func,
                InfrastructureFailureKind::Cancelled,
                "decompilation cancelled before starting worker".to_string(),
                config,
            );
        }
        Err(CancellationReason::TimedOut) => {
            let ms = config.timeout_ms.unwrap_or(0);
            return make_aborted_render_result(
                binary.as_ref(),
                func,
                AbortedRenderKind::AnalysisTimedOut,
                render_timeout_message(ms),
                config,
            );
        }
        Ok(()) => {}
    }

    let Some(permit) = tracker.try_acquire_slot() else {
        return make_infrastructure_error_result(
            binary.as_ref(),
            func,
            InfrastructureFailureKind::WorkerUnavailable,
            format!(
                "decompiler worker concurrency quota exhausted (max_live_workers={})",
                policy.max_live_workers
            ),
            config,
        );
    };

    let exited = Arc::clone(&permit.exited);
    let timeout_ms = config.timeout_ms.filter(|ms| *ms > 0);
    let task_token = batch_token.child(timeout_ms.map(Duration::from_millis));
    let task_token_for_thread = task_token.clone();
    let func_for_thread = func.clone();
    let binary_for_thread = Arc::clone(&binary);
    let (result_tx, result_rx) = mpsc::sync_channel::<FunctionRenderResult>(1);

    let spawn = thread::Builder::new()
        .name(format!("fission-rust-decomp-0x{:x}", func.address))
        .stack_size(stack_size_bytes)
        .spawn(move || {
            let _slot_guard = permit.into_guard();
            let result = with_cancellation_scope(task_token_for_thread.clone(), || {
                render_catching_panic(binary_for_thread.as_ref(), &func_for_thread, config, render)
            });
            if !task_token_for_thread.is_cancelled() {
                let _ = result_tx.send(result);
            }
        });

    match spawn {
        Ok(handle) => {
            if timeout_ms.is_some() || batch_token.deadline().is_some() {
                match recv_with_cancellation(&result_rx, &task_token) {
                    Ok(result) => {
                        let _ = handle.join();
                        result
                    }
                    Err(RecvOrCancel::TimedOut) => {
                        task_token.mark_timed_out();
                        let _ = tracker.abandon_with_grace(handle, exited, policy.grace_period);
                        let reported_ms = timeout_ms.unwrap_or(0);
                        make_aborted_render_result(
                            binary.as_ref(),
                            func,
                            AbortedRenderKind::AnalysisTimedOut,
                            render_timeout_message(reported_ms),
                            config,
                        )
                    }
                    Err(RecvOrCancel::Cancelled) => {
                        let _ = tracker.abandon_with_grace(handle, exited, policy.grace_period);
                        make_infrastructure_error_result(
                            binary.as_ref(),
                            func,
                            InfrastructureFailureKind::Cancelled,
                            "decompilation worker cancelled".to_string(),
                            config,
                        )
                    }
                    Err(RecvOrCancel::Disconnected) => {
                        let _ = handle.join();
                        make_internal_error_result(
                            binary.as_ref(),
                            func,
                            "worker thread exited before returning a render result".to_string(),
                            config,
                        )
                    }
                }
            } else {
                // `--timeout-ms=0` disables the wall-clock deadline while still
                // honoring explicit cancellation.
                match recv_with_cancellation(&result_rx, &task_token) {
                    Ok(result) => match handle.join() {
                        Ok(()) => result,
                        Err(_) => make_internal_error_result(
                            binary.as_ref(),
                            func,
                            "render worker thread panicked outside the render".to_string(),
                            config,
                        ),
                    },
                    Err(RecvOrCancel::Cancelled) => {
                        let _ = tracker.abandon_with_grace(handle, exited, policy.grace_period);
                        make_infrastructure_error_result(
                            binary.as_ref(),
                            func,
                            InfrastructureFailureKind::Cancelled,
                            "decompilation worker cancelled".to_string(),
                            config,
                        )
                    }
                    Err(RecvOrCancel::TimedOut) => {
                        task_token.mark_timed_out();
                        let _ = tracker.abandon_with_grace(handle, exited, policy.grace_period);
                        make_aborted_render_result(
                            binary.as_ref(),
                            func,
                            AbortedRenderKind::AnalysisTimedOut,
                            render_timeout_message(0),
                            config,
                        )
                    }
                    Err(RecvOrCancel::Disconnected) => match handle.join() {
                        Ok(()) => make_internal_error_result(
                            binary.as_ref(),
                            func,
                            "worker thread exited before returning a render result".to_string(),
                            config,
                        ),
                        Err(_) => make_internal_error_result(
                            binary.as_ref(),
                            func,
                            "render worker thread panicked outside the render".to_string(),
                            config,
                        ),
                    },
                }
            }
        }
        Err(err) => make_internal_error_result(
            binary.as_ref(),
            func,
            format!("failed to spawn render worker: {err}"),
            config,
        ),
    }
}

fn render_one_function_catching_panic(
    binary: &fission_loader::loader::LoadedBinary,
    facts: &FactStore,
    func: &FunctionInfo,
    config: RenderConfig,
) -> FunctionRenderResult {
    render_catching_panic(binary, func, config, || {
        render_one_function_inner(binary, facts, func, config)
    })
}

/// Run one function's render, turning a panic inside it into that
/// function's analysis failure (the decompiler crashed on this input; the
/// batch runner did not fail).
fn render_catching_panic(
    binary: &fission_loader::loader::LoadedBinary,
    func: &FunctionInfo,
    config: RenderConfig,
    render: impl FnOnce() -> FunctionRenderResult,
) -> FunctionRenderResult {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(render)).unwrap_or_else(|_| {
        make_aborted_render_result(
            binary,
            func,
            AbortedRenderKind::AnalysisPanicked,
            "decompiler panicked while rendering function".to_string(),
            config,
        )
    })
}

/// Batch-local identity of one accepted task: its index in the accepted
/// function list. Unique within a batch even when several functions share
/// an entry address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct BatchTaskId(pub(crate) usize);

/// The thread that claimed a task: a supervisor index, or the coordinator
/// itself in the sequential paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WorkerId(u32);

impl WorkerId {
    const COORDINATOR: WorkerId = WorkerId(u32::MAX);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TaskTerminalKind {
    AnalysisCompleted,
    InfrastructureFailed,
    NotStarted,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub(crate) struct BatchLedgerSummary {
    pub accepted: usize,
    /// Tasks whose render ran to an analysis outcome, including analysis
    /// failures (reported errors, panics, timeouts).
    pub analysis_completed: usize,
    /// Of `analysis_completed`: renders that panicked or timed out.
    pub analysis_aborted: usize,
    pub infrastructure_failed: usize,
    pub not_started: usize,
    pub cancelled: usize,
    pub synthesized_rows: usize,
    /// A second completion for a task that already has its terminal row.
    pub duplicate_completions: usize,
    /// A completion naming a task id outside this batch.
    pub stale_completions: usize,
    /// A completion for a task the sender had not claimed (not running, or
    /// running on another worker).
    pub unexpected_completions: usize,
    /// A completion whose payload was not rendered for the task it names.
    pub identity_mismatches: usize,
    /// Lifecycle transitions refused by the progress tracker.
    pub rejected_transitions: usize,
    pub spawn_errors: usize,
    pub send_errors: usize,
    pub join_errors: usize,
    pub consumer_errors: usize,
    pub sequential_fallback: bool,
    pub peak_active_workers: usize,
    pub still_running_workers: usize,
    pub reclaimed_workers: usize,
    pub quota_exhausted_tasks: usize,
}

impl BatchLedgerSummary {
    pub(crate) fn terminal_total(&self) -> usize {
        self.analysis_completed + self.infrastructure_failed + self.not_started + self.cancelled
    }

    pub(crate) fn reconciles(&self) -> bool {
        self.accepted == self.terminal_total() && self.analysis_aborted <= self.analysis_completed
    }

    pub(crate) fn has_infrastructure_anomalies(&self) -> bool {
        self.infrastructure_failed > 0
            || self.not_started > 0
            || self.cancelled > 0
            || self.synthesized_rows > 0
            || self.duplicate_completions > 0
            || self.stale_completions > 0
            || self.unexpected_completions > 0
            || self.identity_mismatches > 0
            || self.rejected_transitions > 0
            || self.spawn_errors > 0
            || self.send_errors > 0
            || self.join_errors > 0
            || self.consumer_errors > 0
    }
}

pub(crate) struct BatchFanoutReport {
    pub results: Vec<FunctionRenderResult>,
    pub accounting: BatchLedgerSummary,
}

/// Which batch task a result was rendered for. `func` is the ledger's own
/// canonical entry for that task, shared (not copied), so the ledger checks
/// a result against its task by pointer identity: two aliases at one
/// address, or two byte-identical entries, are still different tasks.
#[derive(Clone)]
pub(crate) struct TaskProvenance {
    task_id: BatchTaskId,
    func: Arc<FunctionInfo>,
}

impl TaskProvenance {
    pub(crate) fn task_id(&self) -> BatchTaskId {
        self.task_id
    }
}

impl FunctionRenderResult {
    pub(crate) fn task_id(&self) -> Option<BatchTaskId> {
        self.provenance.as_ref().map(TaskProvenance::task_id)
    }
}

/// One accepted task as handed to a worker: its id and the ledger's
/// canonical function entry.
#[derive(Clone)]
struct ScheduledTask {
    task_id: BatchTaskId,
    func: Arc<FunctionInfo>,
}

impl ScheduledTask {
    /// Mark `result` as rendered for this task.
    fn stamp(&self, mut result: FunctionRenderResult) -> FunctionRenderResult {
        result.provenance = Some(TaskProvenance {
            task_id: self.task_id,
            func: Arc::clone(&self.func),
        });
        result
    }

    /// Render this task's function and stamp the result with this task, so
    /// the stamp is bound to the function that was actually rendered.
    fn render(
        &self,
        render: impl FnOnce(&FunctionInfo) -> FunctionRenderResult,
    ) -> FunctionRenderResult {
        self.stamp(render(&self.func))
    }

    /// Whether `result` carries this task's stamp and describes its function.
    fn owns(&self, result: &FunctionRenderResult) -> bool {
        result.provenance.as_ref().is_some_and(|provenance| {
            provenance.task_id == self.task_id && Arc::ptr_eq(&provenance.func, &self.func)
        }) && result.address == self.func.address
    }
}

struct WorkerCompletion {
    /// The worker that claims to have run the task.
    worker: WorkerId,
    task_id: BatchTaskId,
    result: FunctionRenderResult,
}

/// Non-terminal lifecycle states, shared between the coordinator and the
/// workers. The only legal transitions are
/// `ACCEPTED -> QUEUED -> RUNNING(worker)`, `QUEUED -> CANCELLED(worker)`
/// and `QUEUED -> SEND_FAILED`; terminal states live in the ledger, which
/// only the coordinator touches.
const STATE_ACCEPTED: u8 = 0;
const STATE_QUEUED: u8 = 1;
const STATE_RUNNING: u8 = 2;
const STATE_CANCELLED: u8 = 3;
const STATE_SEND_FAILED: u8 = 4;

struct BatchProgressTracker {
    /// Per task: state in the low 8 bits, claiming worker + 1 above them
    /// (0 = no claimant), so a claim is one compare-and-swap.
    slots: Vec<AtomicU64>,
    rejected_transitions: AtomicUsize,
}

impl BatchProgressTracker {
    fn new(len: usize) -> Self {
        Self {
            slots: (0..len)
                .map(|_| AtomicU64::new(Self::pack(STATE_ACCEPTED, None)))
                .collect(),
            rejected_transitions: AtomicUsize::new(0),
        }
    }

    fn pack(state: u8, claimant: Option<WorkerId>) -> u64 {
        let claimant = claimant.map_or(0, |worker| u64::from(worker.0) + 1);
        (claimant << 8) | u64::from(state)
    }

    fn unpack(word: u64) -> (u8, Option<WorkerId>) {
        let state = (word & 0xff) as u8;
        let claimant = (word >> 8)
            .checked_sub(1)
            .and_then(|w| u32::try_from(w).ok())
            .map(WorkerId);
        (state, claimant)
    }

    fn transition(
        &self,
        task_id: BatchTaskId,
        from: u8,
        to: u8,
        claimant: Option<WorkerId>,
    ) -> bool {
        let accepted = self.slots.get(task_id.0).is_some_and(|slot| {
            slot.compare_exchange(
                Self::pack(from, None),
                Self::pack(to, claimant),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
        });
        if !accepted {
            self.rejected_transitions.fetch_add(1, Ordering::Relaxed);
        }
        accepted
    }

    fn mark_queued(&self, task_id: BatchTaskId) -> bool {
        self.transition(task_id, STATE_ACCEPTED, STATE_QUEUED, None)
    }

    fn mark_send_failed(&self, task_id: BatchTaskId) -> bool {
        self.transition(task_id, STATE_QUEUED, STATE_SEND_FAILED, None)
    }

    /// Claim a queued task for `worker`. Fails (and is counted) unless the
    /// task is queued and unclaimed; a worker must not render a task it
    /// failed to claim.
    fn try_claim(&self, task_id: BatchTaskId, worker: WorkerId) -> bool {
        self.transition(task_id, STATE_QUEUED, STATE_RUNNING, Some(worker))
    }

    fn mark_cancelled(&self, task_id: BatchTaskId, worker: WorkerId) -> bool {
        self.transition(task_id, STATE_QUEUED, STATE_CANCELLED, Some(worker))
    }

    fn snapshot(&self, task_id: BatchTaskId) -> (u8, Option<WorkerId>) {
        self.slots
            .get(task_id.0)
            .map(|slot| Self::unpack(slot.load(Ordering::Acquire)))
            .unwrap_or((STATE_ACCEPTED, None))
    }

    fn rejected_transitions(&self) -> usize {
        self.rejected_transitions.load(Ordering::Relaxed)
    }
}

struct TaskLedgerEntry {
    func: Arc<FunctionInfo>,
    /// The task's one terminal state and row; set exactly once.
    terminal: Option<(TaskTerminalKind, FunctionRenderResult)>,
    /// Completions rejected because their payload belonged to another task.
    identity_mismatches: usize,
}

struct BatchTaskLedger {
    entries: Vec<TaskLedgerEntry>,
    synthesized_rows: usize,
    duplicate_completions: usize,
    stale_completions: usize,
    unexpected_completions: usize,
    identity_mismatches: usize,
    spawn_errors: usize,
    send_errors: usize,
    join_errors: usize,
    consumer_errors: usize,
    sequential_fallback: bool,
    lifecycle: WorkerLifecycleSnapshot,
}

impl BatchTaskLedger {
    fn new(functions: &[FunctionInfo]) -> Self {
        let entries = functions
            .iter()
            .map(|func| TaskLedgerEntry {
                func: Arc::new(func.clone()),
                terminal: None,
                identity_mismatches: 0,
            })
            .collect();
        Self {
            entries,
            synthesized_rows: 0,
            duplicate_completions: 0,
            stale_completions: 0,
            unexpected_completions: 0,
            identity_mismatches: 0,
            spawn_errors: 0,
            send_errors: 0,
            join_errors: 0,
            consumer_errors: 0,
            sequential_fallback: false,
            lifecycle: WorkerLifecycleSnapshot::default(),
        }
    }

    fn task(&self, task_id: BatchTaskId) -> ScheduledTask {
        ScheduledTask {
            task_id,
            func: Arc::clone(&self.entries[task_id.0].func),
        }
    }

    /// Tasks in dispatch order: largest first (LPT) to avoid long-tail
    /// stragglers, ties by address then task id. Presentation order is
    /// decided separately in [`BatchTaskLedger::into_report`].
    fn lpt_schedule(&self) -> Vec<ScheduledTask> {
        let mut indices: Vec<usize> = (0..self.entries.len()).collect();
        indices.sort_by(|&a, &b| {
            let (fa, fb) = (&self.entries[a].func, &self.entries[b].func);
            fb.size
                .cmp(&fa.size)
                .then_with(|| fa.address.cmp(&fb.address))
                .then_with(|| a.cmp(&b))
        });
        indices
            .into_iter()
            .map(|idx| self.task(BatchTaskId(idx)))
            .collect()
    }

    fn terminal_kind_for_outcome(outcome_kind: RenderOutcomeKind) -> TaskTerminalKind {
        match outcome_kind {
            RenderOutcomeKind::Analysis | RenderOutcomeKind::AnalysisAborted => {
                TaskTerminalKind::AnalysisCompleted
            }
            RenderOutcomeKind::Infrastructure(
                InfrastructureFailureKind::WorkerInternalError
                | InfrastructureFailureKind::WorkerUnavailable,
            ) => TaskTerminalKind::InfrastructureFailed,
            RenderOutcomeKind::Infrastructure(InfrastructureFailureKind::NotStarted) => {
                TaskTerminalKind::NotStarted
            }
            RenderOutcomeKind::Infrastructure(InfrastructureFailureKind::Cancelled) => {
                TaskTerminalKind::Cancelled
            }
        }
    }

    /// Accept `completion` as its task's terminal row only if it is a legal
    /// lifecycle step (the task is running, claimed by the sender, and not
    /// yet terminal) and its payload was rendered for that very task.
    /// Anything else is counted and dropped; the task keeps waiting for a
    /// valid completion and is reconciled if none arrives.
    fn record_completion(
        &mut self,
        completion: WorkerCompletion,
        progress: &BatchProgressTracker,
        on_result: &mut impl FnMut(BatchTaskId, &FunctionRenderResult) -> Result<(), String>,
        binary: &LoadedBinary,
        config: RenderConfig,
    ) -> Result<(), ()> {
        let task_id = completion.task_id;
        if task_id.0 >= self.entries.len() {
            self.stale_completions += 1;
            return Ok(());
        }
        if self.entries[task_id.0].terminal.is_some() {
            self.duplicate_completions += 1;
            return Ok(());
        }
        if progress.snapshot(task_id) != (STATE_RUNNING, Some(completion.worker)) {
            self.unexpected_completions += 1;
            return Ok(());
        }
        let task = self.task(task_id);
        if !task.owns(&completion.result) {
            self.identity_mismatches += 1;
            self.entries[task_id.0].identity_mismatches += 1;
            return Ok(());
        }

        let result = completion.result;
        let entry = &mut self.entries[task_id.0];
        match on_result(task_id, &result) {
            Ok(()) => {
                let kind = Self::terminal_kind_for_outcome(result.outcome_kind);
                entry.terminal = Some((kind, result));
                Ok(())
            }
            Err(consumer_err) => {
                self.consumer_errors += 1;
                self.synthesized_rows += 1;
                let synthesized = task.stamp(make_infrastructure_error_result(
                    binary,
                    &task.func,
                    InfrastructureFailureKind::WorkerInternalError,
                    format!("result consumer failed: {consumer_err}"),
                    config,
                ));
                entry.terminal = Some((TaskTerminalKind::InfrastructureFailed, synthesized));
                Err(())
            }
        }
    }

    /// Give every task still without a terminal row exactly one, derived
    /// from how far it got.
    fn reconcile_missing(
        &mut self,
        progress: &BatchProgressTracker,
        cancelled: bool,
        binary: &LoadedBinary,
        config: RenderConfig,
    ) {
        let join_errors = self.join_errors;
        let consumer_errors = self.consumer_errors;
        for idx in 0..self.entries.len() {
            if self.entries[idx].terminal.is_some() {
                continue;
            }
            let task = self.task(BatchTaskId(idx));
            let mismatches = self.entries[idx].identity_mismatches;
            let (state, _) = progress.snapshot(task.task_id);
            let (kind, message) = match state {
                STATE_RUNNING => (
                    InfrastructureFailureKind::WorkerInternalError,
                    if mismatches > 0 {
                        format!(
                            "worker returned {mismatches} completion(s) rendered for a different task and no valid completion for this one"
                        )
                    } else if join_errors > 0 {
                        format!(
                            "worker supervisor panicked or exited abnormally while rendering task (join_errors={join_errors})"
                        )
                    } else if consumer_errors > 0 {
                        "result consumer stopped after failure while worker task was in flight"
                            .to_string()
                    } else {
                        "worker channel closed before returning completion for started task"
                            .to_string()
                    },
                ),
                STATE_CANCELLED => (
                    InfrastructureFailureKind::Cancelled,
                    "batch task cancelled before execution".to_string(),
                ),
                STATE_SEND_FAILED if cancelled => (
                    InfrastructureFailureKind::Cancelled,
                    "batch task cancelled before worker dispatch".to_string(),
                ),
                STATE_SEND_FAILED => (
                    InfrastructureFailureKind::NotStarted,
                    "worker task queue closed before task could be dispatched".to_string(),
                ),
                _ if cancelled => (
                    InfrastructureFailureKind::Cancelled,
                    "batch task cancelled before execution".to_string(),
                ),
                _ if consumer_errors > 0 => (
                    InfrastructureFailureKind::NotStarted,
                    "result consumer aborted batch before worker started task".to_string(),
                ),
                _ if join_errors > 0 => (
                    InfrastructureFailureKind::NotStarted,
                    format!(
                        "worker pool stopped after supervisor failure before starting task (join_errors={join_errors})"
                    ),
                ),
                _ => (
                    InfrastructureFailureKind::NotStarted,
                    "worker pool exited before starting accepted task".to_string(),
                ),
            };

            let synthesized = task.stamp(make_infrastructure_error_result(
                binary, &task.func, kind, message, config,
            ));
            let terminal = Self::terminal_kind_for_outcome(synthesized.outcome_kind);
            self.entries[idx].terminal = Some((terminal, synthesized));
            self.synthesized_rows += 1;
        }
    }

    fn into_report(self, progress: &BatchProgressTracker) -> BatchFanoutReport {
        let mut accounting = BatchLedgerSummary {
            accepted: self.entries.len(),
            synthesized_rows: self.synthesized_rows,
            duplicate_completions: self.duplicate_completions,
            stale_completions: self.stale_completions,
            unexpected_completions: self.unexpected_completions,
            identity_mismatches: self.identity_mismatches,
            rejected_transitions: progress.rejected_transitions(),
            spawn_errors: self.spawn_errors,
            send_errors: self.send_errors,
            join_errors: self.join_errors,
            consumer_errors: self.consumer_errors,
            sequential_fallback: self.sequential_fallback,
            peak_active_workers: self.lifecycle.peak_active_workers,
            still_running_workers: self.lifecycle.active_workers,
            reclaimed_workers: self.lifecycle.reclaimed_workers,
            quota_exhausted_tasks: self.lifecycle.quota_exhausted_tasks,
            ..BatchLedgerSummary::default()
        };

        // Presentation order depends only on canonical task metadata
        // (`(address, task_id)`), never on worker arrival or LPT order,
        // including when aliases share an address.
        let mut order: Vec<usize> = (0..self.entries.len()).collect();
        order.sort_by_key(|&idx| (self.entries[idx].func.address, idx));

        let mut rows: Vec<Option<FunctionRenderResult>> = Vec::with_capacity(self.entries.len());
        for (idx, entry) in self.entries.into_iter().enumerate() {
            let Some((kind, result)) = entry.terminal else {
                panic!("batch task {idx} has no terminal state during report assembly");
            };
            assert_eq!(
                result.task_id(),
                Some(BatchTaskId(idx)),
                "terminal row for batch task {idx} is stamped for another task"
            );
            match kind {
                TaskTerminalKind::AnalysisCompleted => {
                    accounting.analysis_completed += 1;
                    if result.outcome_kind == RenderOutcomeKind::AnalysisAborted {
                        accounting.analysis_aborted += 1;
                    }
                }
                TaskTerminalKind::InfrastructureFailed => accounting.infrastructure_failed += 1,
                TaskTerminalKind::NotStarted => accounting.not_started += 1,
                TaskTerminalKind::Cancelled => accounting.cancelled += 1,
            }
            rows.push(Some(result));
        }
        assert!(
            accounting.reconciles(),
            "batch ledger accounting failed to reconcile: {accounting:?}"
        );

        let results: Vec<FunctionRenderResult> = order
            .into_iter()
            .map(|idx| rows[idx].take().expect("each task row is emitted once"))
            .collect();
        assert_eq!(results.len(), accounting.accepted);

        BatchFanoutReport {
            results,
            accounting,
        }
    }
}

type RenderFn = Arc<
    dyn Fn(&LoadedBinary, &FactStore, &FunctionInfo, RenderConfig) -> FunctionRenderResult
        + Send
        + Sync,
>;

fn production_render_fn() -> RenderFn {
    Arc::new(render_one_function_catching_panic)
}

fn sync_cancel(cancel: &AtomicBool, token: &CancellationToken) {
    if cancel.load(Ordering::Acquire) {
        token.cancel();
    }
}

enum WorkerSpawnError {
    QuotaExhausted,
    Io(std::io::Error),
}

struct PersistentDecompWorker {
    request_tx: mpsc::SyncSender<(ScheduledTask, CancellationToken)>,
    result_rx: mpsc::Receiver<FunctionRenderResult>,
    handle: Option<thread::JoinHandle<()>>,
    exited: Arc<AtomicBool>,
    tracker: Arc<WorkerSlotTracker>,
}

impl PersistentDecompWorker {
    fn spawn(
        worker_idx: usize,
        binary: Arc<LoadedBinary>,
        facts: Arc<FactStore>,
        config: RenderConfig,
        stack_size_bytes: usize,
        tracker: Arc<WorkerSlotTracker>,
        render_fn: RenderFn,
    ) -> Result<Self, WorkerSpawnError> {
        let Some(permit) = tracker.try_acquire_slot() else {
            return Err(WorkerSpawnError::QuotaExhausted);
        };
        let exited = Arc::clone(&permit.exited);
        let (request_tx, request_rx) = mpsc::sync_channel::<(ScheduledTask, CancellationToken)>(0);
        let (result_tx, result_rx) = mpsc::sync_channel::<FunctionRenderResult>(1);
        let handle = thread::Builder::new()
            .name(format!("fission-rust-decomp-worker-{worker_idx}"))
            .stack_size(stack_size_bytes)
            .spawn(move || {
                let _slot_guard = permit.into_guard();
                while let Ok((task, task_token)) = request_rx.recv() {
                    if task_token.is_cancelled() {
                        break;
                    }
                    let result = task.render(|func| {
                        with_cancellation_scope(task_token.clone(), || {
                            render_fn(binary.as_ref(), facts.as_ref(), func, config)
                        })
                    });
                    if task_token.is_cancelled() {
                        break;
                    }
                    if result_tx.send(result).is_err() {
                        break;
                    }
                }
            })
            .map_err(WorkerSpawnError::Io)?;
        Ok(Self {
            request_tx,
            result_rx,
            handle: Some(handle),
            exited,
            tracker,
        })
    }

    fn shutdown(mut self) {
        drop(self.request_tx);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }

    /// Close request channel, wait up to `grace` for cooperative unwind, and
    /// otherwise retain the live worker handle in `tracker` so its slot
    /// stays accounted for until exit.
    fn cancel_and_release(mut self, grace: Duration) {
        drop(self.request_tx);
        if let Some(handle) = self.handle.take() {
            let _ = self.tracker.abandon_with_grace(handle, self.exited, grace);
        }
    }
}

struct WorkerSupervisorContext {
    worker_idx: usize,
    worker: WorkerId,
    binary: Arc<LoadedBinary>,
    facts: Arc<FactStore>,
    config: RenderConfig,
    stack_size_bytes: usize,
    timeout_ms: Option<u64>,
    task_rx: Arc<Mutex<mpsc::Receiver<ScheduledTask>>>,
    result_tx: mpsc::Sender<WorkerCompletion>,
    progress: Arc<BatchProgressTracker>,
    cancel: Arc<AtomicBool>,
    stop_dispatch: Arc<AtomicBool>,
    batch_token: CancellationToken,
    slot_tracker: Arc<WorkerSlotTracker>,
    policy: WorkerLifecyclePolicy,
    render_fn: RenderFn,
}

/// Render `task` on the supervisor's persistent large-stack child, bounded
/// by `timeout_ms` and the pool's `max_live_workers` quota.
fn render_on_persistent_child(
    ctx: &WorkerSupervisorContext,
    persistent_child: &mut Option<PersistentDecompWorker>,
    task: &ScheduledTask,
    timeout_ms: u64,
) -> FunctionRenderResult {
    let infrastructure = |kind: InfrastructureFailureKind, message: String| {
        task.render(|func| {
            make_infrastructure_error_result(&ctx.binary, func, kind, message, ctx.config)
        })
    };
    sync_cancel(&ctx.cancel, &ctx.batch_token);
    if ctx.batch_token.is_cancelled() {
        return infrastructure(
            InfrastructureFailureKind::Cancelled,
            "decompilation cancelled before worker execution".to_string(),
        );
    }

    if persistent_child.is_none() {
        // Reuse one `stack_size_bytes` thread across tasks so thread-local
        // SLEIGH/type caches stay warm and we avoid per-function 32 MiB
        // thread creation, while keeping even timed-out threads strictly
        // bounded under `max_live_workers`.
        match PersistentDecompWorker::spawn(
            ctx.worker_idx,
            Arc::clone(&ctx.binary),
            Arc::clone(&ctx.facts),
            ctx.config,
            ctx.stack_size_bytes,
            Arc::clone(&ctx.slot_tracker),
            Arc::clone(&ctx.render_fn),
        ) {
            Ok(worker) => *persistent_child = Some(worker),
            Err(WorkerSpawnError::QuotaExhausted) => {
                return infrastructure(
                    InfrastructureFailureKind::WorkerUnavailable,
                    format!(
                        "decompiler worker concurrency quota exhausted (max_live_workers={})",
                        ctx.policy.max_live_workers
                    ),
                );
            }
            Err(WorkerSpawnError::Io(err)) => {
                return infrastructure(
                    InfrastructureFailureKind::WorkerInternalError,
                    format!("failed to spawn render worker: {err}"),
                );
            }
        }
    }

    let task_token = ctx
        .batch_token
        .child(Some(Duration::from_millis(timeout_ms)));
    let worker = persistent_child
        .as_mut()
        .expect("persistent worker initialized");
    if worker
        .request_tx
        .send((task.clone(), task_token.clone()))
        .is_err()
    {
        if let Some(dead) = persistent_child.take() {
            dead.shutdown();
        }
        return infrastructure(
            InfrastructureFailureKind::WorkerInternalError,
            "worker thread exited before receiving a render task".to_string(),
        );
    }
    match recv_with_cancellation(&worker.result_rx, &task_token) {
        Ok(result) if task.owns(&result) => {
            sync_cancel(&ctx.cancel, &task_token);
            if let Err(reason) = task_token.check() {
                match reason {
                    CancellationReason::TimedOut => task.render(|func| {
                        make_aborted_render_result(
                            &ctx.binary,
                            func,
                            AbortedRenderKind::AnalysisTimedOut,
                            render_timeout_message(timeout_ms),
                            ctx.config,
                        )
                    }),
                    CancellationReason::Cancelled => infrastructure(
                        InfrastructureFailureKind::Cancelled,
                        "decompilation cancelled before publishing result".to_string(),
                    ),
                }
            } else {
                result
            }
        }
        Ok(_) => {
            task_token.cancel();
            if let Some(child) = persistent_child.take() {
                child.cancel_and_release(ctx.policy.grace_period);
            }
            infrastructure(
                InfrastructureFailureKind::WorkerInternalError,
                format!(
                    "worker child returned a result not rendered for {:?}",
                    task.task_id
                ),
            )
        }
        Err(RecvOrCancel::TimedOut) => {
            task_token.mark_timed_out();
            if let Some(child) = persistent_child.take() {
                child.cancel_and_release(ctx.policy.grace_period);
            }
            task.render(|func| {
                make_aborted_render_result(
                    &ctx.binary,
                    func,
                    AbortedRenderKind::AnalysisTimedOut,
                    render_timeout_message(timeout_ms),
                    ctx.config,
                )
            })
        }
        Err(RecvOrCancel::Cancelled) => {
            task_token.cancel();
            if let Some(child) = persistent_child.take() {
                child.cancel_and_release(ctx.policy.grace_period);
            }
            infrastructure(
                InfrastructureFailureKind::Cancelled,
                "decompilation worker cancelled".to_string(),
            )
        }
        Err(RecvOrCancel::Disconnected) => {
            if let Some(dead) = persistent_child.take() {
                dead.shutdown();
            }
            infrastructure(
                InfrastructureFailureKind::WorkerInternalError,
                "worker thread exited before returning a render result".to_string(),
            )
        }
    }
}

fn spawn_production_supervisor(
    ctx: WorkerSupervisorContext,
) -> Result<thread::JoinHandle<()>, std::io::Error> {
    let mut builder =
        thread::Builder::new().name(format!("fission-rust-decomp-supervisor-{}", ctx.worker_idx));
    if ctx.timeout_ms.is_none() {
        builder = builder.stack_size(ctx.stack_size_bytes);
    }
    builder.spawn(move || {
        let mut persistent_child: Option<PersistentDecompWorker> = None;
        let _no_timeout_guard = if ctx.timeout_ms.is_none() {
            ctx.slot_tracker
                .try_acquire_slot()
                .map(WorkerSlotPermit::into_guard)
        } else {
            None
        };
        loop {
            sync_cancel(&ctx.cancel, &ctx.batch_token);
            if ctx.batch_token.is_cancelled() || ctx.stop_dispatch.load(Ordering::Acquire) {
                break;
            }
            let task = match ctx.task_rx.lock() {
                Ok(locked) => locked.recv(),
                Err(_) => break,
            };
            let Ok(task) = task else { break };
            sync_cancel(&ctx.cancel, &ctx.batch_token);
            if ctx.batch_token.is_cancelled() {
                ctx.progress.mark_cancelled(task.task_id, ctx.worker);
                break;
            }
            if ctx.stop_dispatch.load(Ordering::Acquire) {
                break;
            }
            if !ctx.progress.try_claim(task.task_id, ctx.worker) {
                // Not queued, or already claimed: rendering it would give the
                // task a second owner. The refusal is counted.
                continue;
            }

            let rendered = if let Some(timeout_ms) = ctx.timeout_ms {
                render_on_persistent_child(&ctx, &mut persistent_child, &task, timeout_ms)
            } else {
                let task_token = ctx.batch_token.child(None);
                let out = task.render(|func| {
                    with_cancellation_scope(task_token.clone(), || {
                        (ctx.render_fn)(ctx.binary.as_ref(), ctx.facts.as_ref(), func, ctx.config)
                    })
                });
                sync_cancel(&ctx.cancel, &task_token);
                if task_token.is_cancelled() {
                    task.render(|func| {
                        make_infrastructure_error_result(
                            &ctx.binary,
                            func,
                            InfrastructureFailureKind::Cancelled,
                            "decompilation cancelled before publishing result".to_string(),
                            ctx.config,
                        )
                    })
                } else {
                    out
                }
            };
            if ctx
                .result_tx
                .send(WorkerCompletion {
                    worker: ctx.worker,
                    task_id: task.task_id,
                    result: rendered,
                })
                .is_err()
            {
                break;
            }
        }
        if let Some(worker) = persistent_child {
            worker.shutdown();
        }
    })
}

/// Render `task` on the coordinator thread and record it, through the same
/// lifecycle (`QUEUED -> RUNNING -> terminal`) the worker pool uses.
fn run_task_on_coordinator(
    ledger: &mut BatchTaskLedger,
    progress: &BatchProgressTracker,
    task: &ScheduledTask,
    render: impl FnOnce(&FunctionInfo) -> FunctionRenderResult,
    on_result: &mut impl FnMut(BatchTaskId, &FunctionRenderResult) -> Result<(), String>,
    binary: &LoadedBinary,
    config: RenderConfig,
) -> Result<(), ()> {
    if !progress.mark_queued(task.task_id)
        || !progress.try_claim(task.task_id, WorkerId::COORDINATOR)
    {
        return Ok(());
    }
    let result = task.render(render);
    ledger.record_completion(
        WorkerCompletion {
            worker: WorkerId::COORDINATOR,
            task_id: task.task_id,
            result,
        },
        progress,
        on_result,
        binary,
        config,
    )
}

pub(crate) fn run_sequential_batch(
    binary: Arc<LoadedBinary>,
    facts: Arc<FactStore>,
    functions: &[FunctionInfo],
    config: RenderConfig,
    stack_size_bytes: usize,
) -> BatchFanoutReport {
    run_sequential_batch_with_policy(
        binary,
        facts,
        functions,
        config,
        stack_size_bytes,
        WorkerLifecyclePolicy::for_workers(1),
        CancellationToken::new(),
        production_render_fn(),
    )
}

#[allow(clippy::too_many_arguments)]
fn run_sequential_batch_with_policy(
    binary: Arc<LoadedBinary>,
    facts: Arc<FactStore>,
    functions: &[FunctionInfo],
    config: RenderConfig,
    stack_size_bytes: usize,
    policy: WorkerLifecyclePolicy,
    batch_token: CancellationToken,
    render_fn: RenderFn,
) -> BatchFanoutReport {
    let mut ledger = BatchTaskLedger::new(functions);
    let progress = BatchProgressTracker::new(functions.len());
    let tracker = WorkerSlotTracker::new(policy.max_live_workers);

    for idx in 0..functions.len() {
        let task = ledger.task(BatchTaskId(idx));
        if batch_token.is_cancelled() {
            if progress.mark_queued(task.task_id) {
                progress.mark_cancelled(task.task_id, WorkerId::COORDINATOR);
            }
            continue;
        }
        let binary_for_task = Arc::clone(&binary);
        let facts_for_task = Arc::clone(&facts);
        let render_fn_for_task = Arc::clone(&render_fn);
        let _ = run_task_on_coordinator(
            &mut ledger,
            &progress,
            &task,
            |func| {
                let func_owned = func.clone();
                let binary_for_render = Arc::clone(&binary_for_task);
                let facts_for_render = Arc::clone(&facts_for_task);
                render_on_large_stack_with_pool(
                    Arc::clone(&binary_for_task),
                    func,
                    config,
                    stack_size_bytes,
                    &tracker,
                    &batch_token,
                    policy,
                    move || {
                        render_fn_for_task(
                            binary_for_render.as_ref(),
                            facts_for_render.as_ref(),
                            &func_owned,
                            config,
                        )
                    },
                )
            },
            &mut |_, _| Ok(()),
            binary.as_ref(),
            config,
        );
    }

    ledger.reconcile_missing(
        &progress,
        batch_token.is_cancelled(),
        binary.as_ref(),
        config,
    );
    ledger.lifecycle = tracker.shutdown_drain(policy.shutdown_grace);
    ledger.into_report(&progress)
}

pub(crate) fn run_worker_fanout_fanin(
    binary: Arc<LoadedBinary>,
    facts: Arc<FactStore>,
    functions: &[FunctionInfo],
    config: RenderConfig,
    worker_count: usize,
    stack_size_bytes: usize,
) -> BatchFanoutReport {
    run_worker_fanout_fanin_with_policy(
        binary,
        facts,
        functions,
        config,
        worker_count,
        stack_size_bytes,
        WorkerLifecyclePolicy::for_workers(worker_count),
        CancellationToken::new(),
        production_render_fn(),
    )
}

#[allow(clippy::too_many_arguments)]
fn run_worker_fanout_fanin_with_policy(
    binary: Arc<LoadedBinary>,
    facts: Arc<FactStore>,
    functions: &[FunctionInfo],
    config: RenderConfig,
    worker_count: usize,
    stack_size_bytes: usize,
    policy: WorkerLifecyclePolicy,
    batch_token: CancellationToken,
    render_fn: RenderFn,
) -> BatchFanoutReport {
    run_worker_fanout_fanin_with_hooks_and_policy(
        binary,
        facts,
        functions,
        config,
        worker_count,
        stack_size_bytes,
        spawn_production_supervisor,
        Arc::new(AtomicBool::new(false)),
        batch_token,
        policy,
        render_fn,
        |_, _| Ok(()),
    )
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn run_worker_fanout_fanin_with_hooks<S, C>(
    binary: Arc<LoadedBinary>,
    facts: Arc<FactStore>,
    functions: &[FunctionInfo],
    config: RenderConfig,
    worker_count: usize,
    stack_size_bytes: usize,
    spawn_supervisor: S,
    cancel: Arc<AtomicBool>,
    on_result: C,
) -> BatchFanoutReport
where
    S: FnMut(WorkerSupervisorContext) -> Result<thread::JoinHandle<()>, std::io::Error>,
    C: FnMut(BatchTaskId, &FunctionRenderResult) -> Result<(), String>,
{
    run_worker_fanout_fanin_with_hooks_and_policy(
        binary,
        facts,
        functions,
        config,
        worker_count,
        stack_size_bytes,
        spawn_supervisor,
        cancel,
        CancellationToken::new(),
        WorkerLifecyclePolicy::for_workers(worker_count),
        production_render_fn(),
        on_result,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_worker_fanout_fanin_with_hooks_and_policy<S, C>(
    binary: Arc<LoadedBinary>,
    facts: Arc<FactStore>,
    functions: &[FunctionInfo],
    config: RenderConfig,
    worker_count: usize,
    stack_size_bytes: usize,
    mut spawn_supervisor: S,
    cancel: Arc<AtomicBool>,
    batch_token: CancellationToken,
    policy: WorkerLifecyclePolicy,
    render_fn: RenderFn,
    mut on_result: C,
) -> BatchFanoutReport
where
    S: FnMut(WorkerSupervisorContext) -> Result<thread::JoinHandle<()>, std::io::Error>,
    C: FnMut(BatchTaskId, &FunctionRenderResult) -> Result<(), String>,
{
    let mut ledger = BatchTaskLedger::new(functions);
    let progress = Arc::new(BatchProgressTracker::new(functions.len()));
    let stop_dispatch = Arc::new(AtomicBool::new(false));
    let slot_tracker = WorkerSlotTracker::new(policy.max_live_workers);

    // Keep both queues unbounded while enqueueing all tasks before receiving
    // results; bounded queues with enqueue-all-before-receive could deadlock
    // (coordinated with #1158).
    let (task_tx, task_rx) = mpsc::channel::<ScheduledTask>();
    let task_rx = Arc::new(Mutex::new(task_rx));
    let (result_tx, result_rx) = mpsc::channel::<WorkerCompletion>();
    let timeout_ms = config.timeout_ms.filter(|ms| *ms > 0);

    let mut worker_handles = Vec::with_capacity(worker_count);
    let mut last_spawn_error = None;
    for worker_idx in 0..worker_count {
        let ctx = WorkerSupervisorContext {
            worker_idx,
            worker: WorkerId(u32::try_from(worker_idx).expect("worker index fits in u32")),
            binary: Arc::clone(&binary),
            facts: Arc::clone(&facts),
            config,
            stack_size_bytes,
            timeout_ms,
            task_rx: Arc::clone(&task_rx),
            result_tx: result_tx.clone(),
            progress: Arc::clone(&progress),
            cancel: Arc::clone(&cancel),
            stop_dispatch: Arc::clone(&stop_dispatch),
            batch_token: batch_token.clone(),
            slot_tracker: Arc::clone(&slot_tracker),
            policy,
            render_fn: Arc::clone(&render_fn),
        };
        match spawn_supervisor(ctx) {
            Ok(handle) => worker_handles.push(handle),
            Err(err) => {
                ledger.spawn_errors += 1;
                last_spawn_error = Some(err);
            }
        }
    }
    drop(result_tx);
    // Drop the coordinator's handle to `task_rx` so if all workers exit
    // immediately, `task_tx.send` observes disconnection instead of buffering.
    drop(task_rx);

    if worker_handles.is_empty() {
        ledger.sequential_fallback = true;
        if let Some(err) = last_spawn_error {
            tracing::warn!(
                "failed to spawn decompilation worker threads ({err}); falling back to sequential rendering"
            );
        }
        let can_use_large_stack_watchdog = ledger.spawn_errors == 0;
        for task in ledger.lpt_schedule() {
            sync_cancel(&cancel, &batch_token);
            if batch_token.is_cancelled() || stop_dispatch.load(Ordering::Acquire) {
                break;
            }
            let step = if can_use_large_stack_watchdog && timeout_ms.is_some() {
                let binary_for_task = Arc::clone(&binary);
                let facts_for_task = Arc::clone(&facts);
                let render_fn_for_task = Arc::clone(&render_fn);
                run_task_on_coordinator(
                    &mut ledger,
                    &progress,
                    &task,
                    |func| {
                        let func_owned = func.clone();
                        let binary_for_render = Arc::clone(&binary_for_task);
                        let facts_for_render = Arc::clone(&facts_for_task);
                        render_on_large_stack_with_pool(
                            Arc::clone(&binary_for_task),
                            func,
                            config,
                            stack_size_bytes,
                            &slot_tracker,
                            &batch_token,
                            policy,
                            move || {
                                render_fn_for_task(
                                    binary_for_render.as_ref(),
                                    facts_for_render.as_ref(),
                                    &func_owned,
                                    config,
                                )
                            },
                        )
                    },
                    &mut on_result,
                    binary.as_ref(),
                    config,
                )
            } else {
                let task_token = batch_token.child(timeout_ms.map(Duration::from_millis));
                run_task_on_coordinator(
                    &mut ledger,
                    &progress,
                    &task,
                    |func| {
                        let out = with_cancellation_scope(task_token.clone(), || {
                            render_fn(binary.as_ref(), facts.as_ref(), func, config)
                        });
                        sync_cancel(&cancel, &task_token);
                        match task_token.check() {
                            Err(CancellationReason::TimedOut) => make_aborted_render_result(
                                binary.as_ref(),
                                func,
                                AbortedRenderKind::AnalysisTimedOut,
                                render_timeout_message(timeout_ms.unwrap_or(0)),
                                config,
                            ),
                            Err(CancellationReason::Cancelled) => make_infrastructure_error_result(
                                binary.as_ref(),
                                func,
                                InfrastructureFailureKind::Cancelled,
                                "decompilation cancelled before publishing result".to_string(),
                                config,
                            ),
                            Ok(()) => out,
                        }
                    },
                    &mut on_result,
                    binary.as_ref(),
                    config,
                )
            };
            if step.is_err() {
                stop_dispatch.store(true, Ordering::Release);
                break;
            }
        }
        sync_cancel(&cancel, &batch_token);
        ledger.reconcile_missing(
            &progress,
            batch_token.is_cancelled(),
            binary.as_ref(),
            config,
        );
        ledger.lifecycle = slot_tracker.shutdown_drain(policy.shutdown_grace);
        return ledger.into_report(&progress);
    }

    for task in ledger.lpt_schedule() {
        sync_cancel(&cancel, &batch_token);
        if batch_token.is_cancelled() {
            break;
        }
        let task_id = task.task_id;
        progress.mark_queued(task_id);
        if task_tx.send(task).is_err() {
            progress.mark_send_failed(task_id);
            ledger.send_errors += 1;
            break;
        }
    }
    drop(task_tx);

    // Drain until all worker `Sender`s close rather than counting messages:
    // counting would let duplicate or stale completions end the loop early
    // and drop legitimate late task results.
    while let Ok(completion) = result_rx.recv() {
        if ledger
            .record_completion(
                completion,
                progress.as_ref(),
                &mut on_result,
                binary.as_ref(),
                config,
            )
            .is_err()
        {
            stop_dispatch.store(true, Ordering::Release);
            batch_token.cancel();
            break;
        }
    }
    drop(result_rx);

    for handle in worker_handles {
        if handle.join().is_err() {
            ledger.join_errors += 1;
        }
    }

    sync_cancel(&cancel, &batch_token);
    ledger.reconcile_missing(
        &progress,
        batch_token.is_cancelled(),
        binary.as_ref(),
        config,
    );
    ledger.lifecycle = slot_tracker.shutdown_drain(policy.shutdown_grace);
    ledger.into_report(&progress)
}

#[cfg(test)]
mod tests {
    use super::{render_timeout_message, resolve_render_timeout_ms, resolve_resource_limits};
    use fission_decompiler::midend_core::ir::ResourceLimits;

    #[test]
    fn configured_timeout_is_used_when_cli_does_not_override_it() {
        assert_eq!(
            resolve_render_timeout_ms(None),
            Some(fission_core::CONFIG.decompiler.timeout_ms)
        );
    }

    #[test]
    fn explicit_timeout_values_are_preserved() {
        assert_eq!(resolve_render_timeout_ms(Some(37)), Some(37));
        assert_eq!(resolve_render_timeout_ms(Some(0)), Some(0));
    }

    #[test]
    fn render_timeout_message_includes_exact_budget() {
        assert_eq!(
            render_timeout_message(37),
            "preview_timeout: Rust-Sleigh render timed out after 37ms"
        );
    }

    #[test]
    fn fanout_falls_back_sequentially_when_no_workers_can_spawn() {
        use super::RenderConfig;
        use fission_loader::loader::{
            BinaryLoadSpec, DataBuffer, FunctionInfo, LoadedBinaryBuilder, SectionInfo,
        };
        use fission_static::analysis::decomp::facts::FactStore;
        use std::sync::Arc;

        let mut code = vec![0x90u8; 0x20];
        code[0..6].copy_from_slice(&[0xb8, 0x01, 0x00, 0x00, 0x00, 0xc3]);
        code[0x10..0x16].copy_from_slice(&[0xb8, 0x02, 0x00, 0x00, 0x00, 0xc3]);
        let binary = Arc::new(
            LoadedBinaryBuilder::new("worker-fallback.elf".into(), DataBuffer::Heap(code))
                .format("ELF")
                .image_base(0x1000)
                .entry_point(0x1000)
                .is_64bit(true)
                .load_spec(BinaryLoadSpec::new(
                    "ELF",
                    0x1000,
                    "x86:LE:64:default",
                    "gcc",
                    "synthetic-test",
                ))
                .add_section(SectionInfo {
                    name: ".text".into(),
                    virtual_address: 0x1000,
                    virtual_size: 0x20,
                    file_offset: 0,
                    file_size: 0x20,
                    is_executable: true,
                    is_readable: true,
                    is_writable: false,
                })
                .add_functions([
                    FunctionInfo {
                        name: "fn_one".into(),
                        address: 0x1000,
                        size: 6,
                        ..Default::default()
                    },
                    FunctionInfo {
                        name: "fn_two".into(),
                        address: 0x1010,
                        size: 6,
                        ..Default::default()
                    },
                ])
                .build()
                .expect("build binary"),
        );
        let facts = Arc::new(FactStore::from_binary_without_signature_matches(&binary));
        let functions = binary.functions.clone();
        let config = RenderConfig {
            benchmark: false,
            ghidra_compat: false,
            effective_no_warnings: true,
            debug_decomp: false,
            debug_decomp_bundle: false,
            requested_address: None,
            timeout_ms: Some(0),
            resource_limits: Default::default(),
            prehir: false,
        };

        // Zero workers: sequential fallback still returns all functions cleanly.
        let zero_workers = super::run_worker_fanout_fanin(
            Arc::clone(&binary),
            Arc::clone(&facts),
            &functions,
            config,
            0,
            8 * 1024 * 1024,
        );
        assert_eq!(zero_workers.results.len(), 2);
        assert!(zero_workers.accounting.sequential_fallback);
        assert_eq!(zero_workers.accounting.accepted, 2);
        assert_eq!(zero_workers.accounting.analysis_completed, 2);
        assert_eq!(zero_workers.accounting.spawn_errors, 0);
        assert!(zero_workers.accounting.reconciles());

        // Unsatisfiable stack size triggers OS thread spawn error -> sequential fallback.
        let impossible_stack =
            super::run_worker_fanout_fanin(binary, facts, &functions, config, 2, usize::MAX / 2);
        assert_eq!(impossible_stack.results.len(), 2);
        assert!(impossible_stack.accounting.sequential_fallback);
        assert_eq!(impossible_stack.accounting.spawn_errors, 2);
        assert_eq!(impossible_stack.accounting.analysis_completed, 2);
        assert!(impossible_stack.accounting.reconciles());
    }

    mod ledger_fault_tests {
        use super::super::{
            BatchTaskId, WorkerCompletion, WorkerSupervisorContext,
            run_worker_fanout_fanin_with_hooks,
        };
        use crate::cli::oneshot::rust_decomp::record::{
            CliRustDecompileRecord, CliRustOutcome, RenderConfig,
        };
        use crate::cli::oneshot::rust_decomp::{
            FunctionRenderResult, RenderOutcomeKind, record_into_function_render_result,
        };
        use fission_loader::loader::{
            BinaryLoadSpec, DataBuffer, FunctionInfo, LoadedBinary, LoadedBinaryBuilder,
            SectionInfo,
        };
        use fission_static::analysis::decomp::facts::FactStore;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};
        use std::thread;

        fn test_config() -> RenderConfig {
            RenderConfig {
                benchmark: false,
                ghidra_compat: false,
                effective_no_warnings: true,
                debug_decomp: false,
                debug_decomp_bundle: false,
                requested_address: None,
                timeout_ms: Some(0),
                resource_limits: Default::default(),
                prehir: false,
            }
        }

        fn make_test_binary(functions: Vec<FunctionInfo>) -> (Arc<LoadedBinary>, Arc<FactStore>) {
            let code = vec![0xc3u8; 0x100];
            let mut builder =
                LoadedBinaryBuilder::new("ledger-test.elf".into(), DataBuffer::Heap(code))
                    .format("ELF")
                    .image_base(0x1000)
                    .entry_point(0x1000)
                    .is_64bit(true)
                    .load_spec(BinaryLoadSpec::new(
                        "ELF",
                        0x1000,
                        "x86:LE:64:default",
                        "gcc",
                        "synthetic-test",
                    ))
                    .add_section(SectionInfo {
                        name: ".text".into(),
                        virtual_address: 0x1000,
                        virtual_size: 0x100,
                        file_offset: 0,
                        file_size: 0x100,
                        is_executable: true,
                        is_readable: true,
                        is_writable: false,
                    });
            builder = builder.add_functions(functions);
            let binary = Arc::new(builder.build().expect("build test binary"));
            let facts = Arc::new(FactStore::from_binary_without_signature_matches(&binary));
            (binary, facts)
        }

        /// Every accepted task has exactly one row, stamped for that task.
        fn assert_one_row_per_task(report: &super::super::BatchFanoutReport, accepted: usize) {
            let mut ids: Vec<usize> = report
                .results
                .iter()
                .map(|row| row.task_id().expect("batch rows are stamped").0)
                .collect();
            ids.sort_unstable();
            assert_eq!(ids, (0..accepted).collect::<Vec<_>>());
        }

        fn make_func(name: &str, address: u64, size: u64) -> FunctionInfo {
            FunctionInfo {
                name: name.to_string(),
                address,
                size,
                ..Default::default()
            }
        }

        fn fake_success(func: &FunctionInfo, code: &str) -> FunctionRenderResult {
            record_into_function_render_result(
                CliRustDecompileRecord {
                    func: func.clone(),
                    outcome: CliRustOutcome::Success {
                        code: code.to_string(),
                        code_prehir: None,
                        fell_back: false,
                        fallback_reason: None,
                        build_stats: None,
                        hint_stats: None,
                        decomp_sec: 0.001,
                        variables: Vec::new(),
                        line_mappings: Vec::new(),
                        globals: Vec::new(),
                        frame: Vec::new(),
                        declared_callees: Vec::new(),
                    },
                },
                None,
                false,
            )
        }

        fn fake_hard_error(func: &FunctionInfo, err: &str) -> FunctionRenderResult {
            record_into_function_render_result(
                CliRustDecompileRecord {
                    func: func.clone(),
                    outcome: CliRustOutcome::HardError {
                        error_text: err.to_string(),
                        decomp_sec: 0.001,
                    },
                },
                None,
                false,
            )
        }

        #[test]
        fn aliased_functions_correlate_by_task_id_and_order_deterministically() {
            // Two aliases share 0x1000; alias_second has larger size so LPT schedules
            // it first, AND workers deliver completions in reverse arrival order.
            let functions = vec![
                make_func("fn_other", 0x1020, 4),
                make_func("alias_first", 0x1000, 8),
                make_func("alias_second", 0x1000, 16),
            ];
            let (binary, facts) = make_test_binary(functions.clone());
            let dequeue_order = Arc::new(Mutex::new(Vec::<(usize, u64)>::new()));
            let dequeue_order_ref = Arc::clone(&dequeue_order);

            let report = run_worker_fanout_fanin_with_hooks(
                binary,
                facts,
                &functions,
                test_config(),
                1,
                8 * 1024 * 1024,
                move |ctx: WorkerSupervisorContext| {
                    let dequeue_order = Arc::clone(&dequeue_order_ref);
                    thread::Builder::new().spawn(move || {
                        let mut buffered = Vec::new();
                        loop {
                            let task = match ctx.task_rx.lock() {
                                Ok(locked) => locked.recv(),
                                Err(_) => break,
                            };
                            let Ok(task) = task else { break };
                            dequeue_order
                                .lock()
                                .unwrap()
                                .push((task.task_id.0, task.func.address));
                            ctx.progress.try_claim(task.task_id, ctx.worker);
                            let mut rendered =
                                fake_success(&task.func, &format!("// {}", task.func.name));
                            rendered = task.stamp(rendered);
                            buffered.push(WorkerCompletion {
                                worker: ctx.worker,
                                task_id: task.task_id,
                                result: rendered,
                            });
                        }
                        // Send in reverse of LPT order to prove arrival order cannot perturb output.
                        for completion in buffered.into_iter().rev() {
                            let _ = ctx.result_tx.send(completion);
                        }
                    })
                },
                Arc::new(AtomicBool::new(false)),
                |_, _| Ok(()),
            );

            // LPT order: alias_second (size 16), alias_first (size 8), fn_other (size 4).
            assert_eq!(
                *dequeue_order.lock().unwrap(),
                vec![(2, 0x1000), (1, 0x1000), (0, 0x1020)]
            );
            // Presentation order: (address ASC, original task_id ASC) -> alias_first (1) before alias_second (2), then fn_other (0).
            assert_eq!(report.results.len(), 3);
            assert_eq!(report.results[0].task_id(), Some(BatchTaskId(1)));
            assert_eq!(report.results[0].plain_output, "// alias_first");
            assert_eq!(report.results[1].task_id(), Some(BatchTaskId(2)));
            assert_eq!(report.results[1].plain_output, "// alias_second");
            assert_eq!(report.results[2].task_id(), Some(BatchTaskId(0)));
            assert_eq!(report.results[2].plain_output, "// fn_other");
            assert!(report.accounting.reconciles());
            assert_eq!(report.accounting.accepted, 3);
            assert_eq!(report.accounting.analysis_completed, 3);
            assert!(!report.accounting.has_infrastructure_anomalies());
        }

        /// Two accepted tasks share one entry address (aliases). A worker
        /// that routes each result under the other alias's task id must not
        /// get either result accepted: the address alone cannot tell the
        /// aliases apart.
        #[test]
        fn swapped_task_ids_with_identical_addresses_are_rejected() {
            let functions = vec![
                make_func("alias_first", 0x1000, 8),
                make_func("alias_second", 0x1000, 8),
            ];
            let (binary, facts) = make_test_binary(functions.clone());

            let report = run_worker_fanout_fanin_with_hooks(
                binary,
                facts,
                &functions,
                test_config(),
                1,
                8 * 1024 * 1024,
                |ctx: WorkerSupervisorContext| {
                    thread::Builder::new().spawn(move || {
                        let mut rendered = Vec::new();
                        while let Ok(task) = ctx.task_rx.lock().unwrap().recv() {
                            ctx.progress.try_claim(task.task_id, ctx.worker);
                            let mut result =
                                fake_success(&task.func, &format!("// {}", task.func.name));
                            result = task.stamp(result);
                            rendered.push((task.task_id, result));
                        }
                        // Route each payload under the *other* alias's id.
                        let ids: Vec<BatchTaskId> = rendered.iter().map(|(id, _)| *id).collect();
                        for (idx, (_, result)) in rendered.into_iter().enumerate() {
                            let _ = ctx.result_tx.send(WorkerCompletion {
                                worker: ctx.worker,
                                task_id: ids[1 - idx],
                                result,
                            });
                        }
                    })
                },
                Arc::new(AtomicBool::new(false)),
                |_, _| Ok(()),
            );

            assert_eq!(report.results.len(), 2);
            for (row, own) in report.results.iter().zip(["alias_first", "alias_second"]) {
                let other = if own == "alias_first" {
                    "alias_second"
                } else {
                    "alias_first"
                };
                assert_ne!(
                    row.plain_output,
                    format!("// {other}"),
                    "row for {own} carries the other alias's result"
                );
            }
            assert_eq!(report.accounting.analysis_completed, 0);
            assert_eq!(report.accounting.infrastructure_failed, 2);
            assert_eq!(report.accounting.identity_mismatches, 2);
            assert!(report.accounting.reconciles());
            assert_one_row_per_task(&report, 2);
            for row in &report.results {
                assert_eq!(row.json_entry["job_status"], "infrastructure_failed");
                assert!(
                    row.json_entry["error"]
                        .as_str()
                        .is_some_and(|e| e.contains("rendered for a different task")),
                    "{}",
                    row.json_entry["error"]
                );
            }
        }

        /// `--addresses-file` listing one address twice yields two accepted
        /// tasks with byte-identical metadata. They are still two tasks: a
        /// swapped routing is caught by task identity, not by comparing
        /// names or addresses, and correct routing keeps both rows.
        #[test]
        fn byte_identical_duplicate_tasks_keep_distinct_identity() {
            let functions = vec![make_func("dup", 0x1000, 8), make_func("dup", 0x1000, 8)];
            let (binary, facts) = make_test_binary(functions.clone());

            for swap in [true, false] {
                let report = run_worker_fanout_fanin_with_hooks(
                    Arc::clone(&binary),
                    Arc::clone(&facts),
                    &functions,
                    test_config(),
                    1,
                    8 * 1024 * 1024,
                    move |ctx: WorkerSupervisorContext| {
                        thread::Builder::new().spawn(move || {
                            let mut rendered = Vec::new();
                            while let Ok(task) = ctx.task_rx.lock().unwrap().recv() {
                                assert!(ctx.progress.try_claim(task.task_id, ctx.worker));
                                let result = task.stamp(fake_success(
                                    &task.func,
                                    &format!("// task {}", task.task_id.0),
                                ));
                                rendered.push((task.task_id, result));
                            }
                            let ids: Vec<BatchTaskId> =
                                rendered.iter().map(|(id, _)| *id).collect();
                            for (idx, (own, result)) in rendered.into_iter().enumerate() {
                                let _ = ctx.result_tx.send(WorkerCompletion {
                                    worker: ctx.worker,
                                    task_id: if swap { ids[1 - idx] } else { own },
                                    result,
                                });
                            }
                        })
                    },
                    Arc::new(AtomicBool::new(false)),
                    |_, _| Ok(()),
                );

                assert_one_row_per_task(&report, 2);
                assert!(report.accounting.reconciles());
                if swap {
                    assert_eq!(report.accounting.identity_mismatches, 2);
                    assert_eq!(report.accounting.infrastructure_failed, 2);
                } else {
                    assert_eq!(report.accounting.identity_mismatches, 0);
                    assert_eq!(report.results[0].plain_output, "// task 0");
                    assert_eq!(report.results[1].plain_output, "// task 1");
                }
            }
        }

        /// A completion for a task no worker has claimed (still queued) is
        /// not a legal lifecycle transition; accepting it let a forged row
        /// stand in for a function that never ran.
        #[test]
        fn completion_for_unclaimed_task_is_rejected() {
            let functions = vec![make_func("fn_0", 0x1000, 16), make_func("fn_1", 0x1020, 8)];
            let (binary, facts) = make_test_binary(functions.clone());
            let forged_target = functions[1].clone();

            let report = run_worker_fanout_fanin_with_hooks(
                binary,
                facts,
                &functions,
                test_config(),
                1,
                8 * 1024 * 1024,
                move |ctx: WorkerSupervisorContext| {
                    let forged_target = forged_target.clone();
                    thread::Builder::new().spawn(move || {
                        // Runs only the first (largest) task, fn_0.
                        let Ok(task) = ctx.task_rx.lock().unwrap().recv() else {
                            return;
                        };
                        assert!(ctx.progress.try_claim(task.task_id, ctx.worker));
                        // fn_1 (task 1) stays queued: nobody ever runs it.
                        let fake = fake_success(&forged_target, "forged");
                        let _ = ctx.result_tx.send(WorkerCompletion {
                            worker: ctx.worker,
                            task_id: BatchTaskId(1),
                            result: fake,
                        });
                        let real = task.stamp(fake_success(&task.func, "real:fn_0"));
                        let _ = ctx.result_tx.send(WorkerCompletion {
                            worker: ctx.worker,
                            task_id: task.task_id,
                            result: real,
                        });
                    })
                },
                Arc::new(AtomicBool::new(false)),
                |_, _| Ok(()),
            );

            assert_eq!(report.results[0].plain_output, "real:fn_0");
            assert_ne!(report.results[1].plain_output, "forged");
            assert_eq!(report.results[1].json_entry["job_status"], "not_started");
            assert_eq!(report.accounting.analysis_completed, 1);
            assert_eq!(report.accounting.not_started, 1);
            assert_eq!(report.accounting.unexpected_completions, 1);
            assert!(report.accounting.reconciles());
            assert_one_row_per_task(&report, 2);
        }

        /// A completion for a running task sent by a worker that did not
        /// claim it is rejected even when its payload is correctly stamped;
        /// the claimant's own completion is the one that counts.
        #[test]
        fn completion_from_non_claimant_worker_is_rejected() {
            let functions = vec![make_func("fn_0", 0x1000, 16)];
            let (binary, facts) = make_test_binary(functions.clone());
            let handoff = Arc::new(Mutex::new(None::<super::super::ScheduledTask>));
            let forged_sent = Arc::new(AtomicBool::new(false));

            let report = run_worker_fanout_fanin_with_hooks(
                binary,
                facts,
                &functions,
                test_config(),
                2,
                8 * 1024 * 1024,
                move |ctx: WorkerSupervisorContext| {
                    let handoff = Arc::clone(&handoff);
                    let forged_sent = Arc::clone(&forged_sent);
                    thread::Builder::new().spawn(move || {
                        if ctx.worker_idx == 1 {
                            // Never claims anything; forges worker 0's task.
                            let task = loop {
                                if let Some(task) = handoff.lock().unwrap().clone() {
                                    break task;
                                }
                                thread::yield_now();
                            };
                            let forged = task.stamp(fake_success(&task.func, "forged"));
                            let _ = ctx.result_tx.send(WorkerCompletion {
                                worker: ctx.worker,
                                task_id: task.task_id,
                                result: forged,
                            });
                            forged_sent.store(true, Ordering::Release);
                            return;
                        }
                        let Ok(task) = ctx.task_rx.lock().unwrap().recv() else {
                            return;
                        };
                        assert!(ctx.progress.try_claim(task.task_id, ctx.worker));
                        *handoff.lock().unwrap() = Some(task.clone());
                        while !forged_sent.load(Ordering::Acquire) {
                            thread::yield_now();
                        }
                        let real = task.stamp(fake_success(&task.func, "real"));
                        let _ = ctx.result_tx.send(WorkerCompletion {
                            worker: ctx.worker,
                            task_id: task.task_id,
                            result: real,
                        });
                    })
                },
                Arc::new(AtomicBool::new(false)),
                |_, _| Ok(()),
            );

            assert_eq!(report.results[0].plain_output, "real");
            assert_eq!(report.accounting.unexpected_completions, 1);
            assert_eq!(report.accounting.duplicate_completions, 0);
            assert_eq!(report.accounting.analysis_completed, 1);
            assert!(report.accounting.reconciles());
            assert_one_row_per_task(&report, 1);
        }

        /// The tracker admits only `ACCEPTED -> QUEUED -> RUNNING | CANCELLED
        /// | SEND_FAILED`; a second claim, a claim of an unqueued task, or a
        /// cancel after a claim is refused and counted.
        #[test]
        fn progress_tracker_refuses_illegal_transitions() {
            let tracker = super::super::BatchProgressTracker::new(2);
            let (t0, t1) = (BatchTaskId(0), BatchTaskId(1));
            let (w0, w1) = (super::super::WorkerId(0), super::super::WorkerId(1));
            assert!(!tracker.try_claim(t0, w0), "claim before queueing");
            assert!(tracker.mark_queued(t0));
            assert!(!tracker.mark_queued(t0), "queued twice");
            assert!(tracker.try_claim(t0, w0));
            assert!(!tracker.try_claim(t0, w1), "second claimant");
            assert!(!tracker.mark_cancelled(t0, w0), "cancel after claim");
            assert!(!tracker.mark_send_failed(t0), "send failure after claim");
            assert_eq!(
                tracker.snapshot(t0),
                (super::super::STATE_RUNNING, Some(w0))
            );
            assert!(!tracker.try_claim(BatchTaskId(7), w0), "out of range");
            assert!(tracker.mark_queued(t1));
            assert!(tracker.mark_cancelled(t1, w1));
            assert!(!tracker.try_claim(t1, w1), "claim after cancel");
            assert_eq!(tracker.rejected_transitions(), 7);
        }

        /// A panic inside the render is the function's analysis failure, not
        /// a job-infrastructure failure: the attempt ran.
        #[test]
        fn render_panic_is_an_analysis_failure() {
            let func = make_func("fn_panics", 0x1000, 8);
            let (binary, _facts) = make_test_binary(vec![func.clone()]);
            let result = super::super::render_catching_panic(&binary, &func, test_config(), || {
                panic!("simulated decompiler defect")
            });
            assert_eq!(result.outcome_kind, RenderOutcomeKind::AnalysisAborted);
            assert_eq!(result.json_entry["failure_domain"], "analysis");
            assert_eq!(result.json_entry["analysis_status"], "panicked");
            assert!(result.json_entry.get("job_status").is_none());
            // The pre-split value is kept for existing consumers.
            assert_eq!(
                result.json_entry["fallback_reason"],
                "rust_sleigh:worker_internal_error"
            );
        }

        /// A render that outlives its time budget is the function's analysis
        /// outcome as well.
        #[test]
        fn render_timeout_is_an_analysis_failure() {
            let func = make_func("fn_slow", 0x1000, 8);
            let (binary, _facts) = make_test_binary(vec![func.clone()]);
            let slow_func = func.clone();
            let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
            let mut config = test_config();
            config.timeout_ms = Some(20);
            let result = super::super::render_on_large_stack_with(
                binary,
                &func,
                config,
                8 * 1024 * 1024,
                move || {
                    // Outlives the budget until the test releases it.
                    let _ = release_rx.recv();
                    fake_success(&slow_func, "too late")
                },
            );
            drop(release_tx);
            assert_eq!(result.outcome_kind, RenderOutcomeKind::AnalysisAborted);
            assert_eq!(result.json_entry["failure_domain"], "analysis");
            assert_eq!(result.json_entry["analysis_status"], "timed_out");
        }

        #[test]
        fn supervisor_panic_after_dequeue_is_observed_and_reconciled() {
            let functions = vec![
                make_func("fn_big_panic", 0x1000, 32),
                make_func("fn_ok_1", 0x1020, 8),
                make_func("fn_ok_2", 0x1040, 4),
            ];
            let (binary, facts) = make_test_binary(functions.clone());
            let gate = Arc::new(AtomicBool::new(false));
            let rendered_counts = Arc::new(Mutex::new([0usize; 3]));
            let rendered_counts_ref = Arc::clone(&rendered_counts);

            let report = run_worker_fanout_fanin_with_hooks(
                binary,
                facts,
                &functions,
                test_config(),
                2,
                8 * 1024 * 1024,
                move |ctx: WorkerSupervisorContext| {
                    let gate = Arc::clone(&gate);
                    let rendered_counts = Arc::clone(&rendered_counts_ref);
                    thread::Builder::new().spawn(move || {
                        loop {
                            if ctx.worker_idx == 1 {
                                while !gate.load(Ordering::Acquire) {
                                    thread::yield_now();
                                }
                            }
                            let task = match ctx.task_rx.lock() {
                                Ok(locked) => locked.recv(),
                                Err(_) => break,
                            };
                            let Ok(task) = task else { break };
                            ctx.progress.try_claim(task.task_id, ctx.worker);
                            rendered_counts.lock().unwrap()[task.task_id.0] += 1;
                            if ctx.worker_idx == 0 {
                                gate.store(true, Ordering::Release);
                                panic!("simulated supervisor crash after dequeue");
                            }
                            let mut rendered =
                                fake_success(&task.func, &format!("// {}", task.func.name));
                            rendered = task.stamp(rendered);
                            let _ = ctx.result_tx.send(WorkerCompletion {
                                worker: ctx.worker,
                                task_id: task.task_id,
                                result: rendered,
                            });
                        }
                    })
                },
                Arc::new(AtomicBool::new(false)),
                |_, _| Ok(()),
            );

            assert_eq!(report.results.len(), 3);
            assert_eq!(report.accounting.accepted, 3);
            assert_eq!(report.accounting.analysis_completed, 2);
            assert_eq!(report.accounting.infrastructure_failed, 1);
            assert_eq!(report.accounting.synthesized_rows, 1);
            assert_eq!(report.accounting.join_errors, 1);
            assert!(report.accounting.reconciles());
            // Each task was attempted at most once (no implicit retry of the panicked task).
            assert_eq!(*rendered_counts.lock().unwrap(), [1, 1, 1]);

            let panicked_row = &report.results[0];
            assert_eq!(panicked_row.task_id(), Some(BatchTaskId(0)));
            assert_eq!(
                panicked_row.json_entry["failure_domain"],
                "job_infrastructure"
            );
            assert_eq!(
                panicked_row.json_entry["job_status"],
                "infrastructure_failed"
            );
            assert_eq!(
                panicked_row.json_entry["fallback_reason"],
                "rust_sleigh:worker_internal_error"
            );
        }

        #[test]
        fn partial_spawn_failure_runs_on_surviving_workers() {
            let functions = vec![make_func("fn_a", 0x1000, 16), make_func("fn_b", 0x1020, 8)];
            let (binary, facts) = make_test_binary(functions.clone());

            let report = run_worker_fanout_fanin_with_hooks(
                binary,
                facts,
                &functions,
                test_config(),
                2,
                8 * 1024 * 1024,
                |ctx: WorkerSupervisorContext| {
                    if ctx.worker_idx == 0 {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::WouldBlock,
                            "simulated partial thread spawn error",
                        ));
                    }
                    thread::Builder::new().spawn(move || {
                        while let Ok(task) = ctx.task_rx.lock().unwrap().recv() {
                            ctx.progress.try_claim(task.task_id, ctx.worker);
                            let mut rendered = fake_success(&task.func, "ok");
                            rendered = task.stamp(rendered);
                            let _ = ctx.result_tx.send(WorkerCompletion {
                                worker: ctx.worker,
                                task_id: task.task_id,
                                result: rendered,
                            });
                        }
                    })
                },
                Arc::new(AtomicBool::new(false)),
                |_, _| Ok(()),
            );

            assert_eq!(report.results.len(), 2);
            assert!(!report.accounting.sequential_fallback);
            assert_eq!(report.accounting.spawn_errors, 1);
            assert_eq!(report.accounting.analysis_completed, 2);
            assert_eq!(report.accounting.synthesized_rows, 0);
            assert!(report.accounting.reconciles());
        }

        #[test]
        fn unexpected_channel_close_and_send_failure_synthesize_not_started_rows() {
            let functions = vec![
                make_func("fn_0", 0x1000, 20),
                make_func("fn_1", 0x1020, 10),
                make_func("fn_2", 0x1040, 5),
            ];
            let (binary, facts) = make_test_binary(functions.clone());

            // Worker exits before draining any tasks -> channel disconnects while queueing.
            let immediate_exit = run_worker_fanout_fanin_with_hooks(
                Arc::clone(&binary),
                Arc::clone(&facts),
                &functions,
                test_config(),
                1,
                8 * 1024 * 1024,
                |ctx: WorkerSupervisorContext| {
                    thread::Builder::new().spawn(move || {
                        drop(ctx);
                    })
                },
                Arc::new(AtomicBool::new(false)),
                |_, _| Ok(()),
            );

            assert_eq!(immediate_exit.results.len(), 3);
            assert_eq!(immediate_exit.accounting.accepted, 3);
            assert_eq!(immediate_exit.accounting.not_started, 3);
            assert_eq!(immediate_exit.accounting.synthesized_rows, 3);
            assert!(immediate_exit.accounting.send_errors <= 1);
            assert!(immediate_exit.accounting.reconciles());
            for row in &immediate_exit.results {
                assert_eq!(row.json_entry["failure_domain"], "job_infrastructure");
                assert_eq!(row.json_entry["job_status"], "not_started");
                assert_eq!(
                    row.json_entry["fallback_reason"],
                    "rust_sleigh:worker_not_started"
                );
            }

            // Worker completes 1 task then closes channel unexpectedly while queued work remains.
            let mid_exit = run_worker_fanout_fanin_with_hooks(
                binary,
                facts,
                &functions,
                test_config(),
                1,
                8 * 1024 * 1024,
                |ctx: WorkerSupervisorContext| {
                    thread::Builder::new().spawn(move || {
                        if let Ok(task) = ctx.task_rx.lock().unwrap().recv() {
                            ctx.progress.try_claim(task.task_id, ctx.worker);
                            let mut rendered = fake_success(&task.func, "first");
                            rendered = task.stamp(rendered);
                            let _ = ctx.result_tx.send(WorkerCompletion {
                                worker: ctx.worker,
                                task_id: task.task_id,
                                result: rendered,
                            });
                        }
                        // Drop `task_rx` and `result_tx` cleanly with remaining work unstarted.
                    })
                },
                Arc::new(AtomicBool::new(false)),
                |_, _| Ok(()),
            );

            assert_eq!(mid_exit.results.len(), 3);
            assert_eq!(mid_exit.accounting.analysis_completed, 1);
            assert_eq!(mid_exit.accounting.not_started, 2);
            assert_eq!(mid_exit.accounting.synthesized_rows, 2);
            assert_eq!(mid_exit.accounting.join_errors, 0);
            assert!(mid_exit.accounting.reconciles());
        }

        #[test]
        fn duplicate_and_stale_completions_are_rejected_without_overwriting() {
            let functions = vec![
                make_func("fn_first", 0x1000, 16),
                make_func("fn_second", 0x1020, 8),
            ];
            let (binary, facts) = make_test_binary(functions.clone());

            let report = run_worker_fanout_fanin_with_hooks(
                binary,
                facts,
                &functions,
                test_config(),
                1,
                8 * 1024 * 1024,
                |ctx: WorkerSupervisorContext| {
                    thread::Builder::new().spawn(move || {
                        let mut injected = false;
                        while let Ok(task) = ctx.task_rx.lock().unwrap().recv() {
                            ctx.progress.try_claim(task.task_id, ctx.worker);
                            if !injected {
                                injected = true;
                                // Stale completion #1: out-of-range TaskId.
                                let _ = ctx.result_tx.send(WorkerCompletion {
                                    worker: ctx.worker,
                                    task_id: BatchTaskId(99),
                                    result: fake_success(&task.func, "stale_id"),
                                });
                                // Mismatch: payload rendered for another function.
                                let wrong_addr_func = make_func("wrong", 0x9999, 4);
                                let _ = ctx.result_tx.send(WorkerCompletion {
                                    worker: ctx.worker,
                                    task_id: task.task_id,
                                    result: task
                                        .stamp(fake_success(&wrong_addr_func, "stale_addr")),
                                });
                            }

                            let valid = task.stamp(fake_success(
                                &task.func,
                                &format!("first:{}", task.func.name),
                            ));
                            let _ = ctx.result_tx.send(WorkerCompletion {
                                worker: ctx.worker,
                                task_id: task.task_id,
                                result: valid,
                            });
                            // Duplicate completion with mutated payload must be ignored.
                            let dup = task.stamp(fake_success(
                                &task.func,
                                &format!("dup:{}", task.func.name),
                            ));
                            let _ = ctx.result_tx.send(WorkerCompletion {
                                worker: ctx.worker,
                                task_id: task.task_id,
                                result: dup,
                            });
                        }
                    })
                },
                Arc::new(AtomicBool::new(false)),
                |_, _| Ok(()),
            );

            assert_eq!(report.results.len(), 2);
            assert_eq!(report.results[0].plain_output, "first:fn_first");
            assert_eq!(report.results[1].plain_output, "first:fn_second");
            assert_eq!(report.accounting.stale_completions, 1);
            assert_eq!(report.accounting.identity_mismatches, 1);
            assert_eq!(report.accounting.duplicate_completions, 2);
            assert_eq!(report.accounting.analysis_completed, 2);
            assert!(report.accounting.reconciles());
        }

        #[test]
        fn cancellation_synthesizes_cancelled_rows_for_unstarted_tasks() {
            let functions = vec![
                make_func("fn_0", 0x1000, 30),
                make_func("fn_1", 0x1020, 20),
                make_func("fn_2", 0x1040, 10),
            ];
            let (binary, facts) = make_test_binary(functions.clone());
            let cancel = Arc::new(AtomicBool::new(false));
            let cancel_for_worker = Arc::clone(&cancel);

            let report = run_worker_fanout_fanin_with_hooks(
                binary,
                facts,
                &functions,
                test_config(),
                1,
                8 * 1024 * 1024,
                move |ctx: WorkerSupervisorContext| {
                    let cancel_ref = Arc::clone(&cancel_for_worker);
                    thread::Builder::new().spawn(move || {
                        if let Ok(task) = ctx.task_rx.lock().unwrap().recv() {
                            ctx.progress.try_claim(task.task_id, ctx.worker);
                            let mut rendered = fake_success(&task.func, "done");
                            rendered = task.stamp(rendered);
                            let _ = ctx.result_tx.send(WorkerCompletion {
                                worker: ctx.worker,
                                task_id: task.task_id,
                                result: rendered,
                            });
                            cancel_ref.store(true, Ordering::Release);
                        }
                        while let Ok(task) = ctx.task_rx.lock().unwrap().recv() {
                            if ctx.cancel.load(Ordering::Acquire) {
                                ctx.progress.mark_cancelled(task.task_id, ctx.worker);
                                break;
                            }
                        }
                    })
                },
                cancel,
                |_, _| Ok(()),
            );

            assert_eq!(report.results.len(), 3);
            assert_eq!(report.accounting.accepted, 3);
            assert_eq!(report.accounting.analysis_completed, 1);
            assert_eq!(report.accounting.cancelled, 2);
            assert_eq!(report.accounting.synthesized_rows, 2);
            assert!(report.accounting.reconciles());
            assert_eq!(report.results[0].json_entry["code"], "done");
            for cancelled_row in &report.results[1..] {
                assert_eq!(
                    cancelled_row.json_entry["failure_domain"],
                    "job_infrastructure"
                );
                assert_eq!(cancelled_row.json_entry["job_status"], "cancelled");
                assert_eq!(
                    cancelled_row.json_entry["fallback_reason"],
                    "rust_sleigh:worker_cancelled"
                );
            }
        }

        #[test]
        fn result_consumer_failure_reconciles_all_tasks_and_joins_workers() {
            let functions = vec![
                make_func("fn_0", 0x1000, 30),
                make_func("fn_1", 0x1020, 20),
                make_func("fn_2", 0x1040, 10),
            ];
            let (binary, facts) = make_test_binary(functions.clone());
            let seen = Arc::new(AtomicUsize::new(0));
            let joined_count = Arc::new(AtomicUsize::new(0));
            let joined_ref = Arc::clone(&joined_count);

            let report = run_worker_fanout_fanin_with_hooks(
                binary,
                facts,
                &functions,
                test_config(),
                1,
                8 * 1024 * 1024,
                move |ctx: WorkerSupervisorContext| {
                    let joined = Arc::clone(&joined_ref);
                    thread::Builder::new().spawn(move || {
                        loop {
                            if ctx.stop_dispatch.load(Ordering::Acquire) {
                                break;
                            }
                            let Ok(task) = ctx.task_rx.lock().unwrap().recv() else {
                                break;
                            };
                            if ctx.stop_dispatch.load(Ordering::Acquire) {
                                break;
                            }
                            ctx.progress.try_claim(task.task_id, ctx.worker);
                            let mut rendered = fake_success(&task.func, "rendered");
                            rendered = task.stamp(rendered);
                            if ctx
                                .result_tx
                                .send(WorkerCompletion {
                                    worker: ctx.worker,
                                    task_id: task.task_id,
                                    result: rendered,
                                })
                                .is_err()
                            {
                                break;
                            }
                        }
                        joined.fetch_add(1, Ordering::Release);
                    })
                },
                Arc::new(AtomicBool::new(false)),
                move |_, _| {
                    if seen.fetch_add(1, Ordering::SeqCst) == 1 {
                        Err("simulated sink write failure".to_string())
                    } else {
                        Ok(())
                    }
                },
            );

            assert_eq!(joined_count.load(Ordering::Acquire), 1);
            assert_eq!(report.results.len(), 3);
            assert_eq!(report.accounting.accepted, 3);
            assert_eq!(report.accounting.analysis_completed, 1);
            assert_eq!(report.accounting.consumer_errors, 1);
            assert_eq!(
                report.accounting.infrastructure_failed + report.accounting.not_started,
                2
            );
            assert_eq!(report.accounting.synthesized_rows, 2);
            assert!(report.accounting.reconciles());
        }

        #[test]
        fn analysis_failure_is_distinct_from_job_infrastructure_failure() {
            let functions = vec![
                make_func("fn_analysis_err", 0x1000, 16),
                make_func("fn_worker_panic", 0x1020, 8),
            ];
            let (binary, facts) = make_test_binary(functions.clone());

            let report = run_worker_fanout_fanin_with_hooks(
                binary,
                facts,
                &functions,
                test_config(),
                1,
                8 * 1024 * 1024,
                |ctx: WorkerSupervisorContext| {
                    thread::Builder::new().spawn(move || {
                        while let Ok(task) = ctx.task_rx.lock().unwrap().recv() {
                            ctx.progress.try_claim(task.task_id, ctx.worker);
                            if task.task_id == BatchTaskId(0) {
                                let mut rendered = fake_hard_error(&task.func, "bad pcode");
                                rendered = task.stamp(rendered);
                                let _ = ctx.result_tx.send(WorkerCompletion {
                                    worker: ctx.worker,
                                    task_id: task.task_id,
                                    result: rendered,
                                });
                            } else {
                                panic!("supervisor died on task 1");
                            }
                        }
                    })
                },
                Arc::new(AtomicBool::new(false)),
                |_, _| Ok(()),
            );

            assert_eq!(report.results.len(), 2);
            assert_eq!(report.accounting.analysis_completed, 1);
            assert_eq!(report.accounting.infrastructure_failed, 1);
            assert_eq!(report.accounting.join_errors, 1);
            assert!(report.accounting.reconciles());

            // Analysis failure keeps normal `rust_sleigh: ...` fallback_reason without job_infrastructure tag.
            let analysis_row = &report.results[0].json_entry;
            assert_eq!(analysis_row["fallback_reason"], "rust_sleigh: bad pcode");
            assert!(analysis_row.get("failure_domain").is_none());
            assert!(analysis_row.get("job_status").is_none());

            // Infrastructure failure carries explicit job failure classification.
            let infra_row = &report.results[1].json_entry;
            assert_eq!(infra_row["failure_domain"], "job_infrastructure");
            assert_eq!(infra_row["job_status"], "infrastructure_failed");
            assert_eq!(
                infra_row["fallback_reason"],
                "rust_sleigh:worker_internal_error"
            );
        }

        #[test]
        fn uncooperative_stuck_jobs_stay_bounded_and_release_arcs_after_unblock() {
            use std::sync::Condvar;
            use std::time::{Duration, Instant};

            let functions: Vec<_> = (0..6)
                .map(|i| make_func(&format!("stuck_{i}"), 0x1000 + (i as u64) * 0x20, 16))
                .collect();
            let (binary, facts) = make_test_binary(functions.clone());

            let gate = Arc::new((Mutex::new(false), Condvar::new()));
            let live_renders = Arc::new(AtomicUsize::new(0));
            let peak_renders = Arc::new(AtomicUsize::new(0));

            let gate_ref = Arc::clone(&gate);
            let live_ref = Arc::clone(&live_renders);
            let peak_ref = Arc::clone(&peak_renders);
            let render_fn: super::super::RenderFn = Arc::new(move |_bin, _facts, func, _config| {
                struct LiveGuard(Arc<AtomicUsize>);
                impl Drop for LiveGuard {
                    fn drop(&mut self) {
                        self.0.fetch_sub(1, Ordering::AcqRel);
                    }
                }
                let cur = live_ref.fetch_add(1, Ordering::AcqRel) + 1;
                peak_ref.fetch_max(cur, Ordering::AcqRel);
                let _guard = LiveGuard(Arc::clone(&live_ref));

                let (lock, cv) = &*gate_ref;
                let mut open = lock.lock().unwrap();
                while !*open {
                    open = cv.wait(open).unwrap();
                }
                fake_success(func, "unblocked too late")
            });

            let mut config = test_config();
            config.timeout_ms = Some(20);
            let policy = super::super::WorkerLifecyclePolicy {
                max_live_workers: 2,
                grace_period: Duration::from_millis(10),
                shutdown_grace: Duration::from_millis(10),
            };

            let report = super::super::run_worker_fanout_fanin_with_policy(
                Arc::clone(&binary),
                Arc::clone(&facts),
                &functions,
                config,
                2,
                8 * 1024 * 1024,
                policy,
                fission_core::CancellationToken::new(),
                render_fn,
            );

            assert_eq!(report.results.len(), 6);
            assert_eq!(report.accounting.accepted, 6);
            assert!(report.accounting.reconciles());
            assert!(
                report.accounting.peak_active_workers <= 2,
                "peak_active_workers={}",
                report.accounting.peak_active_workers
            );
            assert!(
                peak_renders.load(Ordering::Acquire) <= 2,
                "concurrent stuck renders exceeded cap: {}",
                peak_renders.load(Ordering::Acquire)
            );
            assert_eq!(report.accounting.still_running_workers, 2);
            assert!(
                report.accounting.quota_exhausted_tasks >= 1,
                "expected excess tasks to hit worker quota, accounting={:?}",
                report.accounting
            );
            assert!(
                report
                    .results
                    .iter()
                    .all(|r| r.json_entry.get("code").is_some()
                        && r.json_entry["fell_back"] == serde_json::json!(true)),
                "stuck/exhausted batch must not emit non-fallback Success"
            );
            assert!(
                report
                    .results
                    .iter()
                    .any(|r| r.json_entry["job_status"] == "worker_unavailable"
                        && r.json_entry["fallback_reason"] == "rust_sleigh:worker_unavailable")
            );
            // Retained Arcs are bounded by the 2 live workers + report's own provenance refs = none for binary/facts except 2 live workers + this test handle.
            assert!(Arc::strong_count(&binary) <= 5);
            assert!(Arc::strong_count(&facts) <= 3);

            // Release the test barrier and verify the abandoned threads exit and drop all retained Arcs.
            {
                let (lock, cv) = &*gate;
                *lock.lock().unwrap() = true;
                cv.notify_all();
            }
            let deadline = Instant::now() + Duration::from_secs(2);
            while (live_renders.load(Ordering::Acquire) > 0
                || Arc::strong_count(&binary) > 1
                || Arc::strong_count(&facts) > 1)
                && Instant::now() < deadline
            {
                thread::sleep(Duration::from_millis(5));
            }
            drop(report);
            assert_eq!(live_renders.load(Ordering::Acquire), 0);
            assert_eq!(Arc::strong_count(&binary), 1);
            assert_eq!(Arc::strong_count(&facts), 1);
        }

        #[test]
        fn cooperatively_cancellable_slow_jobs_reclaim_slots_and_allow_later_success() {
            use std::time::Duration;

            let functions = vec![
                make_func("slow_1", 0x1000, 40),
                make_func("slow_2", 0x1020, 30),
                make_func("slow_3", 0x1040, 20),
                make_func("fast_4", 0x1060, 10),
            ];
            let (binary, facts) = make_test_binary(functions.clone());

            let render_fn: super::super::RenderFn = Arc::new(|_bin, _facts, func, _config| {
                if func.name.starts_with("slow_") {
                    while !fission_core::cancellation::is_current_cancelled() {
                        thread::sleep(Duration::from_millis(2));
                    }
                    return fake_success(func, "should never publish cancelled slow result");
                }
                fake_success(func, "fast_ok")
            });

            let mut config = test_config();
            config.timeout_ms = Some(25);
            let policy = super::super::WorkerLifecyclePolicy {
                max_live_workers: 1,
                grace_period: Duration::from_millis(80),
                shutdown_grace: Duration::from_millis(100),
            };

            let report = super::super::run_worker_fanout_fanin_with_policy(
                Arc::clone(&binary),
                Arc::clone(&facts),
                &functions,
                config,
                1,
                8 * 1024 * 1024,
                policy,
                fission_core::CancellationToken::new(),
                render_fn,
            );

            assert_eq!(report.results.len(), 4);
            assert_eq!(report.accounting.accepted, 4);
            assert!(report.accounting.reconciles());
            assert_eq!(report.accounting.peak_active_workers, 1);
            assert_eq!(report.accounting.still_running_workers, 0);
            assert_eq!(report.accounting.quota_exhausted_tasks, 0);
            assert!(
                report.accounting.reclaimed_workers >= 3,
                "reclaimed_workers={}",
                report.accounting.reclaimed_workers
            );
            for row in &report.results[..3] {
                assert_eq!(row.json_entry["failure_domain"], "analysis");
                assert_eq!(row.json_entry["analysis_status"], "timed_out");
            }
            assert_eq!(report.results[3].plain_output, "fast_ok");
            assert_eq!(report.results[3].json_entry["fell_back"], false);

            drop(report);
            assert_eq!(Arc::strong_count(&binary), 1);
            assert_eq!(Arc::strong_count(&facts), 1);
        }

        #[test]
        fn unified_timeout_and_zero_timeout_semantics_across_all_lanes() {
            use std::time::Duration;

            let functions = vec![make_func("coop_slow", 0x1000, 16)];
            let (binary, facts) = make_test_binary(functions.clone());

            let coop_render: super::super::RenderFn = Arc::new(|_bin, _facts, func, _config| {
                for _ in 0..40 {
                    if fission_core::cancellation::is_current_cancelled() {
                        return fake_success(func, "late cancelled success");
                    }
                    thread::sleep(Duration::from_millis(2));
                }
                fake_success(func, "completed")
            });

            // 1. Sequential batch honors wall timeout cleanly.
            let mut timed_cfg = test_config();
            timed_cfg.timeout_ms = Some(15);
            let policy = super::super::WorkerLifecyclePolicy {
                max_live_workers: 1,
                grace_period: Duration::from_millis(80),
                shutdown_grace: Duration::from_millis(80),
            };
            let seq_timed = super::super::run_sequential_batch_with_policy(
                Arc::clone(&binary),
                Arc::clone(&facts),
                &functions,
                timed_cfg,
                8 * 1024 * 1024,
                policy,
                fission_core::CancellationToken::new(),
                Arc::clone(&coop_render),
            );
            assert_eq!(
                seq_timed.results[0].json_entry["analysis_status"],
                "timed_out"
            );
            assert_eq!(seq_timed.accounting.still_running_workers, 0);

            // 2. Sequential fallback (worker_count == 0) honors wall timeout cleanly.
            let zero_worker_timed = super::super::run_worker_fanout_fanin_with_policy(
                Arc::clone(&binary),
                Arc::clone(&facts),
                &functions,
                timed_cfg,
                0,
                8 * 1024 * 1024,
                policy,
                fission_core::CancellationToken::new(),
                Arc::clone(&coop_render),
            );
            assert!(zero_worker_timed.accounting.sequential_fallback);
            assert_eq!(
                zero_worker_timed.results[0].json_entry["analysis_status"],
                "timed_out"
            );

            // 3. Coordinator fallback after OS spawn failure still honors cooperative timeout.
            let spawn_fail_timed = super::super::run_worker_fanout_fanin_with_hooks_and_policy(
                Arc::clone(&binary),
                Arc::clone(&facts),
                &functions,
                timed_cfg,
                1,
                8 * 1024 * 1024,
                |_| {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::WouldBlock,
                        "simulated spawn fail",
                    ))
                },
                Arc::new(AtomicBool::new(false)),
                fission_core::CancellationToken::new(),
                policy,
                Arc::clone(&coop_render),
                |_, _| Ok(()),
            );
            assert!(spawn_fail_timed.accounting.sequential_fallback);
            assert_eq!(spawn_fail_timed.accounting.spawn_errors, 1);
            assert_eq!(
                spawn_fail_timed.results[0].json_entry["analysis_status"],
                "timed_out"
            );

            // 4. `--timeout-ms=0` disables wall deadline so a slow job finishes, while explicit cancel still aborts it.
            let mut zero_cfg = test_config();
            zero_cfg.timeout_ms = Some(0);
            let zero_ok = super::super::run_worker_fanout_fanin_with_policy(
                Arc::clone(&binary),
                Arc::clone(&facts),
                &functions,
                zero_cfg,
                1,
                8 * 1024 * 1024,
                policy,
                fission_core::CancellationToken::new(),
                Arc::clone(&coop_render),
            );
            assert_eq!(zero_ok.results[0].plain_output, "completed");

            let cancel_token = fission_core::CancellationToken::new();
            let canceller = cancel_token.clone();
            let race_render: super::super::RenderFn =
                Arc::new(move |_bin, _facts, func, _config| {
                    canceller.cancel();
                    fake_success(func, "must not publish cancelled success")
                });
            let cancelled = super::super::run_worker_fanout_fanin_with_policy(
                Arc::clone(&binary),
                Arc::clone(&facts),
                &functions,
                zero_cfg,
                1,
                8 * 1024 * 1024,
                policy,
                cancel_token,
                race_render,
            );
            assert_eq!(cancelled.results[0].json_entry["job_status"], "cancelled");

            // 5. Direct single-function large-stack wrapper returns cleanest outcome cleanly.
            let direct = super::super::render_one_function_on_large_stack(
                binary,
                facts,
                &functions[0],
                test_config(),
                8 * 1024 * 1024,
            );
            assert_eq!(direct.address, 0x1000);
        }
    }

    #[test]
    fn work_limits_default_when_nothing_is_configured_or_passed() {
        // the repository configuration sets no work limits
        assert_eq!(resolve_resource_limits(&[]), ResourceLimits::DEFAULT);
    }

    #[test]
    fn a_flag_overrides_only_its_own_limit() {
        let flags = ["sese_region_proof_calls=7".parse().unwrap()];
        let limits = resolve_resource_limits(&flags);
        assert_eq!(limits.sese_region_proof_calls, 7);
        assert_eq!(
            limits.structuring_total_work,
            ResourceLimits::DEFAULT.structuring_total_work
        );
    }
}
