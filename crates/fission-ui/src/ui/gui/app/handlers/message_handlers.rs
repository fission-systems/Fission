//! Individual message handlers for AsyncMessage variants.

use crossbeam_channel::Sender;
use std::sync::Arc;

use crate::analysis::loader::LoadedBinary;
use crate::ui::gui::messages::AsyncMessage;
use crate::ui::gui::state::AppState;

use super::super::decomp_worker;

/// Handle successful binary load
pub fn handle_binary_loaded(
    state: &mut AppState,
    binary: Arc<LoadedBinary>,
    decomp_tx: &Sender<decomp_worker::DecompileRequest>,
) {
    // Note: Internal function discovery now disabled for fast loading
    // Can be triggered separately via "Analyze" button

    state.log(format!("[✓] Loaded: {}", binary.path));
    state.log(format!(
        "    {} {} | Entry: 0x{:x}",
        if binary.is_64bit { "64-bit" } else { "32-bit" },
        binary.format,
        binary.entry_point
    ));
    state.log(format!("    {} functions found", binary.functions.len()));

    // Run detection (DiE-style)
    let detection = crate::analysis::detector::detect(&binary);
    if !detection.detections.is_empty() {
        state.log("[*] Detection results:".to_string());
        for d in &detection.detections {
            state.log(format!(
                "    {} {} {}",
                match d.detection_type {
                    crate::analysis::DetectionType::Packer => "📦",
                    crate::analysis::DetectionType::Protector => "🛡️",
                    crate::analysis::DetectionType::Compiler => "🔧",
                    crate::analysis::DetectionType::Language => "💻",
                    crate::analysis::DetectionType::Library => "📚",
                    crate::analysis::DetectionType::Linker => "🔗",
                    crate::analysis::DetectionType::Installer => "📥",
                    crate::analysis::DetectionType::Sfx => "📁",
                },
                d.display(),
                if d.confidence == crate::analysis::Confidence::High {
                    "✓"
                } else {
                    ""
                }
            ));
        }
        state.analysis.detection_result = Some(detection);
    }

    // Build cross-references database
    let xref_db = crate::analysis::xrefs::XrefDatabase::build_from_binary(&binary);
    let xref_count = xref_db.total_refs();
    state.log(format!("[*] 🔗 Built {} cross-references", xref_count));
    state.analysis.xref_db = Some(xref_db);

    state.analysis.loaded_binary = Some(binary.clone());

    // Run CRT signature matching on known functions
    let sig_db = crate::analysis::signatures::SignatureDatabase::new();
    let func_addrs: Vec<(u64, String)> = binary
        .functions
        .iter()
        .map(|f| (f.address, f.name.clone()))
        .collect();
    let matched_sigs =
        sig_db.identify_functions_in_binary(&binary.data, &func_addrs, binary.image_base);
    if !matched_sigs.is_empty() {
        state.log(format!(
            "[*] CRT signatures matched: {} functions",
            matched_sigs.len()
        ));
    }

    // Merge IAT symbols with CRT signature matches
    let mut combined_symbols = binary.iat_symbols.clone();
    combined_symbols.extend(matched_sigs);

    // Determine GDT paths based on architecture
    let (gdt_path, _gdt_json_path) = if binary.is_64bit {
        (
            "ghidra/typeinfo/win32/windows_vs12_64.gdt",
            "ghidra/typeinfo/win32/windows_vs12_64.gdt.types.json",
        )
    } else {
        (
            "ghidra/typeinfo/win32/windows_vs12_32.gdt",
            "ghidra/typeinfo/win32/windows_vs12_32.gdt.types.json",
        )
    };

    // GDT parsing is now handled by C++ GdtBinaryParser directly
    let gdt_json_path_opt = if std::path::Path::new(gdt_path).exists() {
        Some(gdt_path.to_string())
    } else {
        None
    };

    // Trigger background binary load for decompiler context
    state.log(format!(
        "[*] IAT symbols extracted: {} entries",
        binary.iat_symbols.len()
    ));

    state.log(format!(
        "[*] Binary data: {} bytes (image_base: 0x{:x})",
        binary.data.len(),
        binary.image_base
    ));

    let request = decomp_worker::DecompileRequest::load_binary(
        binary.data.clone(),
        binary.image_base,
        combined_symbols,
        binary.global_symbols.clone(),
        binary.functions.clone(),
        gdt_json_path_opt,
        binary.sections.clone(),
    );
    if let Err(e) = decomp_tx.send(request) {
        state.log(format!(
            "[!] Failed to trigger decompiler binary load: {}",
            e
        ));
        state.analysis.decompiler_context_loaded = false;
    } else {
        state.log("[*] Initializing decompiler persistent context...");
        state.analysis.decompiler_context_loaded = true;
    }
}

