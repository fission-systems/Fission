//! Decompilation Worker Pool - Multi-threaded decompilation with FFI.
//!
//! Features:
//! - Multiple worker threads for parallel processing
//! - DecompilerNative: Direct FFI to libdecomp (10-100x faster than subprocess)
//! - Request debouncing (only process latest user request)
//! - Background prefetching support

use crate::ui::gui::core::messages::AsyncMessage;
use crossbeam_channel::{Receiver, Sender};
use fission_core::config::CONFIG;
use fission_loader::detect_pe_is_64bit;
use fission_loader::loader::FunctionInfo;
use fission_loader::loader::types::SectionInfo;
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

/// Request to decompile a function
#[derive(Debug, Clone)]
pub struct DecompileRequest {
    /// Unique request ID for debouncing
    pub request_id: u64,
    /// Binary identifier (hash) for routing to correct worker
    pub binary_id: String,
    /// Function bytes to decompile
    pub bytes: Vec<u8>,
    /// Base address
    pub address: u64,
    /// Is 64-bit architecture
    #[allow(dead_code)]
    pub is_64bit: bool,
    /// Is this a prefetch request (low priority, can be skipped)
    #[allow(dead_code)]
    pub is_prefetch: bool,
    /// Is this a binary load request (loads full binary into context)
    pub is_binary_load: bool,
    /// Image base address for binary load (critical for address translation)
    pub image_base: u64,
    /// IAT symbols to inject into decompiler (address -> name)
    pub iat_symbols: HashMap<u64, String>,
    /// Global data symbols to improve global name cleanup
    pub global_symbols: HashMap<u64, String>,
    /// Known function symbols for on-demand lookups
    pub functions: Vec<FunctionInfo>,
    /// GDT types JSON path for Windows structure definitions
    #[allow(dead_code)]
    pub gdt_json_path: Option<String>,
    /// Binary sections for memory mapping
    pub sections: Vec<SectionInfo>,
    /// Binary hash for persistent caching
    pub binary_hash: String,
    /// Is this a CFG analysis request
    pub is_cfg_request: bool,
    /// Is this a cache clear request
    pub is_clear_cache: bool,
}

impl DecompileRequest {
    #[allow(dead_code)]
    pub fn new(request_id: u64, bytes: Vec<u8>, address: u64, is_64bit: bool) -> Self {
        Self {
            request_id,
            binary_id: String::new(),
            bytes,
            address,
            is_64bit,
            is_prefetch: false,
            is_binary_load: false,
            image_base: 0,
            iat_symbols: HashMap::new(),
            global_symbols: HashMap::new(),
            functions: Vec::new(),
            gdt_json_path: None,
            sections: Vec::new(),
            binary_hash: String::new(),
            is_cfg_request: false,
            is_clear_cache: false,
        }
    }

    /// Create a request to clear the decompiler cache
    pub fn clear_cache() -> Self {
        Self {
            request_id: 0,
            binary_id: String::new(),
            bytes: Vec::new(),
            address: 0,
            is_64bit: false,
            is_prefetch: false,
            is_binary_load: false,
            image_base: 0,
            iat_symbols: HashMap::new(),
            global_symbols: HashMap::new(),
            functions: Vec::new(),
            gdt_json_path: None,
            sections: Vec::new(),
            binary_hash: String::new(),
            is_cfg_request: false,
            is_clear_cache: true,
        }
    }

    /// Create a CFG analysis request
    pub fn cfg_analysis(address: u64) -> Self {
        Self {
            request_id: 0,
            binary_id: String::new(),
            bytes: Vec::new(),
            address,
            is_64bit: false,
            is_prefetch: false,
            is_binary_load: false,
            image_base: 0,
            iat_symbols: HashMap::new(),
            global_symbols: HashMap::new(),
            functions: Vec::new(),
            gdt_json_path: None,
            sections: Vec::new(),
            binary_hash: String::new(),
            is_cfg_request: true,
            is_clear_cache: false,
        }
    }

