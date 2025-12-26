//! Decompilation Worker Pool - Multi-threaded decompilation with process pool.
//!
//! Features:
//! - Multiple worker threads for parallel processing
//! - DecompilerPool with N fission_decomp processes (auto-detected based on CPU)
//! - Request debouncing (only process latest user request)
//! - Background prefetching support

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, atomic::{AtomicU64, Ordering}};
use std::collections::HashMap;
use crate::analysis::decomp::{DecompilerPool, DecompilerServer};
use crate::config::{CONFIG, DecompilerMode};
use crate::core::errors::FissionError;
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
    /// Is this a binary load request (loads full binary into context)
    pub is_binary_load: bool,
    /// Image base address for binary load (critical for address translation)
    pub image_base: u64,
    /// IAT symbols to inject into decompiler (address -> name)
    pub iat_symbols: HashMap<u64, String>,
}

impl DecompileRequest {
    pub fn new(request_id: u64, bytes: Vec<u8>, address: u64, is_64bit: bool) -> Self {
        Self {
            request_id,
            bytes,
            address,
            is_64bit,
            is_prefetch: false,
            is_binary_load: false,
            image_base: 0,
            iat_symbols: HashMap::new(),
        }
    }

    pub fn load_binary(bytes: Vec<u8>, image_base: u64, iat_symbols: HashMap<u64, String>) -> Self {
        Self {
            request_id: 0, // Load request doesn't use ID
            bytes,
            address: 0,
            is_64bit: false, // irrelevant for load
            is_prefetch: false,
            is_binary_load: true,
            image_base,
            iat_symbols,
        }
    }

    pub fn prefetch(bytes: Vec<u8>, address: u64, is_64bit: bool) -> Self {
        Self {
            request_id: 0, // Prefetch doesn't use request ID
            bytes,
            address,
            is_64bit,
            is_prefetch: true,
            is_binary_load: false,
            image_base: 0,
            iat_symbols: HashMap::new(),
        }
    }
}

/// Decompiler wrapper - supports Single server or Pool mode
enum DecompilerBackend {
    Pool(Arc<DecompilerPool>),
    Server(Arc<Mutex<DecompilerServer>>),
}

impl DecompilerBackend {
    fn decompile(&mut self, bytes: &[u8], address: u64, is_64bit: bool) -> crate::core::prelude::Result<String> {
        match self {
            DecompilerBackend::Pool(pool) => pool.decompile(bytes, address, is_64bit).map_err(Into::into),
            DecompilerBackend::Server(server) => {
                let mut guard = server.lock().map_err(|e| FissionError::decompiler(format!("Lock error: {}", e)))?;
                guard.decompile(bytes, address, is_64bit)
            }
        }
    }
}

/// Spawns the decompiler worker threads
pub fn spawn_worker(
    request_rx: Receiver<DecompileRequest>,
    result_tx: Sender<AsyncMessage>,
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
    
    crate::core::logging::info(&format!("[decomp-worker] Spawned {} worker threads (auto-detected)", num_workers));
}

