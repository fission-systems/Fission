#![cfg(not(feature = "native_decomp"))]

use crate::ui::gui::core::messages::AsyncMessage;
use crossbeam_channel::{Receiver, Sender};
use std::sync::{
    Arc,
    atomic::AtomicU64,
};

use super::WorkerRequest;

pub fn spawn_worker(
    request_rx: Receiver<WorkerRequest>,
    result_tx: Sender<AsyncMessage>,
    _latest_request_id: Arc<AtomicU64>,
) {
    crate::core::logging::warn(
        "[decomp-worker] Native decompiler not available (build with --features native_decomp)",
    );

    // Spawn a stub worker that responds with "not available" messages
    std::thread::Builder::new()
        .name("decomp-worker-stub".to_string())
        .spawn(move || {
            for request in request_rx {
                match request {
                    WorkerRequest::CfgAnalysis(req) => {
                        let _ = result_tx.send(AsyncMessage::CfgAnalysisResult {
                            address: req.address,
                            block_count: 0,
                            edge_count: 0,
                            cyclomatic_complexity: 1,
                            max_nesting_depth: 0,
                            loops: Vec::new(),
                            blocks: Vec::new(),
                            dot_content: String::new(),
                        });
                    }
                    WorkerRequest::LoadBinary(_) => {
                        // Context loaded - no specific message needed
                    }
                    WorkerRequest::ClearCache(_) => {}
                    WorkerRequest::Decompile(req) => {
                        let _ = result_tx.send(AsyncMessage::DecompileError {
                            address: req.address,
                            error: "Native decompiler not available (build with --features native_decomp)"
                                .to_string(),
                        });
                    }
                }
            }
        })
        .expect("Failed to spawn stub worker thread");

    crate::core::logging::info("[decomp-worker] Spawned stub worker (returns 'not available')");
}