    pub fn load_binary(
        bytes: Vec<u8>,
        image_base: u64,
        iat_symbols: HashMap<u64, String>,
        global_symbols: HashMap<u64, String>,
        functions: Vec<FunctionInfo>,
        gdt_json_path: Option<String>,
        sections: Vec<SectionInfo>,
        binary_hash: String,
    ) -> Self {
        Self {
            request_id: 0,
            binary_id: binary_hash.clone(),
            bytes,
            address: 0,
            is_64bit: false,
            is_prefetch: false,
            is_binary_load: true,
            image_base,
            iat_symbols,
            global_symbols,
            functions,
            gdt_json_path,
            sections,
            binary_hash,
            is_cfg_request: false,
            is_clear_cache: false,
        }
    }

    #[allow(dead_code)]
    pub fn prefetch(bytes: Vec<u8>, address: u64, is_64bit: bool) -> Self {
        Self {
            request_id: 0,
            binary_id: String::new(),
            bytes,
            address,
            is_64bit,
            is_prefetch: true,
            is_binary_load: false,
            image_base: 0,
            iat_symbols: HashMap::new(),
            global_symbols: HashMap::new(),
            functions: Vec::new(),
            gdt_json_path: None,
            sections: Vec::new(),
            binary_hash: String::new(),
            is_cfg_request: false,
            is_clear_cache: false,
        }
    }
}

// =============================================================================
// Native implementation (requires native_decomp feature)
// =============================================================================

/// Worker handle for a specific binary
#[cfg(feature = "native_decomp")]
struct BinaryWorker {
    /// Channel to send requests to this worker
    tx: crossbeam_channel::Sender<DecompileRequest>,
}

/// Pool of per-binary decompiler workers
#[cfg(feature = "native_decomp")]
struct DecompilerPool {
    workers: HashMap<String, BinaryWorker>,
    result_tx: Sender<AsyncMessage>,
    latest_request_id: Arc<AtomicU64>,
}

#[cfg(feature = "native_decomp")]
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

    fn dispatch(&mut self, request: DecompileRequest) {
        let binary_id = if request.binary_id.is_empty() {
            // Use binary_hash as fallback if binary_id not set
            if request.binary_hash.is_empty() {
                "default".to_string()
            } else {
                request.binary_hash.clone()
            }
        } else {
            request.binary_id.clone()
        };

        let worker = self.get_or_create_worker(&binary_id);
        let _ = worker.tx.send(request);
    }
}

#[cfg(feature = "native_decomp")]
pub fn spawn_worker(
    request_rx: Receiver<DecompileRequest>,
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

/// Worker loop for a single binary's decompiler context
#[cfg(feature = "native_decomp")]
fn binary_worker_loop(
    binary_id: String,
    request_rx: crossbeam_channel::Receiver<DecompileRequest>,
    result_tx: Sender<AsyncMessage>,
    latest_request_id: Arc<AtomicU64>,
) {
    use fission_analysis::analysis::decomp::CachingDecompiler;

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

        // Handle CFG analysis requests
        if request.is_cfg_request {
            let _ = result_tx.send(AsyncMessage::CfgAnalysisResult {
                address: request.address,
                block_count: 0,
                edge_count: 0,
                cyclomatic_complexity: 0,
                max_nesting_depth: 0,
                loops: Vec::new(),
                blocks: Vec::new(),
                dot_content: String::new(),
            });
            continue;
        }

        // Handle cache clear requests
        if request.is_clear_cache {
            if let Some(ref mut decomp) = native_decomp {
                decomp.clear_cache();
                crate::core::logging::info(&format!(
                    "[decomp-worker-{}] Cache cleared",
                    &binary_id[..8.min(binary_id.len())]
                ));
            }
            continue;
        }

        // Handle binary load requests
        if request.is_binary_load {
            handle_binary_load_for_worker(&request, &mut native_decomp, &result_tx);
            continue;
        }

        // Debouncing: Skip if this is not the latest request
        if request.request_id != latest_request_id.load(Ordering::SeqCst) {
            continue;
        }

        // Handle decompilation request
        handle_decompile_for_worker(&request, &mut native_decomp, &result_tx);
    }
}

