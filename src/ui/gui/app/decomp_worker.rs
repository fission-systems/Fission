//! Decompilation Worker Pool - Multi-threaded decompilation with process pool.
//!
//! Features:
//! - Multiple worker threads for parallel processing
//! - DecompilerPool with N fission_decomp processes (auto-detected based on CPU)
//! - DecompilerNative: Direct FFI when `native_decomp` feature enabled (10-100x faster)
//! - Request debouncing (only process latest user request)
//! - Background prefetching support

use crate::analysis::decomp::{DecompilerPool, DecompilerServer};
use crate::config::{DecompilerMode, CONFIG};
use crate::core::errors::FissionError;
use crate::ui::gui::messages::AsyncMessage;
use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

#[cfg(feature = "native_decomp")]
use crate::analysis::decomp::ffi::DecompilerNative;

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
    /// GDT types JSON path for Windows structure definitions
    pub gdt_json_path: Option<String>,
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
            gdt_json_path: None,
        }
    }

    pub fn load_binary(
        bytes: Vec<u8>,
        image_base: u64,
        iat_symbols: HashMap<u64, String>,
        gdt_json_path: Option<String>,
    ) -> Self {
        Self {
            request_id: 0, // Load request doesn't use ID
            bytes,
            address: 0,
            is_64bit: false, // irrelevant for load
            is_prefetch: false,
            is_binary_load: true,
            image_base,
            iat_symbols,
            gdt_json_path,
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
            gdt_json_path: None,
        }
    }
}

/// Decompiler wrapper - supports Native FFI, Pool, or Server mode
enum DecompilerBackend {
    #[cfg(feature = "native_decomp")]
    Native(DecompilerNative),
    Pool(Arc<DecompilerPool>),
    Server(Arc<Mutex<DecompilerServer>>),
}

impl DecompilerBackend {
    fn decompile(
        &mut self,
        bytes: &[u8],
        address: u64,
        is_64bit: bool,
    ) -> crate::core::prelude::Result<String> {
        match self {
            #[cfg(feature = "native_decomp")]
            DecompilerBackend::Native(native) => {
                // Native FFI decompilation - much faster!
                native.decompile(address)
            }
            DecompilerBackend::Pool(pool) => {
                pool.decompile(bytes, address, is_64bit).map_err(Into::into)
            }
            DecompilerBackend::Server(server) => {
                let mut guard = server
                    .lock()
                    .map_err(|e| FissionError::decompiler(format!("Lock error: {}", e)))?;
                guard.decompile(bytes, address, is_64bit)
            }
        }
    }

