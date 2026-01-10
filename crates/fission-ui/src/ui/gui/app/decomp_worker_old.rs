//! Decompilation Worker Pool - Multi-threaded decompilation with FFI.
//!
//! Features:
//! - Multiple worker threads for parallel processing
//! - DecompilerNative: Direct FFI to libdecomp (10-100x faster than subprocess)
//! - Request debouncing (only process latest user request)
//! - Background prefetching support
//!
//! **Migration Note**: This module now uses FFI exclusively. The old subprocess
//! pool has been deprecated in favor of the faster in-process FFI approach.

// Native decompiler implementation (requires libdecomp)
#[cfg(feature = "native_decomp")]
mod native_impl {
    use fission_ffi::DecompilerNative;
    use fission_loader::loader::FunctionInfo;
use fission_loader::loader::types::SectionInfo;
use crate::ui::gui::core::messages::AsyncMessage;
use crossbeam_channel::{Receiver, Sender};
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

/// Request to decompile a function
pub struct DecompileRequest {
    /// Unique request ID for debouncing
    pub request_id: u64,
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
    pub gdt_json_path: Option<String>,
    /// Binary sections for memory mapping
    pub sections: Vec<SectionInfo>,
    /// Is this a CFG analysis request
    pub is_cfg_request: bool,
}

impl DecompileRequest {
    #[allow(dead_code)]
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
            global_symbols: HashMap::new(),
            functions: Vec::new(),
            gdt_json_path: None,
            sections: Vec::new(),
            is_cfg_request: false,
        }
    }

    /// Create a CFG analysis request
    pub fn cfg_analysis(address: u64) -> Self {
        Self {
            request_id: 0,
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
            is_cfg_request: true,
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
            global_symbols,
            functions,
            gdt_json_path,
            sections,
            is_cfg_request: false,
        }
    }

    #[allow(dead_code)]
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
            global_symbols: HashMap::new(),
            functions: Vec::new(),
            gdt_json_path: None,
            sections: Vec::new(),
            is_cfg_request: false,
        }
    }
}

/// Spawns the decompiler worker threads using FFI backend
pub fn spawn_worker(
    request_rx: Receiver<DecompileRequest>,
    result_tx: Sender<AsyncMessage>,
    latest_request_id: Arc<AtomicU64>,
) {
    // Wrap receiver in Arc<Mutex> for sharing across threads
    let request_rx = Arc::new(Mutex::new(request_rx));

    // Native decompiler (single instance per worker, protected by mutex)
    let native_decomp: Arc<Mutex<Option<DecompilerNative>>> = Arc::new(Mutex::new(None));

    // Get worker count from config
    // IMPORTANT: Native FFI mode uses single worker because Ghidra's global state
    // (SleighArchitecture::specpaths, print languages) is not thread-safe
    let num_workers = 1; // Single worker for FFI to avoid Ghidra thread-safety issues

    // Log which backend will be used
    crate::core::logging::info(
        "[decomp-worker] Native FFI backend (single worker for thread safety)",
    );

    // Spawn worker thread
    for i in 0..num_workers {
        let request_rx = Arc::clone(&request_rx);
        let result_tx = result_tx.clone();
        let latest_request_id = Arc::clone(&latest_request_id);
        let native_decomp = Arc::clone(&native_decomp);

        std::thread::Builder::new()
            .name(format!("decomp-worker-{}", i))
            .spawn(move || {
                worker_loop(i, request_rx, result_tx, native_decomp, latest_request_id);
            })
            .expect("Failed to spawn decompiler worker thread");
    }

    crate::core::logging::info("[decomp-worker] Spawned 1 worker thread (FFI mode)");
}