/// Handle binary load for a specific worker
#[cfg(feature = "native_decomp")]
fn handle_binary_load_for_worker(
    request: &DecompileRequest,
    native_decomp: &mut Option<fission_analysis::analysis::decomp::CachingDecompiler>,
    result_tx: &Sender<AsyncMessage>,
) {
    // Resolve SLA directory
    let sla_dir = match CONFIG.decompiler.resolve_sla_directory() {
        Ok(dir) => dir,
        Err(e) => {
            let _ = result_tx.send(AsyncMessage::DecompilerContextError {
                error: e,
                suggestion: Some("Set FISSION_SLA_DIR environment variable".to_string()),
            });
            return;
        }
    };

    // Build a minimal LoadedBinary for CachingDecompiler
    let is_64bit = detect_pe_is_64bit(&request.bytes);
    let mut dummy_binary = fission_loader::loader::LoadedBinaryBuilder::new(
        "dummy".to_string(),
        request.bytes.clone(),
    )
    .image_base(request.image_base)
    .is_64bit(is_64bit)
    .format("PE")
    .add_sections(request.sections.clone())
    .build()
    .map_err(|e| format!("Failed to build binary: {}", e))
    .unwrap(); // Assuming build won't fail with basic inputs

    dummy_binary.inner_mut().hash = request.binary_hash.clone();

    // Create CachingDecompiler
    match fission_analysis::analysis::decomp::CachingDecompiler::new(&dummy_binary, &sla_dir, 100) {
        Ok(mut decomp) => {
            // Load binary data
            let inner = decomp.inner_mut();
            if let Err(e) = inner.load_binary(&request.bytes, request.image_base, is_64bit) {
                let _ = result_tx.send(AsyncMessage::DecompilerContextError {
                    error: format!("Failed to load binary: {}", e),
                    suggestion: None,
                });
                return;
            }

            // Register sections
            for section in &request.sections {
                let _ = inner.add_memory_block(
                    &section.name,
                    section.virtual_address,
                    section.virtual_size,
                    section.file_offset,
                    section.file_size,
                    section.is_executable,
                    section.is_writable,
                );
            }

            // Add symbols
            inner.add_symbols(&request.iat_symbols);
            inner.add_global_symbols(&request.global_symbols);

            // Add function entries
            for func in &request.functions {
                let _ = inner.add_function(func.address, Some(&func.name));
            }

            // Set symbol provider
            inner.set_symbol_provider(
                &request.functions,
                &request.global_symbols,
                &request.sections,
            );

            *native_decomp = Some(decomp);
            crate::core::logging::info(&format!(
                "[decomp-worker] Binary context loaded (hash: {}...)",
                &request.binary_hash[..16.min(request.binary_hash.len())]
            ));
            let _ = result_tx.send(AsyncMessage::DecompilerContextLoaded);
        }
        Err(e) => {
            let _ = result_tx.send(AsyncMessage::DecompilerContextError {
                error: format!("Failed to create decompiler: {}", e),
                suggestion: Some("Check libdecomp library is accessible".to_string()),
            });
        }
    }
}