fn worker_loop(
    worker_id: usize,
    num_workers: usize,
    request_rx: Arc<Mutex<Receiver<DecompileRequest>>>,
    result_tx: Sender<AsyncMessage>,
    pool: Arc<Mutex<Option<Arc<DecompilerPool>>>>,
    latest_request_id: Arc<AtomicU64>,
) {
    // Local cache of the pool to avoid locking mutex on every request
    let mut local_pool: Option<Arc<DecompilerPool>> = None;

    loop {
        // Get next request (blocking)
        let request = {
            let rx = match request_rx.lock() {
                Ok(guard) => guard,
                Err(poisoned) => {
                    crate::core::logging::warn(&format!("[decomp-worker-{}] Request queue mutex poisoned, recovering...", worker_id));
                    poisoned.into_inner()
                }
            };
            match rx.recv() {
                Ok(req) => req,
                Err(_) => break, // Channel closed
            }
        };
        
        // Handle Binary Load Request (Broadcast to pool)
        if request.is_binary_load {
             // Initialize pool if needed
             let decompiler_pool = {
                let mut pool_guard = match pool.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        crate::core::logging::warn(&format!("[decomp-worker-{}] Pool mutex poisoned, recovering...", worker_id));
                        poisoned.into_inner()
                    }
                };
                if pool_guard.is_none() {
                    if let Some(cli_path) = crate::analysis::decomp::native::find_cli() {
                        let sla_dir = std::env::current_dir()
                            .unwrap()
                            .join("ghidra_decompiler")
                            .to_string_lossy()
                            .into_owned();

                        if let Ok(new_pool) = DecompilerPool::new(&cli_path, &sla_dir, num_workers) {
                            *pool_guard = Some(Arc::new(new_pool));
                        }
                    }
                }
                pool_guard.clone()
            };

            if let Some(ref p) = decompiler_pool {
                let sla_dir = std::env::current_dir()
                    .unwrap()
                    .join("ghidra_decompiler")
                    .to_string_lossy()
                    .into_owned();
                
                // Load binary into ALL workers
                // Note: Since each worker thread picks up requests from the SAME channel,
                // we have a problem: only ONE worker will pick up this request!
                // BUT `pool.load_binary` iterates over ALL workers in the pool.
                // So ANY worker that picks this up can trigger the load on ALL workers.
                // However, `pool.load_binary` requires locking the workers.
                // If other workers are busy, we might block or fail.
                // Ideally, `load_binary` should be called when no other work is happening (e.g. file load).
                
                if let Err(e) = p.load_binary(&request.bytes, &sla_dir, request.image_base, &request.iat_symbols) {
                     crate::core::logging::error(&format!("[decomp-worker] Failed to load binary: {}", e));
                } else {
                     crate::core::logging::info("[decomp-worker] Binary loaded successfully");
                }
            }
            continue;
        }

        // For prefetch requests, check config and use non-blocking decompile
        if request.is_prefetch {
            // Skip if prefetch is disabled in config
            if !CONFIG.decompiler.enable_prefetch {
                continue;
            }

            // Initialize pool if needed for prefetch
            let decompiler_pool = {
                let mut pool_guard = match pool.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                if pool_guard.is_none() {
                    if let Some(cli_path) = crate::analysis::decomp::native::find_cli() {
                        let sla_dir = std::env::current_dir()
                            .map(|p| p.join("ghidra_decompiler").to_string_lossy().into_owned())
                            .unwrap_or_else(|_| ".".to_string());

                        if let Ok(new_pool) = DecompilerPool::new(&cli_path, &sla_dir, num_workers) {
                            *pool_guard = Some(Arc::new(new_pool));
                        }
                    }
                }
                pool_guard.clone()
            };

            // Use non-blocking try_decompile for prefetch (don't block other requests)
            if let Some(ref p) = decompiler_pool {
                if let Some(result) = p.try_decompile(&request.bytes, request.address, request.is_64bit) {
                    if let Ok(c_code) = result {
                        // Send prefetch result (will be cached but won't update UI)
                        let _ = result_tx.send(AsyncMessage::DecompileResult {
                            address: request.address,
                            c_code,
                        });
                    }
                }
                // If all workers busy, skip prefetch silently
            }
            continue;
        }
        
        // Check if this request is still the latest
        let current_latest = latest_request_id.load(Ordering::SeqCst);
        if request.request_id != current_latest {
            continue;
        }
        
        // Initialize pool if needed (Lazy Local Cache)
        if local_pool.is_none() {
            let mut pool_guard = match pool.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            
            if pool_guard.is_none() {
                 if let Some(cli_path) = crate::analysis::decomp::native::find_cli() {
                    let sla_dir = std::env::current_dir()
                        .map(|p| p.join("ghidra_decompiler").to_string_lossy().into_owned())
                        .unwrap_or_else(|_| ".".to_string());
                    
                    match DecompilerPool::new(&cli_path, &sla_dir, num_workers) {
                        Ok(new_pool) => {
                            crate::core::logging::info(&format!("[decomp-worker-{}] Pool initialized with {} workers", worker_id, num_workers));
                            *pool_guard = Some(Arc::new(new_pool));
                        }
                        Err(e) => {
                            let _ = result_tx.send(AsyncMessage::DecompileError {
                                address: request.address,
                                error: format!("Failed to init decompiler pool: {}", e),
                            });
                        }
                    }
                } else {
                    let _ = result_tx.send(AsyncMessage::DecompileError {
                        address: request.address,
                        error: "Decompiler CLI not found".to_string(),
                    });
                }
            }
            local_pool = pool_guard.clone();
        }

        // Check again before expensive operation
        let current_latest = latest_request_id.load(Ordering::SeqCst);
        if request.request_id != current_latest {
            continue;
        }
        
        // Perform decompilation using local pool ref (No Mutex Lock!)
        let result = if let Some(ref p) = local_pool {
            p.decompile(&request.bytes, request.address, request.is_64bit)
        } else {
            Err(FissionError::decompiler("Decompiler pool not available"))
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
