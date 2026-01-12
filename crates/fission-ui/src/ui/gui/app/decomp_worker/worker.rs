#![cfg(feature = "native_decomp")]

use crate::ui::gui::core::messages::AsyncMessage;
use crossbeam_channel::Sender;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use fission_analysis::analysis::decomp::CachingDecompiler;

use super::WorkerRequest;
use super::native::{handle_binary_load_for_worker, handle_decompile_for_worker};

/// Worker loop for a single binary's decompiler context
pub(crate) fn binary_worker_loop(
    binary_id: String,
    request_rx: crossbeam_channel::Receiver<WorkerRequest>,
    result_tx: Sender<AsyncMessage>,
    latest_request_id: Arc<AtomicU64>,
) {
    // Each binary worker has its own decompiler context
    let mut native_decomp: Option<CachingDecompiler> = None;

    crate::core::logging::info(&format!(
        "[decomp-worker-{}] Worker started",
        &binary_id[..8.min(binary_id.len())]
    ));

    loop {
        let request = match request_rx.recv() {
            Ok(req) => req,
            Err(_) => {
                crate::core::logging::info(&format!(
                    "[decomp-worker-{}] Channel closed, exiting",
                    &binary_id[..8.min(binary_id.len())]
                ));
                return;
            }
        };

        match request {
            WorkerRequest::CfgAnalysis(req) => {
                let _ = result_tx.send(AsyncMessage::CfgAnalysisResult {
                    address: req.address,
                    block_count: 0,
                    edge_count: 0,
                    cyclomatic_complexity: 0,
                    max_nesting_depth: 0,
                    loops: Vec::new(),
                    blocks: Vec::new(),
                    dot_content: String::new(),
                });
            }
            WorkerRequest::ClearCache(_) => {
                if let Some(ref mut decomp) = native_decomp {
                    decomp.clear_cache();
                    crate::core::logging::info(&format!(
                        "[decomp-worker-{}] Cache cleared",
                        &binary_id[..8.min(binary_id.len())]
                    ));
                }
            }
            WorkerRequest::LoadBinary(req) => {
                handle_binary_load_for_worker(&req, &mut native_decomp, &result_tx);
            }
            WorkerRequest::Decompile(req) => {
                if req.request_id != latest_request_id.load(Ordering::SeqCst) {
                    continue;
                }
                handle_decompile_for_worker(&req, &mut native_decomp, &result_tx);
            }
        }
    }
}
