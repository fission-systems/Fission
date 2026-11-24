use crate::cli::oneshot::rust_decomp::record::{InfrastructureFailureKind, RenderConfig};
use crate::cli::oneshot::rust_decomp::{
    FunctionRenderResult, RenderOutcomeKind, make_infrastructure_error_result,
    make_internal_error_result, render_one_function_inner,
};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::facts::FactStore;
use std::cmp::min;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

const DEFAULT_DECOMP_STACK_MB: usize = 32;

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

pub(crate) fn render_one_function_on_large_stack(
    binary: Arc<fission_loader::loader::LoadedBinary>,
    facts: Arc<FactStore>,
    func: &FunctionInfo,
    config: RenderConfig,
    stack_size_bytes: usize,
) -> FunctionRenderResult {
    let func_owned = func.clone();
    let func_for_error = func.clone();
    let binary_for_thread = Arc::clone(&binary);
    let facts_for_thread = Arc::clone(&facts);
    let (result_tx, result_rx) = mpsc::sync_channel::<FunctionRenderResult>(1);

    let spawn = thread::Builder::new()
        .name(format!("fission-rust-decomp-0x{:x}", func.address))
        .stack_size(stack_size_bytes)
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                render_one_function_inner(
                    binary_for_thread.as_ref(),
                    facts_for_thread.as_ref(),
                    &func_owned,
                    config,
                )
            }))
            .unwrap_or_else(|_| {
                make_internal_error_result(
                    binary_for_thread.as_ref(),
                    &func_owned,
                    "worker thread panicked while rendering function".to_string(),
                    config,
                )
            });
            let _ = result_tx.send(result);
        });

    match spawn {
        Ok(handle) => {
            if let Some(timeout_ms) = config.timeout_ms.filter(|timeout_ms| *timeout_ms > 0) {
                match result_rx.recv_timeout(Duration::from_millis(timeout_ms)) {
                    Ok(result) => {
                        let _ = handle.join();
                        result
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => make_internal_error_result(
                        binary.as_ref(),
                        &func_for_error,
                        render_timeout_message(timeout_ms),
                        config,
                    ),
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
                        let _ = handle.join();
                        make_internal_error_result(
                            binary.as_ref(),
                            &func_for_error,
                            "worker thread exited before returning a render result".to_string(),
                            config,
                        )
                    }
                }
            } else {
                match handle.join() {
                    Ok(()) => result_rx.recv().unwrap_or_else(|_| {
                        make_internal_error_result(
                            binary.as_ref(),
                            &func_for_error,
                            "worker thread exited before returning a render result".to_string(),
                            config,
                        )
                    }),
                    Err(_) => make_internal_error_result(
                        binary.as_ref(),
                        &func_for_error,
                        "worker thread panicked while rendering function".to_string(),
                        config,
                    ),
                }
            }
        }
        Err(err) => make_internal_error_result(
            binary.as_ref(),
            &func_for_error,
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
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        render_one_function_inner(binary, facts, func, config)
    }))
    .unwrap_or_else(|_| {
        make_internal_error_result(
            binary,
            func,
            "worker thread panicked while rendering function".to_string(),
            config,
        )
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct BatchTaskId(pub(crate) usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TaskTerminalKind {
    AnalysisCompleted,
    InfrastructureFailed,
    NotStarted,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TaskLifecycleState {
    Accepted,
    Queued,
    Running,
    Terminal(TaskTerminalKind),
}

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub(crate) struct BatchLedgerSummary {
    pub accepted: usize,
    pub analysis_completed: usize,
    pub infrastructure_failed: usize,
    pub not_started: usize,
    pub cancelled: usize,
    pub synthesized_rows: usize,
    pub duplicate_completions: usize,
    pub stale_completions: usize,
    pub spawn_errors: usize,
    pub send_errors: usize,
    pub join_errors: usize,
    pub consumer_errors: usize,
    pub sequential_fallback: bool,
}

impl BatchLedgerSummary {
    pub(crate) fn terminal_total(&self) -> usize {
        self.analysis_completed + self.infrastructure_failed + self.not_started + self.cancelled
    }

    pub(crate) fn reconciles(&self) -> bool {
        self.accepted == self.terminal_total()
    }

    pub(crate) fn has_infrastructure_anomalies(&self) -> bool {
        self.infrastructure_failed > 0
            || self.not_started > 0
            || self.cancelled > 0
            || self.synthesized_rows > 0
            || self.duplicate_completions > 0
            || self.stale_completions > 0
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

#[derive(Clone)]
struct ScheduledTask {
    task_id: BatchTaskId,
    func: FunctionInfo,
}

struct WorkerCompletion {
    task_id: BatchTaskId,
    result: FunctionRenderResult,
}

const PROGRESS_ACCEPTED: u8 = 0;
const PROGRESS_QUEUED: u8 = 1;
const PROGRESS_RUNNING: u8 = 2;
const PROGRESS_CANCELLED_BY_WORKER: u8 = 3;
const PROGRESS_SEND_FAILED: u8 = 4;

struct BatchProgressTracker {
    states: Vec<AtomicU8>,
}

impl BatchProgressTracker {
    fn new(len: usize) -> Self {
        Self {
            states: (0..len).map(|_| AtomicU8::new(PROGRESS_ACCEPTED)).collect(),
        }
    }

    fn mark_queued(&self, task_id: BatchTaskId) {
        if let Some(slot) = self.states.get(task_id.0) {
            slot.store(PROGRESS_QUEUED, Ordering::Release);
        }
    }

    fn mark_send_failed(&self, task_id: BatchTaskId) {
        if let Some(slot) = self.states.get(task_id.0) {
            slot.store(PROGRESS_SEND_FAILED, Ordering::Release);
        }
    }

    fn mark_running(&self, task_id: BatchTaskId) {
        if let Some(slot) = self.states.get(task_id.0) {
            slot.store(PROGRESS_RUNNING, Ordering::Release);
        }
    }

    fn mark_cancelled_by_worker(&self, task_id: BatchTaskId) {
        if let Some(slot) = self.states.get(task_id.0) {
            slot.store(PROGRESS_CANCELLED_BY_WORKER, Ordering::Release);
        }
    }

    fn get(&self, task_id: BatchTaskId) -> u8 {
        self.states
            .get(task_id.0)
            .map(|slot| slot.load(Ordering::Acquire))
            .unwrap_or(PROGRESS_ACCEPTED)
    }
}

struct TaskLedgerEntry {
    task_id: BatchTaskId,
    func: FunctionInfo,
    state: TaskLifecycleState,
    result: Option<FunctionRenderResult>,
}

struct BatchTaskLedger {
    entries: Vec<TaskLedgerEntry>,
    synthesized_rows: usize,
    duplicate_completions: usize,
    stale_completions: usize,
    spawn_errors: usize,
    send_errors: usize,
    join_errors: usize,
    consumer_errors: usize,
    sequential_fallback: bool,
}

impl BatchTaskLedger {
    fn new(functions: &[FunctionInfo]) -> Self {
        let entries = functions
            .iter()
            .enumerate()
            .map(|(idx, func)| TaskLedgerEntry {
                task_id: BatchTaskId(idx),
                func: func.clone(),
                state: TaskLifecycleState::Accepted,
                result: None,
            })
            .collect();
        Self {
            entries,
            synthesized_rows: 0,
            duplicate_completions: 0,
            stale_completions: 0,
            spawn_errors: 0,
            send_errors: 0,
            join_errors: 0,
            consumer_errors: 0,
            sequential_fallback: false,
        }
    }

    fn mark_queued(&mut self, task_id: BatchTaskId) {
        if let Some(entry) = self.entries.get_mut(task_id.0) {
            if matches!(entry.state, TaskLifecycleState::Accepted) {
                entry.state = TaskLifecycleState::Queued;
            }
        }
    }

    fn mark_running(&mut self, task_id: BatchTaskId) {
        if let Some(entry) = self.entries.get_mut(task_id.0) {
            if !matches!(entry.state, TaskLifecycleState::Terminal(_)) {
                entry.state = TaskLifecycleState::Running;
            }
        }
    }

    fn terminal_kind_for_outcome(outcome_kind: RenderOutcomeKind) -> TaskTerminalKind {
        match outcome_kind {
            RenderOutcomeKind::Analysis => TaskTerminalKind::AnalysisCompleted,
            RenderOutcomeKind::Infrastructure(InfrastructureFailureKind::WorkerInternalError) => {
                TaskTerminalKind::InfrastructureFailed
            }
            RenderOutcomeKind::Infrastructure(InfrastructureFailureKind::NotStarted) => {
                TaskTerminalKind::NotStarted
            }
            RenderOutcomeKind::Infrastructure(InfrastructureFailureKind::Cancelled) => {
                TaskTerminalKind::Cancelled
            }
        }
    }

    fn record_completion(
        &mut self,
        mut completion: WorkerCompletion,
        on_result: &mut impl FnMut(BatchTaskId, &FunctionRenderResult) -> Result<(), String>,
        binary: &LoadedBinary,
        config: RenderConfig,
    ) -> Result<(), ()> {
        let Some(entry) = self.entries.get_mut(completion.task_id.0) else {
            self.stale_completions += 1;
            return Ok(());
        };
        if matches!(entry.state, TaskLifecycleState::Terminal(_)) {
            self.duplicate_completions += 1;
            return Ok(());
        }
        if completion.result.address != entry.func.address {
            self.stale_completions += 1;
            return Ok(());
        }

        completion.result.task_id = Some(completion.task_id);
        match on_result(completion.task_id, &completion.result) {
            Ok(()) => {
                let kind = Self::terminal_kind_for_outcome(completion.result.outcome_kind);
                entry.result = Some(completion.result);
                entry.state = TaskLifecycleState::Terminal(kind);
                Ok(())
            }
            Err(consumer_err) => {
                self.consumer_errors += 1;
                self.synthesized_rows += 1;
                let mut synthesized = make_infrastructure_error_result(
                    binary,
                    &entry.func,
                    InfrastructureFailureKind::WorkerInternalError,
                    format!("result consumer failed: {consumer_err}"),
                    config,
                );
                synthesized.task_id = Some(entry.task_id);
                entry.result = Some(synthesized);
                entry.state = TaskLifecycleState::Terminal(TaskTerminalKind::InfrastructureFailed);
                Err(())
            }
        }
    }

    fn reconcile_missing(
        &mut self,
        progress: &BatchProgressTracker,
        cancelled: bool,
        binary: &LoadedBinary,
        config: RenderConfig,
    ) {
        let join_errors = self.join_errors;
        let consumer_errors = self.consumer_errors;
        for entry in &mut self.entries {
            if matches!(entry.state, TaskLifecycleState::Terminal(_)) {
                continue;
            }
            let raw_progress = progress.get(entry.task_id);
            if raw_progress == PROGRESS_RUNNING {
                entry.state = TaskLifecycleState::Running;
            }
            let (kind, message) = match raw_progress {
                PROGRESS_RUNNING => (
                    InfrastructureFailureKind::WorkerInternalError,
                    if join_errors > 0 {
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
                PROGRESS_CANCELLED_BY_WORKER => (
                    InfrastructureFailureKind::Cancelled,
                    "batch task cancelled before execution".to_string(),
                ),
                PROGRESS_SEND_FAILED => {
                    if cancelled {
                        (
                            InfrastructureFailureKind::Cancelled,
                            "batch task cancelled before worker dispatch".to_string(),
                        )
                    } else {
                        (
                            InfrastructureFailureKind::NotStarted,
                            "worker task queue closed before task could be dispatched".to_string(),
                        )
                    }
                }
                PROGRESS_QUEUED | PROGRESS_ACCEPTED | _ => {
                    if cancelled {
                        (
                            InfrastructureFailureKind::Cancelled,
                            "batch task cancelled before execution".to_string(),
                        )
                    } else if consumer_errors > 0 {
                        (
                            InfrastructureFailureKind::NotStarted,
                            "result consumer aborted batch before worker started task".to_string(),
                        )
                    } else if join_errors > 0 {
                        (
                            InfrastructureFailureKind::NotStarted,
                            format!(
                                "worker pool stopped after supervisor failure before starting task (join_errors={join_errors})"
                            ),
                        )
                    } else {
                        (
                            InfrastructureFailureKind::NotStarted,
                            "worker pool exited before starting accepted task".to_string(),
                        )
                    }
                }
            };

            let mut synthesized =
                make_infrastructure_error_result(binary, &entry.func, kind, message, config);
            synthesized.task_id = Some(entry.task_id);
            let terminal = Self::terminal_kind_for_outcome(synthesized.outcome_kind);
            entry.result = Some(synthesized);
            entry.state = TaskLifecycleState::Terminal(terminal);
            self.synthesized_rows += 1;
        }
    }

    fn into_report(mut self) -> BatchFanoutReport {
        let mut analysis_completed = 0usize;
        let mut infrastructure_failed = 0usize;
        let mut not_started = 0usize;
        let mut cancelled = 0usize;

        for entry in &self.entries {
            match entry.state {
                TaskLifecycleState::Terminal(TaskTerminalKind::AnalysisCompleted) => {
                    analysis_completed += 1;
                }
                TaskLifecycleState::Terminal(TaskTerminalKind::InfrastructureFailed) => {
                    infrastructure_failed += 1;
                }
                TaskLifecycleState::Terminal(TaskTerminalKind::NotStarted) => {
                    not_started += 1;
                }
                TaskLifecycleState::Terminal(TaskTerminalKind::Cancelled) => {
                    cancelled += 1;
                }
                other => {
                    panic!(
                        "batch task ledger entry {:?} still non-terminal during report assembly: {:?}",
                        entry.task_id, other
                    );
                }
            }
        }

        let accounting = BatchLedgerSummary {
            accepted: self.entries.len(),
            analysis_completed,
            infrastructure_failed,
            not_started,
            cancelled,
            synthesized_rows: self.synthesized_rows,
            duplicate_completions: self.duplicate_completions,
            stale_completions: self.stale_completions,
            spawn_errors: self.spawn_errors,
            send_errors: self.send_errors,
            join_errors: self.join_errors,
            consumer_errors: self.consumer_errors,
            sequential_fallback: self.sequential_fallback,
        };
        assert!(
            accounting.reconciles(),
            "batch ledger accounting failed to reconcile: {accounting:?}"
        );

        // Presentation order must depend solely on canonical task metadata
        // (`(address, original_task_id)`), never worker arrival order or LPT
        // execution order, including when aliases share an address.
        self.entries.sort_by(|a, b| {
            a.func
                .address
                .cmp(&b.func.address)
                .then_with(|| a.task_id.cmp(&b.task_id))
        });

        let results: Vec<FunctionRenderResult> = self
            .entries
            .into_iter()
            .map(|entry| {
                entry
                    .result
                    .expect("terminal ledger entry must hold a result")
            })
            .collect();
        assert_eq!(results.len(), accounting.accepted);

        BatchFanoutReport {
            results,
            accounting,
        }
    }
}

struct PersistentDecompWorker {
    request_tx: mpsc::SyncSender<(BatchTaskId, FunctionInfo)>,
    result_rx: mpsc::Receiver<(BatchTaskId, FunctionRenderResult)>,
    handle: Option<thread::JoinHandle<()>>,
}

impl PersistentDecompWorker {
    fn spawn(
        worker_idx: usize,
        binary: Arc<LoadedBinary>,
        facts: Arc<FactStore>,
        config: RenderConfig,
        stack_size_bytes: usize,
    ) -> Result<Self, std::io::Error> {
        let (request_tx, request_rx) = mpsc::sync_channel::<(BatchTaskId, FunctionInfo)>(0);
        let (result_tx, result_rx) = mpsc::sync_channel::<(BatchTaskId, FunctionRenderResult)>(1);
        let handle = thread::Builder::new()
            .name(format!("fission-rust-decomp-worker-{worker_idx}"))
            .stack_size(stack_size_bytes)
            .spawn(move || {
                while let Ok((task_id, func)) = request_rx.recv() {
                    let mut result = render_one_function_catching_panic(
                        binary.as_ref(),
                        facts.as_ref(),
                        &func,
                        config,
                    );
                    result.task_id = Some(task_id);
                    if result_tx.send((task_id, result)).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            request_tx,
            result_rx,
            handle: Some(handle),
        })
    }

    fn shutdown(mut self) {
        drop(self.request_tx);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

struct WorkerSupervisorContext {
    worker_idx: usize,
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
        loop {
            if ctx.cancel.load(Ordering::Acquire) || ctx.stop_dispatch.load(Ordering::Acquire) {
                break;
            }
            let task = match ctx.task_rx.lock() {
                Ok(locked) => locked.recv(),
                Err(_) => break,
            };
            let task = match task {
                Ok(task) => task,
                Err(_) => break,
            };
            if ctx.cancel.load(Ordering::Acquire) {
                ctx.progress.mark_cancelled_by_worker(task.task_id);
                break;
            }
            if ctx.stop_dispatch.load(Ordering::Acquire) {
                break;
            }
            ctx.progress.mark_running(task.task_id);

            let task_id = task.task_id;
            let func = task.func;
            let mut rendered = if let Some(timeout_ms) = ctx.timeout_ms {
                // Reuse one `stack_size_bytes` worker thread across tasks
                // so we keep thread-local SLEIGH/type caches warm and
                // avoid per-function 32 MiB OS thread creation/teardown,
                // while still detaching and replacing a wedged worker on
                // hard timeout or unexpected disconnect.
                if persistent_child.is_none() {
                    match PersistentDecompWorker::spawn(
                        ctx.worker_idx,
                        Arc::clone(&ctx.binary),
                        Arc::clone(&ctx.facts),
                        ctx.config,
                        ctx.stack_size_bytes,
                    ) {
                        Ok(worker) => persistent_child = Some(worker),
                        Err(err) => {
                            let mut err_res = make_internal_error_result(
                                ctx.binary.as_ref(),
                                &func,
                                format!("failed to spawn render worker: {err}"),
                                ctx.config,
                            );
                            err_res.task_id = Some(task_id);
                            if ctx
                                .result_tx
                                .send(WorkerCompletion {
                                    task_id,
                                    result: err_res,
                                })
                                .is_err()
                            {
                                break;
                            }
                            continue;
                        }
                    }
                }
                let worker = persistent_child
                    .as_mut()
                    .expect("persistent worker initialized");
                if worker.request_tx.send((task_id, func.clone())).is_err() {
                    if let Some(dead) = persistent_child.take() {
                        dead.shutdown();
                    }
                    make_internal_error_result(
                        ctx.binary.as_ref(),
                        &func,
                        "worker thread exited before receiving a render task".to_string(),
                        ctx.config,
                    )
                } else {
                    match worker
                        .result_rx
                        .recv_timeout(Duration::from_millis(timeout_ms))
                    {
                        Ok((returned_id, result)) => {
                            if returned_id != task_id || result.address != func.address {
                                if let Some(mut wedged) = persistent_child.take() {
                                    wedged.handle.take();
                                }
                                make_internal_error_result(
                                    ctx.binary.as_ref(),
                                    &func,
                                    format!(
                                        "worker child returned mismatched task correlation ({returned_id:?} vs expected {task_id:?})"
                                    ),
                                    ctx.config,
                                )
                            } else {
                                result
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            // Detach without joining; the next task spawns a
                            // fresh large-stack child thread.
                            if let Some(mut wedged) = persistent_child.take() {
                                wedged.handle.take();
                            }
                            make_internal_error_result(
                                ctx.binary.as_ref(),
                                &func,
                                render_timeout_message(timeout_ms),
                                ctx.config,
                            )
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            if let Some(dead) = persistent_child.take() {
                                dead.shutdown();
                            }
                            make_internal_error_result(
                                ctx.binary.as_ref(),
                                &func,
                                "worker thread exited before returning a render result".to_string(),
                                ctx.config,
                            )
                        }
                    }
                }
            } else {
                render_one_function_catching_panic(
                    ctx.binary.as_ref(),
                    ctx.facts.as_ref(),
                    &func,
                    ctx.config,
                )
            };
            rendered.task_id = Some(task_id);
            if ctx
                .result_tx
                .send(WorkerCompletion {
                    task_id,
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

pub(crate) fn run_sequential_batch(
    binary: Arc<LoadedBinary>,
    facts: Arc<FactStore>,
    functions: &[FunctionInfo],
    config: RenderConfig,
    stack_size_bytes: usize,
) -> BatchFanoutReport {
    let mut ledger = BatchTaskLedger::new(functions);
    let progress = BatchProgressTracker::new(functions.len());

    for (idx, func) in functions.iter().enumerate() {
        let task_id = BatchTaskId(idx);
        progress.mark_running(task_id);
        ledger.mark_running(task_id);
        let mut rendered = render_one_function_on_large_stack(
            Arc::clone(&binary),
            Arc::clone(&facts),
            func,
            config,
            stack_size_bytes,
        );
        rendered.task_id = Some(task_id);
        let _ = ledger.record_completion(
            WorkerCompletion {
                task_id,
                result: rendered,
            },
            &mut |_, _| Ok(()),
            binary.as_ref(),
            config,
        );
    }

    ledger.reconcile_missing(&progress, false, binary.as_ref(), config);
    ledger.into_report()
}

pub(crate) fn run_worker_fanout_fanin(
    binary: Arc<LoadedBinary>,
    facts: Arc<FactStore>,
    functions: &[FunctionInfo],
    config: RenderConfig,
    worker_count: usize,
    stack_size_bytes: usize,
) -> BatchFanoutReport {
    run_worker_fanout_fanin_with_hooks(
        binary,
        facts,
        functions,
        config,
        worker_count,
        stack_size_bytes,
        spawn_production_supervisor,
        Arc::new(AtomicBool::new(false)),
        |_, _| Ok(()),
    )
}

fn lpt_schedule(functions: &[FunctionInfo]) -> Vec<ScheduledTask> {
    let mut indices: Vec<usize> = (0..functions.len()).collect();
    // Schedule largest functions first (LPT) to avoid long-tail stragglers;
    // `BatchTaskLedger::into_report` sorts results by `(address, task_id)`.
    indices.sort_by(|&a, &b| {
        functions[b]
            .size
            .cmp(&functions[a].size)
            .then_with(|| functions[a].address.cmp(&functions[b].address))
            .then_with(|| a.cmp(&b))
    });
    indices
        .into_iter()
        .map(|idx| ScheduledTask {
            task_id: BatchTaskId(idx),
            func: functions[idx].clone(),
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn run_worker_fanout_fanin_with_hooks<S, C>(
    binary: Arc<LoadedBinary>,
    facts: Arc<FactStore>,
    functions: &[FunctionInfo],
    config: RenderConfig,
    worker_count: usize,
    stack_size_bytes: usize,
    mut spawn_supervisor: S,
    cancel: Arc<AtomicBool>,
    mut on_result: C,
) -> BatchFanoutReport
where
    S: FnMut(WorkerSupervisorContext) -> Result<thread::JoinHandle<()>, std::io::Error>,
    C: FnMut(BatchTaskId, &FunctionRenderResult) -> Result<(), String>,
{
    let mut ledger = BatchTaskLedger::new(functions);
    let progress = Arc::new(BatchProgressTracker::new(functions.len()));
    let stop_dispatch = Arc::new(AtomicBool::new(false));

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
        for task in lpt_schedule(functions) {
            if cancel.load(Ordering::Acquire) || stop_dispatch.load(Ordering::Acquire) {
                break;
            }
            progress.mark_running(task.task_id);
            ledger.mark_running(task.task_id);
            let mut rendered = render_one_function_catching_panic(
                binary.as_ref(),
                facts.as_ref(),
                &task.func,
                config,
            );
            rendered.task_id = Some(task.task_id);
            if ledger
                .record_completion(
                    WorkerCompletion {
                        task_id: task.task_id,
                        result: rendered,
                    },
                    &mut on_result,
                    binary.as_ref(),
                    config,
                )
                .is_err()
            {
                stop_dispatch.store(true, Ordering::Release);
                break;
            }
        }
        ledger.reconcile_missing(
            &progress,
            cancel.load(Ordering::Acquire),
            binary.as_ref(),
            config,
        );
        return ledger.into_report();
    }

    for task in lpt_schedule(functions) {
        if cancel.load(Ordering::Acquire) {
            break;
        }
        let task_id = task.task_id;
        progress.mark_queued(task_id);
        if task_tx.send(task).is_err() {
            progress.mark_send_failed(task_id);
            ledger.send_errors += 1;
            break;
        }
        ledger.mark_queued(task_id);
    }
    drop(task_tx);

    // Drain until all worker `Sender`s close rather than looping `0..functions.len()`:
    // counting raw messages would let duplicate or stale completions finish the
    // loop early and drop legitimate late task results.
    while let Ok(completion) = result_rx.recv() {
        if ledger
            .record_completion(completion, &mut on_result, binary.as_ref(), config)
            .is_err()
        {
            stop_dispatch.store(true, Ordering::Release);
            break;
        }
    }
    drop(result_rx);

    for handle in worker_handles {
        if handle.join().is_err() {
            ledger.join_errors += 1;
        }
    }

    ledger.reconcile_missing(
        &progress,
        cancel.load(Ordering::Acquire),
        binary.as_ref(),
        config,
    );
    ledger.into_report()
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
            FunctionRenderResult, record_into_function_render_result,
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
                            ctx.progress.mark_running(task.task_id);
                            let mut rendered =
                                fake_success(&task.func, &format!("// {}", task.func.name));
                            rendered.task_id = Some(task.task_id);
                            buffered.push(WorkerCompletion {
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
            assert_eq!(report.results[0].task_id, Some(BatchTaskId(1)));
            assert_eq!(report.results[0].plain_output, "// alias_first");
            assert_eq!(report.results[1].task_id, Some(BatchTaskId(2)));
            assert_eq!(report.results[1].plain_output, "// alias_second");
            assert_eq!(report.results[2].task_id, Some(BatchTaskId(0)));
            assert_eq!(report.results[2].plain_output, "// fn_other");
            assert!(report.accounting.reconciles());
            assert_eq!(report.accounting.accepted, 3);
            assert_eq!(report.accounting.analysis_completed, 3);
            assert!(!report.accounting.has_infrastructure_anomalies());
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
                            ctx.progress.mark_running(task.task_id);
                            rendered_counts.lock().unwrap()[task.task_id.0] += 1;
                            if ctx.worker_idx == 0 {
                                gate.store(true, Ordering::Release);
                                panic!("simulated supervisor crash after dequeue");
                            }
                            let mut rendered =
                                fake_success(&task.func, &format!("// {}", task.func.name));
                            rendered.task_id = Some(task.task_id);
                            let _ = ctx.result_tx.send(WorkerCompletion {
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
            assert_eq!(panicked_row.task_id, Some(BatchTaskId(0)));
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
                            ctx.progress.mark_running(task.task_id);
                            let mut rendered = fake_success(&task.func, "ok");
                            rendered.task_id = Some(task.task_id);
                            let _ = ctx.result_tx.send(WorkerCompletion {
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
                            ctx.progress.mark_running(task.task_id);
                            let mut rendered = fake_success(&task.func, "first");
                            rendered.task_id = Some(task.task_id);
                            let _ = ctx.result_tx.send(WorkerCompletion {
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
                            ctx.progress.mark_running(task.task_id);
                            if !injected {
                                injected = true;
                                // Stale completion #1: out-of-range TaskId.
                                let _ = ctx.result_tx.send(WorkerCompletion {
                                    task_id: BatchTaskId(99),
                                    result: fake_success(&task.func, "stale_id"),
                                });
                                // Stale completion #2: wrong address for target TaskId.
                                let wrong_addr_func = make_func("wrong", 0x9999, 4);
                                let _ = ctx.result_tx.send(WorkerCompletion {
                                    task_id: task.task_id,
                                    result: fake_success(&wrong_addr_func, "stale_addr"),
                                });
                            }

                            let valid =
                                fake_success(&task.func, &format!("first:{}", task.func.name));
                            let _ = ctx.result_tx.send(WorkerCompletion {
                                task_id: task.task_id,
                                result: valid,
                            });
                            // Duplicate completion with mutated payload must be ignored.
                            let dup = fake_success(&task.func, &format!("dup:{}", task.func.name));
                            let _ = ctx.result_tx.send(WorkerCompletion {
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
            assert_eq!(report.accounting.stale_completions, 2);
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
                            ctx.progress.mark_running(task.task_id);
                            let mut rendered = fake_success(&task.func, "done");
                            rendered.task_id = Some(task.task_id);
                            let _ = ctx.result_tx.send(WorkerCompletion {
                                task_id: task.task_id,
                                result: rendered,
                            });
                            cancel_ref.store(true, Ordering::Release);
                        }
                        while let Ok(task) = ctx.task_rx.lock().unwrap().recv() {
                            if ctx.cancel.load(Ordering::Acquire) {
                                ctx.progress.mark_cancelled_by_worker(task.task_id);
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
                            ctx.progress.mark_running(task.task_id);
                            let mut rendered = fake_success(&task.func, "rendered");
                            rendered.task_id = Some(task.task_id);
                            if ctx
                                .result_tx
                                .send(WorkerCompletion {
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
                            ctx.progress.mark_running(task.task_id);
                            if task.task_id == BatchTaskId(0) {
                                let mut rendered = fake_hard_error(&task.func, "bad pcode");
                                rendered.task_id = Some(task.task_id);
                                let _ = ctx.result_tx.send(WorkerCompletion {
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