    #[cfg(feature = "native_decomp")]
    fn load_binary_native(
        &mut self,
        bytes: &[u8],
        image_base: u64,
        is_64bit: bool,
        iat_symbols: &HashMap<u64, String>,
    ) -> crate::core::prelude::Result<()> {
        match self {
            DecompilerBackend::Native(native) => {
                native.load_binary(bytes, image_base, is_64bit)?;
                native.add_symbols(iat_symbols);
                Ok(())
            }
            _ => Ok(()), // Pool/Server handle loading differently
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

    // Native decompiler (single instance, protected by mutex)
    #[cfg(feature = "native_decomp")]
    let native_decomp: Arc<Mutex<Option<DecompilerNative>>> = Arc::new(Mutex::new(None));

    // Get worker count from config
    // IMPORTANT: Native FFI mode uses single worker because Ghidra's global state
    // (SleighArchitecture::specpaths, print languages) is not thread-safe
    #[cfg(feature = "native_decomp")]
    let num_workers = 1; // Single worker for FFI to avoid Ghidra thread-safety issues
    #[cfg(not(feature = "native_decomp"))]
    let num_workers = CONFIG.decompiler.effective_num_workers();

    // Log which backend will be used
    #[cfg(feature = "native_decomp")]
    crate::core::logging::info(
        "[decomp-worker] Native FFI backend (single worker for thread safety)",
    );
    #[cfg(not(feature = "native_decomp"))]
    crate::core::logging::info("[decomp-worker] Using subprocess pool backend");

    // Spawn multiple worker threads
    for i in 0..num_workers {
        let request_rx = Arc::clone(&request_rx);
        let result_tx = result_tx.clone();
        let latest_request_id = Arc::clone(&latest_request_id);
        let pool = Arc::clone(&pool);
        #[cfg(feature = "native_decomp")]
        let native_decomp = Arc::clone(&native_decomp);
        let num_workers = num_workers; // Copy for closure

        std::thread::Builder::new()
            .name(format!("decomp-worker-{}", i))
            .spawn(move || {
                worker_loop(
                    i,
                    num_workers,
                    request_rx,
                    result_tx,
                    pool,
                    #[cfg(feature = "native_decomp")]
                    native_decomp,
                    latest_request_id,
                );
            })
            .expect("Failed to spawn decompiler worker thread");
    }

    crate::core::logging::info(&format!(
        "[decomp-worker] Spawned {} worker threads (auto-detected)",
        num_workers
    ));
}

fn worker_loop(
    worker_id: usize,
    num_workers: usize,
    request_rx: Arc<Mutex<Receiver<DecompileRequest>>>,
    result_tx: Sender<AsyncMessage>,
    pool: Arc<Mutex<Option<Arc<DecompilerPool>>>>,
    #[cfg(feature = "native_decomp")] native_decomp: Arc<Mutex<Option<DecompilerNative>>>,
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
                    crate::core::logging::warn(&format!(
                        "[decomp-worker-{}] Request queue mutex poisoned, recovering...",
                        worker_id
                    ));
                    poisoned.into_inner()
                }
            };
            match rx.recv() {
                Ok(req) => req,
                Err(_) => break, // Channel closed
            }
        };

        // Handle Binary Load Request
        if request.is_binary_load {
            #[cfg(feature = "native_decomp")]
            {
                // Use native FFI for loading (preferred)
                let mut native_guard = match native_decomp.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };

                // Initialize native decompiler if needed
                if native_guard.is_none() {
                    let sla_dir = std::env::current_dir()
                        .unwrap()
                        .join("ghidra_decompiler")
                        .to_string_lossy()
                        .into_owned();

                    match DecompilerNative::new(&sla_dir) {
                        Ok(native) => {
                            crate::core::logging::info(
                                "[decomp-worker] Native decompiler initialized",
                            );
                            *native_guard = Some(native);
                        }
                        Err(e) => {
                            crate::core::logging::warn(&format!(
                                "[decomp-worker] Failed to init native decompiler: {}, falling back to pool",
                                e
                            ));
                        }
                    }
                }

                // Load binary into native decompiler
                if let Some(ref mut native) = *native_guard {
                    // Detect 64-bit from PE header or assume true
                    let is_64bit = detect_is_64bit(&request.bytes);

                    if let Err(e) = native.load_binary(&request.bytes, request.image_base, is_64bit)
                    {
                        crate::core::logging::debug(&format!(
                            "[decomp-worker] Native load failed: {}",
                            e
                        ));
                    } else {
                        native.add_symbols(&request.iat_symbols);
                        if let Some(ref gdt) = request.gdt_json_path {
                            let _ = native.set_gdt(gdt);
                        }
                        crate::core::logging::info("[decomp-worker] Binary loaded via native FFI");
                        continue; // Success, skip pool fallback
                    }
                }
            }

            // Fallback: Load into subprocess pool
            let decompiler_pool = {
                let mut pool_guard = match pool.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        crate::core::logging::warn(&format!(
                            "[decomp-worker-{}] Pool mutex poisoned, recovering...",
                            worker_id
                        ));
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

                        if let Ok(new_pool) = DecompilerPool::new(&cli_path, &sla_dir, num_workers)
                        {
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

                if let Err(e) = p.load_binary(
                    &request.bytes,
                    &sla_dir,
                    request.image_base,
                    &request.iat_symbols,
                    request.gdt_json_path.as_deref(),
                ) {
                    crate::core::logging::debug(&format!(
                        "[decomp-worker] Failed to load binary: {}",
                        e
                    ));
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

                        if let Ok(new_pool) = DecompilerPool::new(&cli_path, &sla_dir, num_workers)
                        {
                            *pool_guard = Some(Arc::new(new_pool));
                        }
                    }
                }
                pool_guard.clone()
            };

            // Use non-blocking try_decompile for prefetch (don't block other requests)
            if let Some(ref p) = decompiler_pool {
                if let Some(result) =
                    p.try_decompile(&request.bytes, request.address, request.is_64bit)
                {
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

        // Try native decompiler first (when feature enabled)
        #[cfg(feature = "native_decomp")]
        {
            let native_guard = match native_decomp.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };

            if let Some(ref native) = *native_guard {
                // Check again before expensive operation
                let current_latest = latest_request_id.load(Ordering::SeqCst);
                if request.request_id != current_latest {
                    continue;
                }

                let result = native.decompile(request.address);

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
                continue; // Done with native, skip pool
            }
        }

        // Fallback: Initialize pool if needed (Lazy Local Cache)
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
                            crate::core::logging::info(&format!(
                                "[decomp-worker-{}] Pool initialized with {} workers",
                                worker_id, num_workers
                            ));
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

/// Detect if binary is 64-bit from PE header
fn detect_is_64bit(bytes: &[u8]) -> bool {
    // Check PE signature
    if bytes.len() < 0x40 {
        return true; // Default to 64-bit
    }

    // DOS header -> e_lfanew at offset 0x3C
    let pe_offset = if bytes.len() > 0x3F {
        u32::from_le_bytes([bytes[0x3C], bytes[0x3D], bytes[0x3E], bytes[0x3F]]) as usize
    } else {
        return true;
    };

    // Check PE signature and machine type
    if bytes.len() > pe_offset + 6 {
        let machine = u16::from_le_bytes([bytes[pe_offset + 4], bytes[pe_offset + 5]]);
        // 0x8664 = AMD64, 0x14c = i386
        machine == 0x8664
    } else {
        true // Default to 64-bit
    }
}