fn worker_loop(
    worker_id: usize,
    request_rx: Arc<Mutex<Receiver<DecompileRequest>>>,
    result_tx: Sender<AsyncMessage>,
    native_decomp: Arc<Mutex<Option<DecompilerNative>>>,
    latest_request_id: Arc<AtomicU64>,
) {
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
            let mut native_guard = match native_decomp.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };

            // Initialize native decompiler if needed
            if native_guard.is_none() {
                let sla_dir = std::env::current_dir()
                    .unwrap()
                    .join("ghidra_decompiler")
                    .join("languages")
                    .to_string_lossy()
                    .into_owned();

                match DecompilerNative::new(&sla_dir) {
                    Ok(native) => {
                        crate::core::logging::info("[decomp-worker] Native decompiler initialized");
                        *native_guard = Some(native);
                    }
                    Err(e) => {
                        crate::core::logging::error(&format!(
                            "[decomp-worker] Failed to init native decompiler: {}",
                            e
                        ));
                        continue;
                    }
                }
            }

            // Load binary into native decompiler
            if let Some(ref mut native) = *native_guard {
                // Detect 64-bit from PE header or assume true
                let is_64bit = detect_is_64bit(&request.bytes);

                if let Err(e) = native.load_binary(&request.bytes, request.image_base, is_64bit) {
                    crate::core::logging::error(&format!(
                        "[decomp-worker] Native load failed: {}",
                        e
                    ));
                } else {
                    // Register sections with the decompiler
                    for section in &request.sections {
                        if let Err(e) = native.add_memory_block(
                            &section.name,
                            section.virtual_address,
                            section.virtual_size,
                            section.file_offset,
                            section.file_size,
                            section.is_executable,
                            section.is_writable,
                        ) {
                            crate::core::logging::warn(&format!(
                                "[decomp-worker] Failed to add section {}: {}",
                                section.name, e
                            ));
                        }
                    }

                    native.add_symbols(&request.iat_symbols);
                    native.add_global_symbols(&request.global_symbols);
                    native.set_symbol_provider(
                        &request.functions,
                        &request.global_symbols,
                        &request.sections,
                    );
                    if let Some(ref gdt) = request.gdt_json_path {
                        let _ = native.set_gdt(gdt);
                    }
                    crate::core::logging::info("[decomp-worker] Binary loaded via native FFI");
                }
            }
            continue;
        }

        // Handle CFG Analysis Request
        if request.is_cfg_request {
            let native_guard = match native_decomp.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };

            if let Some(ref native) = *native_guard {
                crate::core::logging::info(&format!(
                    "[decomp-worker-{}] Processing CFG request for 0x{:x}",
                    worker_id, request.address
                ));

                match native.get_pcode(request.address) {
                    Ok(pcode_json) => {
                        match fission_analysis::analysis::pcode::PcodeFunction::from_json(
                            &pcode_json,
                        ) {
                            Ok(func) => {
                                match fission_analysis::analysis::cfg::CfgAnalysis::from_pcode(
                                    &func,
                                ) {
                                    Ok(analysis) => {
                                        use crate::ui::gui::core::messages::{
                                            CfgBlockData, CfgLoopData,
                                        };
                                        use fission_analysis::analysis::cfg::{
                                            CfgVisualizer, DotOptions,
                                        };

                                        let loops: Vec<CfgLoopData> = analysis
                                            .loops
                                            .iter()
                                            .map(|l| CfgLoopData {
                                                header: l.header,
                                                kind: format!("{:?}", l.kind),
                                                body: l.body.iter().copied().collect(),
                                            })
                                            .collect();

                                        let blocks: Vec<CfgBlockData> = analysis
                                            .cfg
                                            .blocks
                                            .iter()
                                            .map(|b| CfgBlockData {
                                                index: b.index,
                                                address: format!("0x{:x}", b.start_address),
                                                is_entry: b.is_entry,
                                                is_exit: b.is_exit,
                                                successors: b
                                                    .successors
                                                    .iter()
                                                    .map(|e| e.target)
                                                    .collect(),
                                            })
                                            .collect();

                                        let dot_options = DotOptions::default();
                                        let dot_content = CfgVisualizer::to_dot(
                                            &analysis.cfg,
                                            &analysis.loops,
                                            &dot_options,
                                        );

                                        let _ = result_tx.send(AsyncMessage::CfgAnalysisResult {
                                            address: request.address,
                                            block_count: analysis.cfg.block_count(),
                                            edge_count: analysis.cfg.edge_count(),
                                            cyclomatic_complexity: analysis
                                                .metrics
                                                .cyclomatic_complexity,
                                            max_nesting_depth: analysis.metrics.max_nesting_depth,
                                            loops,
                                            blocks,
                                            dot_content,
                                        });

                                        crate::core::logging::info(&format!(
                                            "[decomp-worker-{}] CFG analysis complete for 0x{:x}",
                                            worker_id, request.address
                                        ));
                                    }
                                    Err(e) => {
                                        let _ = result_tx.send(AsyncMessage::CfgAnalysisError {
                                            address: request.address,
                                            error: format!("CFG build failed: {}", e),
                                        });
                                    }
                                }
                            }
                            Err(e) => {
                                let _ = result_tx.send(AsyncMessage::CfgAnalysisError {
                                    address: request.address,
                                    error: format!("Pcode parse failed: {}", e),
                                });
                            }
                        }
                    }
                    Err(e) => {
                        let _ = result_tx.send(AsyncMessage::CfgAnalysisError {
                            address: request.address,
                            error: format!("Failed to get Pcode: {}", e),
                        });
                    }
                }
            } else {
                let _ = result_tx.send(AsyncMessage::CfgAnalysisError {
                    address: request.address,
                    error: "Decompiler not initialized".to_string(),
                });
            }
            continue;
        }

        // Handle Decompile Request
        let start = std::time::Instant::now();

        // Check if this is the latest request
        let current_request_id = latest_request_id.load(Ordering::SeqCst);
        if request.request_id != current_request_id {
            crate::core::logging::debug(&format!(
                "[decomp-worker-{}] Skipping outdated request {}",
                worker_id, request.request_id
            ));
            continue;
        }

        // Decompile using FFI
        let native_guard = match native_decomp.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };

        if let Some(ref native) = *native_guard {
            match native.decompile(request.address) {
                Ok(c_code) => {
                    crate::core::logging::debug(&format!(
                        "[decomp-worker-{}] Decompiled 0x{:x} ({:?})",
                        worker_id,
                        request.address,
                        start.elapsed()
                    ));

                    let _ = result_tx.send(AsyncMessage::DecompileResult {
                        address: request.address,
                        c_code,
                    });
                }
                Err(e) => {
                    // Log as debug to avoid spam when binary isn't loaded
                    crate::core::logging::debug(&format!(
                        "[decomp-worker-{}] Decompile failed for 0x{:x}: {}",
                        worker_id, request.address, e
                    ));

                    // Provide more helpful error messages based on error type
                    let error_message = e.to_string();
                    let formatted_error = if error_message.contains("inline function") {
                        format!(
                            "// Cannot decompile function at 0x{:x}\n\
                             // Reason: This function is marked for inlining\n\
                             //\n\
                             // Inline functions are typically:\n\
                             // - Compiler-generated stubs\n\
                             // - Very small helper functions\n\
                             // - Functions that have been optimized away\n\
                             //\n\
                             // This is a known limitation of Ghidra's decompiler.",
                            request.address
                        )
                    } else if error_message.contains("recursive decompilation") {
                        format!(
                            "// Cannot decompile function at 0x{:x}\n\
                             // Reason: Function is already being decompiled\n\
                             //\n\
                             // This usually indicates:\n\
                             // - Circular function references\n\
                             // - Decompiler is still processing this function\n\
                             //\n\
                             // Try decompiling a different function first.",
                            request.address
                        )
                    } else if error_message.contains("No binary loaded") {
                        format!(
                            "// No binary loaded\n\
                             //\n\
                             // Please:\n\
                             // 1. Use File → Open to load a binary\n\
                             // 2. Wait for initialization to complete\n\
                             // 3. Try decompiling again"
                        )
                    } else {
                        format!(
                            "// Decompilation failed at 0x{:x}\n\
                             // Error: {}\n\
                             //\n\
                             // Possible causes:\n\
                             // - Invalid function address\n\
                             // - Corrupted or obfuscated code\n\
                             // - Unsupported code pattern\n\
                             //\n\
                             // Try:\n\
                             // 1. Verify the function address is correct\n\
                             // 2. Check if this is actually executable code\n\
                             // 3. Try disassembly view for more details",
                            request.address, error_message
                        )
                    };

                    let _ = result_tx.send(AsyncMessage::DecompileResult {
                        address: request.address,
                        c_code: formatted_error,
                    });
                }
            }
        } else {
            let _ = result_tx.send(AsyncMessage::DecompileResult {
                address: request.address,
                c_code: "// Native decompiler not initialized".to_string(),
            });
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