/// Handle binary load error
pub fn handle_binary_load_error(state: &mut AppState, error: String) {
    state.log(format!("[✗] Failed to load binary: {}", error));
    state.log("    → Ensure the file is a valid PE/ELF/Mach-O executable".to_string());
}

/// Handle decompilation result
pub fn handle_decompile_result(state: &mut AppState, address: u64, c_code: String) {
    super::super::decompiler::cache_decompile_result(state, address, c_code);
    state.log(format!("[✓] Decompiled 0x{:x} (cached)", address));
}

/// Handle decompilation error
pub fn handle_decompile_error(state: &mut AppState, address: u64, error: String) {
    state.analysis.decompiled_code = format!("// Decompilation failed\n// Error: {}\n\n// Possible causes:\n// - Function may not exist at this address\n// - fission_decomp CLI may not be built\n// - Try running: cd ghidra_decompiler/build && cmake .. && make", error);
    state.analysis.decompiling = false;
    state.log(format!("[✗] Decompile error (0x{:x}): {}", address, error));
    state.log("    → Check if ghidra_decompiler/build/fission_decomp exists".to_string());
}

/// Handle file selection
pub fn handle_file_selected(state: &mut AppState, tx: Sender<AsyncMessage>, path: String) {
    super::super::file_ops::load_binary(state, tx, &path);
}

/// Handle debug event
pub fn handle_debug_event_wrapper(state: &mut AppState, evt: crate::debug::types::DebugEvent) {
    super::super::debug_ops::handle_debug_event(state, evt);
}

/// Handle Fission event
pub fn handle_fission_event(state: &mut AppState, evt: crate::app::events::FissionEvent) {
    match evt {
        crate::app::events::FissionEvent::LogMessage {
            level,
            message,
            target,
        } => {
            state.log(format!(
                "[{}] {} - {}",
                level.to_uppercase(),
                target,
                message
            ));
        }
        crate::app::events::FissionEvent::Progress {
            task_id: _,
            current,
            total,
            message,
        } => {
            let percentage = (current as f32 / total as f32).clamp(0.0, 1.0);
            state.ui.progress = Some((percentage, message.clone()));

            // Clear progress when done
            if current >= total {
                state.ui.progress = None;
            }
        }
        crate::app::events::FissionEvent::SelectionChanged { address: Some(addr) } => {
            state.log(format!("[Selection] 0x{:08X}", addr));
            state.ui.selected_xref_addr = Some(addr);
        }
        _ => {} // Ignore others for now
    }
}

/// Handle snapshot save
pub fn handle_save_snapshot(state: &mut AppState, path: String) {
    if let Some(binary) = &state.analysis.loaded_binary {
        if let Err(e) = crate::app::snapshot::save_snapshot(binary, std::path::Path::new(&path)) {
            state.log(format!("[!] Error saving snapshot: {}", e));
        } else {
            state.log(format!("[✓] Snapshot saved to: {}", path));
        }
    } else {
        state.log("[!] No binary loaded to save");
    }
}

/// Handle snapshot load
pub fn handle_load_snapshot(state: &mut AppState, tx: Sender<AsyncMessage>, path: String) {
    match crate::app::snapshot::load_snapshot(std::path::Path::new(&path)) {
        Ok(binary) => {
            state.log(format!("[✓] Snapshot loaded from: {}", path));
            let _ = tx.send(AsyncMessage::BinaryLoaded(Ok(Arc::new(binary))));
        }
        Err(e) => {
            state.log(format!("[!] Error loading snapshot: {}", e));
        }
    }
}