/// Handle decompilation for a specific worker
#[cfg(feature = "native_decomp")]
fn handle_decompile_for_worker(
    request: &DecompileRequest,
    native_decomp: &mut Option<fission_analysis::analysis::decomp::CachingDecompiler>,
    result_tx: &Sender<AsyncMessage>,
) {
    if let Some(decomp) = native_decomp {
        match decomp.decompile(request.address) {
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
    } else {
        let _ = result_tx.send(AsyncMessage::DecompileError {
            address: request.address,
            error: "Native decompiler not initialized via Worker Pool".to_string(),
        });
    }
}

#[cfg(all(feature = "native_decomp", feature = "legacy_single_worker"))]
fn worker_loop_native(
    worker_id: usize,
    request_rx: Arc<Mutex<Receiver<DecompileRequest>>>,
    result_tx: Sender<AsyncMessage>,
    native_decomp: Arc<Mutex<Option<fission_analysis::analysis::decomp::CachingDecompiler>>>,
    latest_request_id: Arc<AtomicU64>,
) {
    loop {
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
                Err(_) => {
                    crate::core::logging::info(&format!(
                        "[decomp-worker-{}] Request channel closed, exiting",
                        worker_id
                    ));
                    return;
                }
            }
        };

        // Handle CFG analysis requests
        if request.is_cfg_request {
            let _ = result_tx.send(AsyncMessage::CfgAnalysisResult {
                address: request.address,
                block_count: 0,
                edge_count: 0,
                cyclomatic_complexity: 0,
                max_nesting_depth: 0,
                loops: Vec::new(),
                blocks: Vec::new(),
                dot_content: String::new(),
            });
            continue;
        }

        // Handle cache clear requests
        if request.is_clear_cache {
            let mut decomp_guard = match native_decomp.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            if let Some(ref mut decomp) = *decomp_guard {
                decomp.clear_cache();
                crate::core::logging::info("[decomp-worker] Persistent decompiler cache cleared");
            }
            continue;
        }

        // Handle binary load requests
        if request.is_binary_load {
            _handle_binary_load_native(&request, &native_decomp, &result_tx);
            continue;
        }

        // Debouncing: Skip if this is not the latest request
        if request.request_id != latest_request_id.load(Ordering::SeqCst) {
            continue;
        }

        // Decompile the function
        handle_decompile_native(&request, &native_decomp, &result_tx);
    }
}

#[cfg(feature = "native_decomp")]
fn _handle_binary_load_native(
    request: &DecompileRequest,
    native_decomp: &Arc<Mutex<Option<fission_analysis::analysis::decomp::CachingDecompiler>>>,
    result_tx: &Sender<AsyncMessage>,
) {
    // Step 1: Resolve SLA directory path
    let sla_dir = CONFIG.decompiler.resolve_sla_directory();

    let sla_dir = match sla_dir {
        Ok(dir) => dir,
        Err(e) => {
            crate::core::logging::error(&format!("SLA directory error: {}", e));
            let _ = result_tx.send(AsyncMessage::DecompilerContextError {
                error: e.clone(),
                suggestion: Some(
                    "Set FISSION_SLA_DIR environment variable to the Ghidra languages folder, \
                     or ensure ghidra_decompiler/languages exists in the workspace"
                        .to_string(),
                ),
            });
            return;
        }
    };

    crate::core::logging::info(&format!("[decomp-worker] Using SLA directory: {}", sla_dir));

    // Step 2: Initialize native decompiler with CachingDecompiler
    let mut decomp_guard = match native_decomp.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    // Use dummy LoadedBinary for CachingDecompiler initialization as we only need the hash
    // In a real scenario, we should pass the actual LoadedBinary if available
    let dummy_binary =
        fission_loader::loader::LoadedBinaryBuilder::new("dummy".to_string(), Vec::new())
            .build()
            .expect("Failed to build dummy binary");

    // Override hash with the actual binary hash from request
    let mut actual_binary = dummy_binary;
    actual_binary.hash = request.binary_hash.clone();

    match fission_analysis::analysis::decomp::CachingDecompiler::new(&actual_binary, &sla_dir, 100)
    {
        Ok(decomp) => {
            *decomp_guard = Some(decomp);
            crate::core::logging::info(
                "[decomp-worker] Caching decompiler initialized successfully",
            );
        }
        Err(e) => {
            let error_msg = format!("Failed to initialize caching decompiler: {}", e);
            crate::core::logging::error(&error_msg);
            let _ = result_tx.send(AsyncMessage::DecompilerContextError {
                error: error_msg,
                suggestion: Some(
                    "Ensure libdecomp.dylib is built and accessible. \
                     Check ghidra_decompiler/build/libdecomp.dylib exists."
                        .to_string(),
                ),
            });
            return;
        }
    }
    drop(decomp_guard);

    // Step 3: Load binary into decompiler context
    let mut decomp_guard = match native_decomp.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    if let Some(ref mut caching_decomp) = *decomp_guard {
        let decomp = caching_decomp.inner_mut();
        let is_64bit = detect_pe_is_64bit(&request.bytes);

        crate::core::logging::info(&format!(
            "[decomp-worker] Loading binary: {} bytes, base=0x{:x}, 64bit={}",
            request.bytes.len(),
            request.image_base,
            is_64bit
        ));

        if let Err(e) = decomp.load_binary(&request.bytes, request.image_base, is_64bit) {
            let error_msg = format!("Failed to load binary: {}", e);
            crate::core::logging::error(&error_msg);
            let _ = result_tx.send(AsyncMessage::DecompilerContextError {
                error: error_msg,
                suggestion: Some(
                    "The binary format may be unsupported. \
                     Try a different file or check if it's a valid PE/ELF/Mach-O."
                        .to_string(),
                ),
            });
            return;
        }

        // Step 4: Register sections
        let section_count = request.sections.len();
        for section in &request.sections {
            let _ = decomp.add_memory_block(
                &section.name,
                section.virtual_address,
                section.virtual_size,
                section.file_offset,
                section.file_size,
                section.is_executable,
                section.is_writable,
            );
        }
        crate::core::logging::debug(&format!(
            "[decomp-worker] Registered {} sections",
            section_count
        ));

        // Step 5: Add symbols
        let iat_count = request.iat_symbols.len();
        let global_count = request.global_symbols.len();
        decomp.add_symbols(&request.iat_symbols);
        decomp.add_global_symbols(&request.global_symbols);
        crate::core::logging::debug(&format!(
            "[decomp-worker] Added {} IAT + {} global symbols",
            iat_count, global_count
        ));

        // Step 6: Add function entries
        let func_count = request.functions.len();
        for func in &request.functions {
            let _ = decomp.add_function(func.address, Some(&func.name));
        }
        crate::core::logging::debug(&format!(
            "[decomp-worker] Added {} function entries",
            func_count
        ));

        // Step 7: Set up symbol provider for on-demand lookups
        decomp.set_symbol_provider(
            &request.functions,
            &request.global_symbols,
            &request.sections,
        );

        crate::core::logging::info("[decomp-worker] Binary loaded successfully, context ready");
    }

    let _ = result_tx.send(AsyncMessage::DecompilerContextLoaded);
}

