use crate::cli::args::OneShotArgs;
use crate::cli::oneshot::function_select::BatchSelectionAccounting;
use serde_json::json;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProcessCpuSnapshot {
    pub user_sec: f64,
    pub system_sec: f64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProcessCpuDelta {
    pub user_sec: f64,
    pub system_sec: f64,
    pub total_sec: f64,
    pub utilization_pct: f64,
    pub effective_parallelism: f64,
}

pub(crate) fn round_six(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

pub(crate) fn round_three(value: f64) -> f64 {
    (value * 1_000.0).round() / 1_000.0
}

pub(crate) fn process_cpu_delta(
    start: ProcessCpuSnapshot,
    end: ProcessCpuSnapshot,
    wall_clock_sec: f64,
) -> ProcessCpuDelta {
    let user_sec = (end.user_sec - start.user_sec).max(0.0);
    let system_sec = (end.system_sec - start.system_sec).max(0.0);
    let total_sec = user_sec + system_sec;
    let wall = wall_clock_sec.max(1e-9);
    ProcessCpuDelta {
        user_sec,
        system_sec,
        total_sec,
        utilization_pct: (total_sec / wall) * 100.0,
        effective_parallelism: total_sec / wall,
    }
}

#[cfg(unix)]
pub(crate) fn capture_process_cpu_snapshot() -> Option<ProcessCpuSnapshot> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if rc != 0 {
        return None;
    }
    let usage = unsafe { usage.assume_init() };
    Some(ProcessCpuSnapshot {
        user_sec: timeval_to_seconds(usage.ru_utime),
        system_sec: timeval_to_seconds(usage.ru_stime),
    })
}

#[cfg(unix)]
fn timeval_to_seconds(value: libc::timeval) -> f64 {
    value.tv_sec as f64 + (value.tv_usec as f64 / 1_000_000.0)
}

#[cfg(not(unix))]
pub(crate) fn capture_process_cpu_snapshot() -> Option<ProcessCpuSnapshot> {
    None
}

pub(crate) fn benchmark_envelope_json(
    cli: &OneShotArgs,
    json_results: Vec<serde_json::Value>,
    result_len: usize,
    worker_count: usize,
    use_worker_fanout: bool,
    available_parallelism: usize,
    worker_env_requested: Option<String>,
    stack_size_bytes: usize,
    selection_accounting: &BatchSelectionAccounting,
    total_decomp_secs: f64,
    total_postprocess_secs: f64,
    wall_clock_sec: f64,
    cpu_start: Option<ProcessCpuSnapshot>,
) -> serde_json::Value {
    let cpu_delta = cpu_start.and_then(|start| {
        capture_process_cpu_snapshot().map(|end| process_cpu_delta(start, end, wall_clock_sec))
    });
    json!({
        "_meta": {
            "tool": "fission",
            "version": env!("CARGO_PKG_VERSION"),
            "profile": cli.profile.as_deref().unwrap_or("balanced"),
            "engine": "rust-sleigh",
            "function_count": result_len,
            "worker_count": worker_count,
            "worker_fanout_enabled": use_worker_fanout,
            "available_parallelism": available_parallelism,
            "worker_env_requested": worker_env_requested,
            "decomp_stack_mb": stack_size_bytes / (1024 * 1024),
            "functions_discovered_total": selection_accounting.functions_discovered_total,
            "functions_selected_total": selection_accounting.functions_selected_total,
            "functions_excluded_import_count": selection_accounting.functions_excluded_import_count,
            "functions_excluded_runtime_wrapper_count": selection_accounting.functions_excluded_runtime_wrapper_count,
            "functions_excluded_provenance_count": selection_accounting.functions_excluded_provenance_count,
            "include_nonuser_functions": selection_accounting.include_nonuser_functions,
            "init_sec": 0.0,
            "total_decomp_sec": round_six(total_decomp_secs),
            "total_postprocess_sec": round_six(total_postprocess_secs),
            "wall_clock_sec": wall_clock_sec,
            "cpu_user_sec": cpu_delta.map(|delta| round_six(delta.user_sec)),
            "cpu_system_sec": cpu_delta.map(|delta| round_six(delta.system_sec)),
            "cpu_total_sec": cpu_delta.map(|delta| round_six(delta.total_sec)),
            "cpu_utilization_pct": cpu_delta.map(|delta| round_three(delta.utilization_pct)),
            "effective_parallelism": cpu_delta.map(|delta| round_three(delta.effective_parallelism)),
        },
        "functions": json_results,
    })
}
