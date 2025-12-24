//! Decompilation Worker - Single dedicated thread for decompile requests.
//!
//! Features:
//! - Uses persistent DecompilerServer for faster repeated requests
//! - Request debouncing (only process latest request)
//! - Background prefetching of adjacent functions

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, atomic::{AtomicU64, Ordering}};
use crate::analysis::decomp::{DecompilerServer, NativeDecompiler};
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
    /// Is this a prefetch request (low priority, can be skipped)
    pub is_prefetch: bool,
}

impl DecompileRequest {
    pub fn new(request_id: u64, bytes: Vec<u8>, address: u64, is_64bit: bool) -> Self {
        Self {
            request_id,
            bytes,
            address,
            is_64bit,
            is_prefetch: false,
        }
    }

    pub fn prefetch(bytes: Vec<u8>, address: u64, is_64bit: bool) -> Self {
        Self {
            request_id: 0, // Prefetch doesn't use request ID
            bytes,
            address,
            is_64bit,
            is_prefetch: true,
        }
    }
}

/// Decompiler wrapper that prefers server mode but falls back to legacy
enum DecompilerBackend {
    Server(DecompilerServer),
    Legacy(NativeDecompiler),
}

impl DecompilerBackend {
    fn decompile(&mut self, bytes: &[u8], address: u64, is_64bit: bool) -> anyhow::Result<String> {
        match self {
            DecompilerBackend::Server(server) => server.decompile(bytes, address, is_64bit),
            DecompilerBackend::Legacy(legacy) => legacy.decompile(bytes, address, is_64bit),
        }
    }
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
    _legacy_decompiler: Arc<Mutex<Option<NativeDecompiler>>>,
    latest_request_id: Arc<AtomicU64>,
) {
    // Try to create server-mode decompiler
    let mut backend: Option<DecompilerBackend> = None;

    while let Ok(request) = request_rx.recv() {
        // For prefetch requests, check if there's a newer regular request
        if request.is_prefetch {
            // Try to receive without blocking to check for newer requests
            if request_rx.try_recv().is_ok() {
                // There's a newer request, skip this prefetch
                continue;
            }
        } else {
            // Check if this request is still the latest
            let current_latest = latest_request_id.load(Ordering::SeqCst);
            if request.request_id != current_latest {
                continue;
            }
        }
        
        // Initialize backend if needed
        if backend.is_none() {
            if let Some(cli_path) = crate::analysis::decomp::native::find_cli() {
                let sla_dir = std::env::current_dir()
                    .unwrap()
                    .join("ghidra_decompiler")
                    .to_string_lossy()
                    .into_owned();
                
                // Try server mode first (faster - keeps process alive)
                match DecompilerServer::new(&cli_path, &sla_dir) {
                    Ok(server) => {
                        eprintln!("[decomp-worker] Using server mode (fast)");
                        backend = Some(DecompilerBackend::Server(server));
                    }
                    Err(_) => {
                        // Fall back to legacy mode
                        match NativeDecompiler::new(&cli_path, &sla_dir) {
                            Ok(legacy) => {
                                eprintln!("[decomp-worker] Using legacy mode");
                                backend = Some(DecompilerBackend::Legacy(legacy));
                            }
                            Err(e) => {
                                if !request.is_prefetch {
                                    let _ = result_tx.send(AsyncMessage::DecompileError {
                                        address: request.address,
                                        error: format!("Failed to init decompiler: {}", e),
                                    });
                                }
                                continue;
                            }
                        }
                    }
                }
            } else {
                if !request.is_prefetch {
                    let _ = result_tx.send(AsyncMessage::DecompileError {
                        address: request.address,
                        error: "Decompiler CLI not found".to_string(),
                    });
                }
                continue;
            }
        }
        
        // Check again before expensive operation (skip for prefetch)
        if !request.is_prefetch {
            let current_latest = latest_request_id.load(Ordering::SeqCst);
            if request.request_id != current_latest {
                continue;
            }
        }
        
        // Perform decompilation
        let result = if let Some(ref mut be) = backend {
            be.decompile(&request.bytes, request.address, request.is_64bit)
        } else {
            Err(anyhow::anyhow!("Decompiler not available"))
        };
        
        // Send result only if still latest (or prefetch always sends for caching)
        if request.is_prefetch {
            // Prefetch results are just for caching, always send
            if let Ok(c_code) = result {
                let _ = result_tx.send(AsyncMessage::DecompileResult {
                    address: request.address,
                    c_code,
                });
            }
            // Silently ignore prefetch errors
        } else {
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
}
