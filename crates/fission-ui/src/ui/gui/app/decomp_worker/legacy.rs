#![cfg(all(feature = "native_decomp", feature = "legacy_single_worker"))]

use crate::ui::gui::core::messages::AsyncMessage;
use crossbeam_channel::{Receiver, Sender};
use fission_core::config::CONFIG;
use fission_loader::detect_pe_is_64bit;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use super::{DecompileTask, LoadBinaryRequest, WorkerRequest};

fn worker_loop_native(
    worker_id: usize,
    request_rx: Arc<Mutex<Receiver<WorkerRequest>>>,
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
                let mut decomp_guard = match native_decomp.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => poisoned.into_inner(),
                };
                if let Some(ref mut decomp) = *decomp_guard {
                    decomp.clear_cache();
                    crate::core::logging::info(
                        "[decomp-worker] Persistent decompiler cache cleared",
                    );
                }
            }
            WorkerRequest::LoadBinary(req) => {
                _handle_binary_load_native(&req, &native_decomp, &result_tx);
            }
            WorkerRequest::Decompile(req) => {
                if req.request_id != latest_request_id.load(Ordering::SeqCst) {
                    continue;
                }
                _handle_decompile_native(&req, &native_decomp, &result_tx);
            }
        }
    }
}

fn _handle_binary_load_native(
    request: &LoadBinaryRequest,
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

fn _handle_decompile_native(
    request: &DecompileTask,
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
