use crate::cli::oneshot::rust_decomp::record::{
    AbortedRenderKind, InfrastructureFailureKind, RenderConfig,
};
use crate::cli::oneshot::rust_decomp::{
    FunctionRenderResult, RenderOutcomeKind, make_aborted_render_result,
    make_infrastructure_error_result, make_internal_error_result, render_one_function_inner,
};
use fission_loader::loader::{FunctionInfo, LoadedBinary};
use fission_static::analysis::decomp::facts::FactStore;
use std::cmp::min;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
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
    let binary_for_render = Arc::clone(&binary);
    render_on_large_stack_with(binary, func, config, stack_size_bytes, move || {
        render_one_function_inner(
            binary_for_render.as_ref(),
            facts.as_ref(),
            &func_owned,
            config,
        )
    })
}

/// Run `render` for `func` on a dedicated large-stack thread, bounded by the
/// configured render timeout. A panic inside `render`, or `render` running
/// past the timeout, is an *analysis* outcome for this function; failing to
/// spawn or lose the thread is an infrastructure one.
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
    let func_for_thread = func.clone();
    let binary_for_thread = Arc::clone(&binary);
    let (result_tx, result_rx) = mpsc::sync_channel::<FunctionRenderResult>(1);

    let spawn = thread::Builder::new()
        .name(format!("fission-rust-decomp-0x{:x}", func.address))
        .stack_size(stack_size_bytes)
        .spawn(move || {
            let result =
                render_catching_panic(binary_for_thread.as_ref(), &func_for_thread, config, render);
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
                    Err(mpsc::RecvTimeoutError::Timeout) => make_aborted_render_result(
                        binary.as_ref(),
                        func,
                        AbortedRenderKind::AnalysisTimedOut,
                        render_timeout_message(timeout_ms),
                        config,
                    ),
                    Err(mpsc::RecvTimeoutError::Disconnected) => {
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
                match handle.join() {
                    Ok(()) => result_rx.recv().unwrap_or_else(|_| {
                        make_internal_error_result(
                            binary.as_ref(),
                            func,
                            "worker thread exited before returning a render result".to_string(),
                            config,
                        )
                    }),
                    // `render` itself runs under `catch_unwind`; a join error
                    // is a panic in the thread's own plumbing.
                    Err(_) => make_internal_error_result(
                        binary.as_ref(),
                        func,
                        "render worker thread panicked outside the render".to_string(),
                        config,
                    ),
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

struct PersistentDecompWorker {
    request_tx: mpsc::SyncSender<ScheduledTask>,
    result_rx: mpsc::Receiver<FunctionRenderResult>,
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
        let (request_tx, request_rx) = mpsc::sync_channel::<ScheduledTask>(0);
        let (result_tx, result_rx) = mpsc::sync_channel::<FunctionRenderResult>(1);
        let handle = thread::Builder::new()
            .name(format!("fission-rust-decomp-worker-{worker_idx}"))
            .stack_size(stack_size_bytes)
            .spawn(move || {
                while let Ok(task) = request_rx.recv() {
                    let result = task.render(|func| {
                        render_one_function_catching_panic(
                            binary.as_ref(),
                            facts.as_ref(),
                            func,
                            config,
                        )
                    });
                    if result_tx.send(result).is_err() {
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

    /// Abandon a child that may still be running; it exits on its own once
    /// its next send finds the result channel closed.
    fn detach(mut self) {
        self.handle.take();
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
}

/// Render `task` on the supervisor's persistent large-stack child, bounded
/// by `timeout_ms`; the child is replaced after a timeout or a failure.
fn render_on_persistent_child(
    ctx: &WorkerSupervisorContext,
    persistent_child: &mut Option<PersistentDecompWorker>,
    task: &ScheduledTask,
    timeout_ms: u64,
) -> FunctionRenderResult {
    let infrastructure = |message: String| {
        task.render(|func| make_internal_error_result(&ctx.binary, func, message, ctx.config))
    };
    if persistent_child.is_none() {
        // Reuse one `stack_size_bytes` thread across tasks so thread-local
        // SLEIGH/type caches stay warm and we avoid per-function 32 MiB
        // thread creation, while still replacing a wedged child.
        match PersistentDecompWorker::spawn(
            ctx.worker_idx,
            Arc::clone(&ctx.binary),
            Arc::clone(&ctx.facts),
            ctx.config,
            ctx.stack_size_bytes,
        ) {
            Ok(worker) => *persistent_child = Some(worker),
            Err(err) => return infrastructure(format!("failed to spawn render worker: {err}")),
        }
    }
    let worker = persistent_child
        .as_mut()
        .expect("persistent worker initialized");
    if worker.request_tx.send(task.clone()).is_err() {
        if let Some(dead) = persistent_child.take() {
            dead.shutdown();
        }
        return infrastructure("worker thread exited before receiving a render task".to_string());
    }
    match worker
        .result_rx
        .recv_timeout(Duration::from_millis(timeout_ms))
    {
        Ok(result) if task.owns(&result) => result,
        Ok(_) => {
            if let Some(child) = persistent_child.take() {
                child.detach();
            }
            infrastructure(format!(
                "worker child returned a result not rendered for {:?}",
                task.task_id
            ))
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            // Detach without joining; the next task spawns a fresh child.
            if let Some(child) = persistent_child.take() {
                child.detach();
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
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            if let Some(dead) = persistent_child.take() {
                dead.shutdown();
            }
            infrastructure("worker thread exited before returning a render result".to_string())
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
        loop {
            if ctx.cancel.load(Ordering::Acquire) || ctx.stop_dispatch.load(Ordering::Acquire) {
                break;
            }
            let task = match ctx.task_rx.lock() {
                Ok(locked) => locked.recv(),
                Err(_) => break,
            };
            let Ok(task) = task else { break };
            if ctx.cancel.load(Ordering::Acquire) {
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
                task.render(|func| {
                    render_one_function_catching_panic(
                        ctx.binary.as_ref(),
                        ctx.facts.as_ref(),
                        func,
                        ctx.config,
                    )
                })
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
    let mut ledger = BatchTaskLedger::new(functions);
    let progress = BatchProgressTracker::new(functions.len());

    for idx in 0..functions.len() {
        let task = ledger.task(BatchTaskId(idx));
        let _ = run_task_on_coordinator(
            &mut ledger,
            &progress,
            &task,
            |func| {
                render_one_function_on_large_stack(
                    Arc::clone(&binary),
                    Arc::clone(&facts),
                    func,
                    config,
                    stack_size_bytes,
                )
            },
            &mut |_, _| Ok(()),
            binary.as_ref(),
            config,
        );
    }

    ledger.reconcile_missing(&progress, false, binary.as_ref(), config);
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
        for task in ledger.lpt_schedule() {
            if cancel.load(Ordering::Acquire) || stop_dispatch.load(Ordering::Acquire) {
                break;
            }
            if run_task_on_coordinator(
                &mut ledger,
                &progress,
                &task,
                |func| {
                    render_one_function_catching_panic(
                        binary.as_ref(),
                        facts.as_ref(),
                        func,
                        config,
                    )
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
        return ledger.into_report(&progress);
    }

    for task in ledger.lpt_schedule() {
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
