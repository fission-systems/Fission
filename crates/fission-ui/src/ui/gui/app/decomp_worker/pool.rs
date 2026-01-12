#![cfg(feature = "native_decomp")]

use crate::ui::gui::core::messages::AsyncMessage;
use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::AtomicU64,
};

use super::worker::binary_worker_loop;
use super::WorkerRequest;

/// Worker handle for a specific binary
struct BinaryWorker {
    /// Channel to send requests to this worker
    tx: Sender<WorkerRequest>,
}

/// Pool of per-binary decompiler workers
struct DecompilerPool {
    workers: HashMap<String, BinaryWorker>,
    result_tx: Sender<AsyncMessage>,
    latest_request_id: Arc<AtomicU64>,
}

impl DecompilerPool {
    fn new(result_tx: Sender<AsyncMessage>, latest_request_id: Arc<AtomicU64>) -> Self {
        Self {
            workers: HashMap::new(),
            result_tx,
            latest_request_id,
        }
    }

    fn get_or_create_worker(&mut self, binary_id: &str) -> &BinaryWorker {
        if !self.workers.contains_key(binary_id) {
            let (tx, rx) = crossbeam_channel::unbounded();
            let result_tx = self.result_tx.clone();
            let latest_request_id = Arc::clone(&self.latest_request_id);
            let binary_id_clone = binary_id.to_string();

            // Spawn a dedicated worker thread for this binary
            std::thread::Builder::new()
                .name(format!(
                    "decomp-worker-{}",
                    &binary_id[..8.min(binary_id.len())]
                ))
                .spawn(move || {
                    binary_worker_loop(binary_id_clone, rx, result_tx, latest_request_id);
                })
                .expect("Failed to spawn binary worker thread");

            self.workers
                .insert(binary_id.to_string(), BinaryWorker { tx });
            crate::core::logging::info(&format!(
                "[decomp-pool] Spawned new worker for binary: {}...",
                &binary_id[..16.min(binary_id.len())]
            ));
        }

        self.workers.get(binary_id).unwrap()
    }

    fn dispatch(&mut self, request: WorkerRequest) {
        let binary_id = match request.binary_id() {
            "" => "default".to_string(),
            id => id.to_string(),
        };

        let worker = self.get_or_create_worker(&binary_id);
        let _ = worker.tx.send(request);
    }
}

pub fn spawn_worker(
    request_rx: Receiver<WorkerRequest>,
    result_tx: Sender<AsyncMessage>,
    latest_request_id: Arc<AtomicU64>,
) {
    crate::core::logging::info("[decomp-pool] Starting multi-binary decompiler pool (FFI mode)");

    // Spawn the router thread that dispatches to per-binary workers
    std::thread::Builder::new()
        .name("decomp-router".to_string())
        .spawn(move || {
            let mut pool = DecompilerPool::new(result_tx, latest_request_id);

            loop {
                match request_rx.recv() {
                    Ok(request) => {
                        pool.dispatch(request);
                    }
                    Err(_) => {
                        crate::core::logging::info(
                            "[decomp-router] Request channel closed, exiting",
                        );
                        return;
                    }
                }
            }
        })
        .expect("Failed to spawn decompiler router thread");

    crate::core::logging::info("[decomp-pool] Router thread started");
}
