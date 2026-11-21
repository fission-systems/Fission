use crate::cli::oneshot::rust_decomp::record::RenderConfig;
use crate::cli::oneshot::rust_decomp::{
    FunctionRenderResult, make_internal_error_result, render_one_function_inner,
};
use fission_loader::loader::FunctionInfo;
use fission_static::analysis::decomp::facts::FactStore;
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

struct PersistentDecompWorker {
    request_tx: mpsc::SyncSender<FunctionInfo>,
    result_rx: mpsc::Receiver<FunctionRenderResult>,
    handle: Option<thread::JoinHandle<()>>,
}

impl PersistentDecompWorker {
    fn spawn(
        worker_idx: usize,
        binary: Arc<fission_loader::loader::LoadedBinary>,
        facts: Arc<FactStore>,
        config: RenderConfig,
        stack_size_bytes: usize,
    ) -> Result<Self, std::io::Error> {
        let (request_tx, request_rx) = mpsc::sync_channel::<FunctionInfo>(0);
        let (result_tx, result_rx) = mpsc::sync_channel::<FunctionRenderResult>(1);
        let handle = thread::Builder::new()
            .name(format!("fission-rust-decomp-worker-{worker_idx}"))
            .stack_size(stack_size_bytes)
            .spawn(move || {
                while let Ok(func) = request_rx.recv() {
                    let func_for_panic = func.clone();
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        render_one_function_inner(binary.as_ref(), facts.as_ref(), &func, config)
                    }))
                    .unwrap_or_else(|_| {
                        make_internal_error_result(
                            binary.as_ref(),
                            &func_for_panic,
                            "worker thread panicked while rendering function".to_string(),
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
}

pub(crate) fn run_worker_fanout_fanin(
    binary: Arc<fission_loader::loader::LoadedBinary>,
    facts: Arc<FactStore>,
    functions: &[FunctionInfo],
    config: RenderConfig,
    worker_count: usize,
    stack_size_bytes: usize,
) -> Vec<FunctionRenderResult> {
    let (task_tx, task_rx) = mpsc::channel::<FunctionInfo>();
    let task_rx = Arc::new(Mutex::new(task_rx));
    let (result_tx, result_rx) = mpsc::channel::<FunctionRenderResult>();
    let timeout = config.timeout_ms.filter(|ms| *ms > 0);

    let mut worker_handles = Vec::with_capacity(worker_count);
    for worker_idx in 0..worker_count {
        let rx = Arc::clone(&task_rx);
        let tx = result_tx.clone();
        let binary = Arc::clone(&binary);
        let facts = Arc::clone(&facts);
        let mut builder =
            thread::Builder::new().name(format!("fission-rust-decomp-supervisor-{worker_idx}"));
        if timeout.is_none() {
            builder = builder.stack_size(stack_size_bytes);
        }
        let spawn = builder.spawn(move || {
            let mut persistent_child: Option<PersistentDecompWorker> = None;
            loop {
                let task = match rx.lock() {
                    Ok(locked) => locked.recv(),
                    Err(_) => break,
                };
                let func = match task {
                    Ok(func) => func,
                    Err(_) => break,
                };
                let rendered = if let Some(timeout_ms) = timeout {
                    // Reuse one `stack_size_bytes` worker thread across tasks
                    // so we keep thread-local SLEIGH/type caches warm and
                    // avoid per-function 32 MiB OS thread creation/teardown,
                    // while still detaching and replacing a wedged worker on
                    // hard timeout or unexpected disconnect.
                    if persistent_child.is_none() {
                        match PersistentDecompWorker::spawn(
                            worker_idx,
                            Arc::clone(&binary),
                            Arc::clone(&facts),
                            config,
                            stack_size_bytes,
                        ) {
                            Ok(worker) => persistent_child = Some(worker),
                            Err(err) => {
                                if tx
                                    .send(make_internal_error_result(
                                        binary.as_ref(),
                                        &func,
                                        format!("failed to spawn render worker: {err}"),
                                        config,
                                    ))
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
                    if worker.request_tx.send(func.clone()).is_err() {
                        if let Some(dead) = persistent_child.take() {
                            dead.shutdown();
                        }
                        make_internal_error_result(
                            binary.as_ref(),
                            &func,
                            "worker thread exited before receiving a render task".to_string(),
                            config,
                        )
                    } else {
                        match worker
                            .result_rx
                            .recv_timeout(Duration::from_millis(timeout_ms))
                        {
                            Ok(result) => result,
                            Err(mpsc::RecvTimeoutError::Timeout) => {
                                // Detach without joining; the next task spawns a
                                // fresh large-stack child thread.
                                if let Some(mut wedged) = persistent_child.take() {
                                    wedged.handle.take();
                                }
                                make_internal_error_result(
                                    binary.as_ref(),
                                    &func,
                                    render_timeout_message(timeout_ms),
                                    config,
                                )
                            }
                            Err(mpsc::RecvTimeoutError::Disconnected) => {
                                if let Some(dead) = persistent_child.take() {
                                    dead.shutdown();
                                }
                                make_internal_error_result(
                                    binary.as_ref(),
                                    &func,
                                    "worker thread exited before returning a render result"
                                        .to_string(),
                                    config,
                                )
                            }
                        }
                    }
                } else {
                    let func_owned = func.clone();
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        render_one_function_inner(
                            binary.as_ref(),
                            facts.as_ref(),
                            &func_owned,
                            config,
                        )
                    }))
                    .unwrap_or_else(|_| {
                        make_internal_error_result(
                            binary.as_ref(),
                            &func_owned,
                            "worker thread panicked while rendering function".to_string(),
                            config,
                        )
                    })
                };
                if tx.send(rendered).is_err() {
                    break;
                }
            }
            if let Some(worker) = persistent_child {
                worker.shutdown();
            }
        });

        if let Ok(handle) = spawn {
            worker_handles.push(handle);
        }
    }
    drop(result_tx);

    // Schedule largest functions first (LPT) to avoid long-tail stragglers;
    // `rust_decomp::mod` re-sorts outputs by entry address before formatting.
    let mut scheduled = functions.to_vec();
    scheduled.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.address.cmp(&b.address)));
    for func in scheduled {
        if task_tx.send(func).is_err() {
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
    use super::{render_timeout_message, resolve_render_timeout_ms};

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
}
