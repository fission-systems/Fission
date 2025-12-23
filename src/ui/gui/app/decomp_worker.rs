//! Decompilation Worker - Single dedicated thread for decompile requests.
//!
//! Prevents resource waste from multiple thread spawns and implements
//! request debouncing (only process latest request).

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, atomic::{AtomicU64, Ordering}};
use crate::analysis::decomp::NativeDecompiler;
use crate::ui::gui::messages::AsyncMessage;

/// Request to decompile a function
pub struct DecompileRequest {
    /// Unique request ID for debouncing
    pub request_id: u64,
    /// Function bytes to decompile
    pub bytes: Vec<u8>,
    /// Base address
    pub address: u64,
    /// Is 64-bit architecture
    pub is_64bit: bool,
}

/// Spawns the decompiler worker thread
pub fn spawn_worker(
    request_rx: Receiver<DecompileRequest>,
    result_tx: Sender<AsyncMessage>,
    native_decompiler: Arc<Mutex<Option<NativeDecompiler>>>,
    latest_request_id: Arc<AtomicU64>,
) {
    std::thread::Builder::new()
        .name("decomp-worker".to_string())
        .spawn(move || {
            worker_loop(request_rx, result_tx, native_decompiler, latest_request_id);
        })
        .expect("Failed to spawn decompiler worker thread");
}

fn worker_loop(
    request_rx: Receiver<DecompileRequest>,
    result_tx: Sender<AsyncMessage>,
    native_decompiler: Arc<Mutex<Option<NativeDecompiler>>>,
    latest_request_id: Arc<AtomicU64>,
) {
    while let Ok(request) = request_rx.recv() {
        // Check if this request is still the latest
        let current_latest = latest_request_id.load(Ordering::SeqCst);
        if request.request_id != current_latest {
            // Newer request exists, skip this one
            continue;
        }
        
        // Ensure decompiler is initialized
        {
            let mut guard = native_decompiler.lock().unwrap();
            if guard.is_none() {
                if let Some(cli_path) = crate::analysis::decomp::native::find_cli() {
                    let sla_dir = std::env::current_dir()
                        .unwrap()
                        .join("ghidra_decompiler")
                        .to_string_lossy()
                        .into_owned();
                    
                    match NativeDecompiler::new(cli_path, &sla_dir) {
                        Ok(nd) => {
                            *guard = Some(nd);
                        }
                        Err(e) => {
                            let _ = result_tx.send(AsyncMessage::DecompileError {
                                address: request.address,
                                error: format!("Failed to init decompiler: {}", e),
                            });
                            continue;
                        }
                    }
                } else {
                    let _ = result_tx.send(AsyncMessage::DecompileError {
                        address: request.address,
                        error: "Decompiler CLI not found".to_string(),
                    });
                    continue;
                }
            }
        }
        
        // Check again before expensive operation
        let current_latest = latest_request_id.load(Ordering::SeqCst);
        if request.request_id != current_latest {
            continue;
        }
        
        // Perform decompilation
        let result = {
            let mut guard = native_decompiler.lock().unwrap();
            if let Some(nd) = guard.as_mut() {
                nd.decompile(&request.bytes, request.address, request.is_64bit)
            } else {
                Err(anyhow::anyhow!("Decompiler not available"))
            }
        };
        
        // Send result only if still latest
        let current_latest = latest_request_id.load(Ordering::SeqCst);
        if request.request_id == current_latest {
            match result {
                Ok(c_code) => {
                    let _ = result_tx.send(AsyncMessage::DecompileResult {
                        address: request.address,
                        c_code,
                    });
                }
                Err(e) => {
                    let _ = result_tx.send(AsyncMessage::DecompileError {
                        address: request.address,
                        error: e.to_string(),
                    });
                }
            }
        }
    }
}