#[cfg(feature = "native_decomp")]
fn _handle_decompile_native(
    request: &DecompileRequest,
    native_decomp: &Arc<Mutex<Option<fission_analysis::analysis::decomp::CachingDecompiler>>>,
    result_tx: &Sender<AsyncMessage>,
) {
    let mut decomp_guard = match native_decomp.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    if let Some(ref mut decomp) = *decomp_guard {
        match decomp.decompile(request.address) {
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
    } else {
        let _ = result_tx.send(AsyncMessage::DecompileError {
            address: request.address,
            error: "Native decompiler not initialized".to_string(),
        });
    }
}

// =============================================================================
// Stub implementation (when native_decomp feature is disabled)
// =============================================================================

#[cfg(not(feature = "native_decomp"))]
pub fn spawn_worker(
    request_rx: Receiver<DecompileRequest>,
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
                if request.is_cfg_request {
                    let _ = result_tx.send(AsyncMessage::CfgAnalysisResult {
                        address: request.address,
                        block_count: 0,
                        edge_count: 0,
                        cyclomatic_complexity: 1,
                        max_nesting_depth: 0,
                        loops: Vec::new(),
                        blocks: Vec::new(),
                        dot_content: String::new(),
                    });
                } else if request.is_binary_load {
                    // Context loaded - no specific message needed
                } else {
                    let _ = result_tx.send(AsyncMessage::DecompileError {
                        address: request.address,
                        error: "Native decompiler not available (build with --features native_decomp)".to_string(),
                    });
                }
            }
        })
        .expect("Failed to spawn stub worker thread");

    crate::core::logging::info("[decomp-worker] Spawned stub worker (returns 'not available')");
}
