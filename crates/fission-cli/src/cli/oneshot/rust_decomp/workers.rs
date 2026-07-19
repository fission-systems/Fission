use crate::cli::oneshot::rust_decomp::record::RenderConfig;
use crate::cli::oneshot::rust_decomp::{
    FunctionRenderResult, make_internal_error_result, render_one_function_inner,
};
use fission_loader::loader::FunctionInfo;
use std::cmp::min;
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

    let cpu = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    min(total_functions, cpu.clamp(1, 8))
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

pub(crate) fn render_one_function_on_large_stack(
    binary: Arc<fission_loader::loader::LoadedBinary>,
    func: &FunctionInfo,
    config: RenderConfig,
    stack_size_bytes: usize,
) -> FunctionRenderResult {
    let func_owned = func.clone();
    let func_for_error = func.clone();
    let binary_for_thread = Arc::clone(&binary);
    let (result_tx, result_rx) = mpsc::sync_channel::<FunctionRenderResult>(1);

    let spawn = thread::Builder::new()
        .name(format!("fission-rust-decomp-0x{:x}", func.address))
        .stack_size(stack_size_bytes)
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                render_one_function_inner(binary_for_thread.as_ref(), &func_owned, config)
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

pub(crate) fn run_worker_fanout_fanin(
    binary: Arc<fission_loader::loader::LoadedBinary>,
    functions: &[FunctionInfo],
    config: RenderConfig,
    worker_count: usize,
    stack_size_bytes: usize,
) -> Vec<FunctionRenderResult> {
    let (task_tx, task_rx) = mpsc::channel::<FunctionInfo>();
    let task_rx = Arc::new(Mutex::new(task_rx));
    let (result_tx, result_rx) = mpsc::channel::<FunctionRenderResult>();

    let mut worker_handles = Vec::with_capacity(worker_count);
    for worker_idx in 0..worker_count {
        let rx = Arc::clone(&task_rx);
        let tx = result_tx.clone();
        let binary = Arc::clone(&binary);
        let spawn = thread::Builder::new()
            .name(format!("fission-rust-decomp-worker-{worker_idx}"))
            .stack_size(stack_size_bytes)
            .spawn(move || {
                loop {
                    let task = match rx.lock() {
                        Ok(locked) => locked.recv(),
                        Err(_) => return,
                    };
                    let func = match task {
                        Ok(func) => func,
                        Err(_) => return,
                    };
                    // Per-task timeout guard (same mechanism as the
                    // single-function path): without this, a function whose
                    // structuring never hits a budget checkpoint can wedge
                    // this persistent worker forever, silently shrinking the
                    // pool by one and -- if worker_count is 1, as it is for
                    // small function counts -- hanging the whole batch. The
                    // stuck computation's own thread is still abandoned in
                    // the background (Rust has no thread cancellation), but
                    // the queue keeps moving.
                    let rendered = render_one_function_on_large_stack(
                        Arc::clone(&binary),
                        &func,
                        config,
                        stack_size_bytes,
                    );
                    if tx.send(rendered).is_err() {
                        return;
                    }
                }
            });

        if let Ok(handle) = spawn {
            worker_handles.push(handle);
        }
    }
    drop(result_tx);

    for func in functions {
        if task_tx.send(func.clone()).is_err() {
            break;
        }
    }
    drop(task_tx);

    let mut outputs = Vec::with_capacity(functions.len());
    for _ in 0..functions.len() {
        if let Ok(output) = result_rx.recv() {
            outputs.push(output);
        } else {
            break;
        }
    }

    for handle in worker_handles {
        let _ = handle.join();
    }

    outputs
}

#[cfg(test)]
mod tests {
    use super::render_timeout_message;

    #[test]
    fn render_timeout_message_includes_exact_budget() {
        assert_eq!(
            render_timeout_message(37),
            "preview_timeout: Rust-Sleigh render timed out after 37ms"
        );
    }
}
