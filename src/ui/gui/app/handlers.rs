//! Message and command handlers.

use crossbeam_channel::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use crate::ui::gui::messages::AsyncMessage;
use crate::ui::gui::state::AppState;

use super::debug_ops;
use super::decompiler;
use super::file_ops;

/// Process pending async messages from background threads
pub fn process_messages(
    state: &mut AppState,
    rx: &Receiver<AsyncMessage>,
    tx: &Sender<AsyncMessage>,
    decomp_tx: &Sender<super::decomp_worker::DecompileRequest>,
    #[cfg(target_os = "windows")] dbg_event_rx: &mut Option<
        crossbeam_channel::Receiver<crate::debug::types::DebugEvent>,
    >,
) {
    while let Ok(msg) = rx.try_recv() {
        match msg {
            AsyncMessage::BinaryLoaded(Ok(binary)) => {
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

                state.analysis.loaded_binary = Some(binary.clone()); // Use clone for local reference if needed, but Arc is cheap

                // Run CRT signature matching on known functions
                let sig_db = crate::analysis::signatures::SignatureDatabase::new();
                let func_addrs: Vec<(u64, String)> = binary
                    .functions
                    .iter()
                    .map(|f| (f.address, f.name.clone()))
                    .collect();
                let matched_sigs = sig_db.identify_functions_in_binary(
                    &binary.data,
                    &func_addrs,
                    binary.image_base,
                );
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
                let (gdt_path, gdt_json_path) = if binary.is_64bit {
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
                // Pass RAW binary data - C++ will use sections to map VAs to file offsets
                state.log(format!(
                    "[*] IAT symbols extracted: {} entries",
                    binary.iat_symbols.len()
                ));
                
                state.log(format!(
                    "[*] Binary data: {} bytes (image_base: 0x{:x})",
                    binary.data.len(),
                    binary.image_base
                ));
                
                let request = super::decomp_worker::DecompileRequest::load_binary(
                    binary.data.clone(),
                    binary.image_base,
                    combined_symbols,
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
            AsyncMessage::BinaryLoaded(Err(e)) => {
                state.log(format!("[✗] Failed to load binary: {}", e));
                state.log("    → Ensure the file is a valid PE/ELF/Mach-O executable".to_string());
            }
            AsyncMessage::DecompileResult { address, c_code } => {
                decompiler::cache_decompile_result(state, address, c_code.clone());
                state.log(format!("[✓] Decompiled 0x{:x} (cached)", address));
            }
            AsyncMessage::DecompileError { address: _, error } => {
                state.analysis.decompiled_code = format!("// Decompilation failed\n// Error: {}\n\n// Possible causes:\n// - Function may not exist at this address\n// - fission_decomp CLI may not be built\n// - Try running: cd ghidra_decompiler/build && cmake .. && make", error);
                state.analysis.decompiling = false;
                state.log(format!("[✗] Decompile error: {}", error));
                state.log(
                    "    → Check if ghidra_decompiler/build/fission_decomp exists".to_string(),
                );
            }
            AsyncMessage::FileSelected(Some(path)) => {
                file_ops::load_binary(state, tx.clone(), &path);
            }
            AsyncMessage::FileSelected(None) => {
                // User cancelled
            }
            AsyncMessage::DebugEvent(evt) => {
                debug_ops::handle_debug_event(state, evt);
            }
            AsyncMessage::Event(evt) => {
                match evt {
                    crate::core::events::FissionEvent::LogMessage {
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
                    crate::core::events::FissionEvent::Progress {
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
                    crate::core::events::FissionEvent::SelectionChanged { address } => {
                        if let Some(addr) = address {
                            state.log(format!("[Selection] 0x{:08X}", addr));
                            state.ui.selected_xref_addr = Some(addr);
                        }
                    }
                    _ => {} // Ignore others for now (or handle specifically)
                }
            }
            AsyncMessage::SaveSnapshot(path) => {
                if let Some(binary) = &state.analysis.loaded_binary {
                    if let Err(e) =
                        crate::core::snapshot::save_snapshot(binary, std::path::Path::new(&path))
                    {
                        state.log(format!("[!] Error saving snapshot: {}", e));
                    } else {
                        state.log(format!("[✓] Snapshot saved to: {}", path));
                    }
                } else {
                    state.log("[!] No binary loaded to save");
                }
            }
            AsyncMessage::LoadSnapshot(path) => {
                match crate::core::snapshot::load_snapshot(std::path::Path::new(&path)) {
                    Ok(binary) => {
                        state.log(format!("[✓] Snapshot loaded from: {}", path));
                        let _ = tx.send(AsyncMessage::BinaryLoaded(Ok(Arc::new(binary))));
                    }
                    Err(e) => {
                        state.log(format!("[!] Error loading snapshot: {}", e));
                    }
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let mut pending = Vec::new();
        if let Some(rx) = dbg_event_rx {
            while let Ok(evt) = rx.try_recv() {
                pending.push(evt);
            }
        }
        for evt in pending {
            debug_ops::handle_debug_event(state, evt);
        }
    }
}

/// Process a CLI command
pub fn process_command(state: &mut AppState, tx: Sender<AsyncMessage>, cmd: &str) {
    match cmd {
        "help" | "?" => {
            state.log("Available commands:");
            state.log("  load <path>  : Load a binary for analysis");
            state.log("  funcs        : List functions");
            state.log("  clear        : Clear console");
            state.log("  exit         : Quit Fission");
        }
        "funcs" | "functions" => {
            if let Some(ref binary) = state.analysis.loaded_binary {
                let funcs: Vec<_> = binary
                    .functions
                    .iter()
                    .map(|f| (f.address, f.name.clone()))
                    .collect();
                state.log(format!("[*] {} functions:", funcs.len()));
                for (addr, name) in funcs {
                    state.log(format!("  0x{:08x} {}", addr, name));
                }
            } else {
                state.log("[!] No binary loaded");
            }
        }
        "clear" => {
            state.clear_logs();
            state.log("[*] Console cleared");
        }
        "exit" | "quit" => {
            std::process::exit(0);
        }
        "undo" => {
            let mut mgr = std::mem::take(&mut state.command_manager);
            match mgr.undo(state) {
                Ok(msg) => state.log(format!("[✓] {}", msg)),
                Err(e) => state.log(format!("[!] Undo failed: {}", e)),
            }
            state.command_manager = mgr;
        }
        "redo" => {
            let mut mgr = std::mem::take(&mut state.command_manager);
            match mgr.redo(state) {
                Ok(msg) => state.log(format!("[✓] {}", msg)),
                Err(e) => state.log(format!("[!] Redo failed: {}", e)),
            }
            state.command_manager = mgr;
        }
        _ if cmd.starts_with("plugin load ") => {
            let path = cmd.trim_start_matches("plugin load ").trim();
            let result = if let Ok(mut mgr) = state.plugin_manager().write() {
                match mgr.load_plugin(path) {
                    Ok(id) => Some(Ok(id)),
                    Err(e) => Some(Err(e)),
                }
            } else {
                None
            };

            match result {
                Some(Ok(id)) => state.log(format!("[✓] Plugin loaded: {}", id)),
                Some(Err(e)) => state.log(format!("[!] Failed to load plugin: {}", e)),
                None => state.log("[!] Failed to lock plugin manager"),
            }
        }
        _ if cmd.starts_with("plugin list") => {
            let plugins = if let Ok(mgr) = state.plugin_manager().read() {
                let mut p: Vec<_> = mgr.list_plugins().into_iter().cloned().collect();
                p.sort_by_key(|p| p.id.clone());
                p
            } else {
                Vec::new()
            };

            state.log("[*] Loaded Plugins:");

            if plugins.is_empty() {
                state.log("    (none)");
            } else {
                for plugin in plugins {
                    state.log(format!(
                        "    - {} ({}) v{} [{}]",
                        plugin.name,
                        plugin.id,
                        plugin.version,
                        if plugin.enabled {
                            "Enabled"
                        } else {
                            "Disabled"
                        }
                    ));
                }
            }
        }
        _ if cmd.starts_with("patch ") => {
            // patch <addr> <byte1> <byte2> ...
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            if parts.len() < 3 {
                state.log("[!] Usage: patch <address> <hex_byte1> [hex_byte2 ...]");
            } else {
                let addr_str = parts[1].trim_start_matches("0x");
                match u64::from_str_radix(addr_str, 16) {
                    Ok(addr) => {
                        let mut bytes = Vec::new();
                        let mut valid = true;

                        for s in &parts[2..] {
                            match u8::from_str_radix(s, 16) {
                                Ok(b) => bytes.push(b),
                                Err(_) => {
                                    state.log(format!("[!] Invalid byte: {}", s));
                                    valid = false;
                                    break;
                                }
                            }
                        }

                        if valid {
                            let command = Box::new(crate::ui::gui::commands::PatchBytesCommand {
                                address: addr,
                                old_bytes: Vec::new(),
                                new_bytes: bytes,
                            });

                            let mut mgr = std::mem::take(&mut state.command_manager);
                            if let Err(e) = mgr.execute(command, state) {
                                state.log(format!("[!] Patch failed: {}", e));
                            }
                            state.command_manager = mgr;
                        }
                    }
                    Err(_) => state.log(format!("[!] Invalid address: {}", parts[1])),
                }
            }
        }
        _ if cmd.starts_with("rename ") => {
            // rename <addr> <new_name>
            let parts: Vec<&str> = cmd.split_whitespace().collect();
            if parts.len() != 3 {
                state.log("[!] Usage: rename <address> <new_name>");
            } else {
                let addr_str = parts[1].trim_start_matches("0x");
                match u64::from_str_radix(addr_str, 16) {
                    Ok(addr) => {
                        let new_name = parts[2].to_string();
                        let command = Box::new(crate::ui::gui::commands::RenameFunctionCommand {
                            address: addr,
                            old_name: String::new(), // Will be filled by execute
                            new_name,
                        });

                        let mut mgr = std::mem::take(&mut state.command_manager);
                        if let Err(e) = mgr.execute(command, state) {
                            state.log(format!("[!] Rename failed: {}", e));
                        }
                        state.command_manager = mgr;
                    }
                    Err(_) => state.log(format!("[!] Invalid address: {}", parts[1])),
                }
            }
        }
        _ if cmd.starts_with("load ") => {
            let path = cmd.trim_start_matches("load ").trim();
            file_ops::load_binary(state, tx, path);
        }
        _ => {
            state.log(format!("[!] Unknown command: {}", cmd));
        }
    }
}
