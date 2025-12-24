//! Decompilation Worker Pool - Multi-threaded decompilation with process pool.
//!
//! Features:
//! - Multiple worker threads for parallel processing
//! - DecompilerPool with N fission_decomp processes (auto-detected based on CPU)
//! - Request debouncing (only process latest user request)
//! - Background prefetching support

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, atomic::{AtomicU64, Ordering}};
use crate::analysis::decomp::{DecompilerPool, DecompilerServer, NativeDecompiler};
use crate::config::CONFIG;
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

/// Decompiler wrapper that prefers pool mode
enum DecompilerBackend {
    Pool(Arc<DecompilerPool>),
    Server(DecompilerServer),
    Legacy(NativeDecompiler),
}

impl DecompilerBackend {
    fn decompile(&mut self, bytes: &[u8], address: u64, is_64bit: bool) -> crate::core::prelude::Result<String> {
        match self {
            DecompilerBackend::Pool(pool) => pool.decompile(bytes, address, is_64bit).map_err(Into::into),
            DecompilerBackend::Server(server) => server.decompile(bytes, address, is_64bit).map_err(Into::into),
            DecompilerBackend::Legacy(legacy) => legacy.decompile(bytes, address, is_64bit).map_err(Into::into),
        }
    }
}

/// Spawns the decompiler worker threads (pool mode)
pub fn spawn_worker(
    request_rx: Receiver<DecompileRequest>,
    result_tx: Sender<AsyncMessage>,
    _native_decompiler: Arc<Mutex<Option<NativeDecompiler>>>,
    latest_request_id: Arc<AtomicU64>,
) {
    // Wrap receiver in Arc<Mutex> for sharing across threads
    let request_rx = Arc::new(Mutex::new(request_rx));
    
    // Create shared pool (will be initialized on first request)
    let pool: Arc<Mutex<Option<Arc<DecompilerPool>>>> = Arc::new(Mutex::new(None));
    
    // Get worker count from config
    let num_workers = CONFIG.decompiler.effective_num_workers();
    
    // Spawn multiple worker threads
    for i in 0..num_workers {
        let request_rx = Arc::clone(&request_rx);
        let result_tx = result_tx.clone();
        let latest_request_id = Arc::clone(&latest_request_id);
        let pool = Arc::clone(&pool);
        let num_workers = num_workers; // Copy for closure
        
        std::thread::Builder::new()
            .name(format!("decomp-worker-{}", i))
            .spawn(move || {
                worker_loop(i, num_workers, request_rx, result_tx, pool, latest_request_id);
            })
            .expect("Failed to spawn decompiler worker thread");
    }
    
    eprintln!("[decomp-worker] Spawned {} worker threads (auto-detected)", num_workers);
}

fn worker_loop(
    worker_id: usize,
    num_workers: usize,
    request_rx: Arc<Mutex<Receiver<DecompileRequest>>>,
    result_tx: Sender<AsyncMessage>,
    pool: Arc<Mutex<Option<Arc<DecompilerPool>>>>,
    latest_request_id: Arc<AtomicU64>,
) {
    loop {
        // Get next request (blocking)
        let request = {
            let rx = request_rx.lock().unwrap();
            match rx.recv() {
                Ok(req) => req,
                Err(_) => break, // Channel closed
            }
        };
        
        // For prefetch requests, check if there's a newer request
        if request.is_prefetch {
            // Skip stale prefetch requests
            continue;
        }
        
        // Check if this request is still the latest
        let current_latest = latest_request_id.load(Ordering::SeqCst);
        if request.request_id != current_latest {
            continue;
        }
        
        // Initialize pool if needed (only first thread does this)
        let decompiler_pool = {
            let mut pool_guard = pool.lock().unwrap();
            if pool_guard.is_none() {
                if let Some(cli_path) = crate::analysis::decomp::native::find_cli() {
                    let sla_dir = std::env::current_dir()
                        .unwrap()
                        .join("ghidra_decompiler")
                        .to_string_lossy()
                        .into_owned();
                    
                    match DecompilerPool::new(&cli_path, &sla_dir, num_workers) {
                        Ok(new_pool) => {
                            eprintln!("[decomp-worker-{}] Pool initialized with {} workers", worker_id, num_workers);
                            *pool_guard = Some(Arc::new(new_pool));
                        }
                        Err(e) => {
                            let _ = result_tx.send(AsyncMessage::DecompileError {
                                address: request.address,
                                error: format!("Failed to init decompiler pool: {}", e),
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
            pool_guard.clone()
        };
        
        // Check again before expensive operation
        let current_latest = latest_request_id.load(Ordering::SeqCst);
        if request.request_id != current_latest {
            continue;
        }
        
        // Perform decompilation using pool
        let result = if let Some(ref p) = decompiler_pool {
            p.decompile(&request.bytes, request.address, request.is_64bit).map_err(|e| crate::core::errors::FissionError::from(e))
        } else {
            Err(crate::err!(decompiler, "Decompiler pool not available"))
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
