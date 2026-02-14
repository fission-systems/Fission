#![cfg(feature = "native_decomp")]

use crate::ui::gui::core::messages::AsyncMessage;
use crossbeam_channel::Sender;
use fission_core::config::CONFIG;
use fission_loader::detect_pe_is_64bit;

use super::{DecompileTask, LoadBinaryRequest};

/// Handle binary load for a specific worker
pub(crate) fn handle_binary_load_for_worker(
    request: &LoadBinaryRequest,
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
        fission_loader::loader::DataBuffer::Heap(request.bytes.clone()),
    )
    .image_base(request.image_base)
    .is_64bit(is_64bit)
    .format("PE")
    .arch_spec(request.arch_spec.clone())
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

            // Try to detect compiler
            let detection = fission_loader::detect(&dummy_binary);
            let is_pe = dummy_binary.format.to_ascii_uppercase().starts_with("PE");
            let compiler_id = detection
                .compiler()
                .map(|d| match d.name.to_lowercase().as_str() {
                    "microsoft visual c++" | "msvc" | "windows" => "windows",
                    "gcc" | "mingw" => {
                        if is_pe {
                            "windows"
                        } else {
                            "gcc"
                        }
                    }
                    "clang" => "clang",
                    _ => "default",
                });

            if let Err(e) = inner.load_binary(
                &request.bytes,
                request.image_base,
                is_64bit,
                Some(&request.arch_spec),
                compiler_id,
            ) {
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
pub(crate) fn handle_decompile_for_worker(
    request: &DecompileTask,
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
