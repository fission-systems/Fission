use crate::ui::gui::state::{AppState, DebugAction};

#[cfg(target_os = "windows")]
pub fn handle_debug_action(state: &mut AppState, action: DebugAction) -> bool {
    if !state.ui.dynamic_mode {
        return false;
    }

    let mut handled = false;
    match action {
        DebugAction::Dump => {
            if let Some(engine_lock) = &state.debug.titan_engine {
                if let Ok(engine) = engine_lock.read() {
                    // Check if we have reconstructed imports to fix
                    if !state.analysis.reconstructed_imports.is_empty() {
                        state.log("[*] Dumping with IAT Fix...".to_string());
                        if let Err(e) = engine
                            .dump_and_fix("dumped_fixed.exe", &state.analysis.reconstructed_imports)
                        {
                            state.log(format!("[Error] Dump & Fix failed: {}", e));
                        } else {
                            state.log(
                                "[✓] Process dumped and fixed to dumped_fixed.exe".to_string(),
                            );
                        }
                    } else {
                        state.log("[*] Dumping raw process (No IAT Fix)...".to_string());
                        if let Err(e) = engine.dump_process("dumped.exe") {
                            state.log(format!("[Error] Dump failed: {}", e));
                        } else {
                            state.log("[✓] Process dumped to dumped.exe".to_string());
                        }
                    }
                }
            }
            handled = true;
        }
        DebugAction::ImportRec => {
            state.log("[*] Starting Import Reconstruction...".to_string());

            if let Some(engine_lock) = &state.debug.titan_engine {
                // We need write access to update modules
                if let Ok(mut engine) = engine_lock.write() {
                    // 1. Update loaded modules
                    if let Some(importer) = &mut engine.importer {
                        if let Err(e) = importer.update_modules() {
                            state.log(format!("[Error] Failed to update modules: {}", e));
                        } else {
                            state.log("[*] Modules updated".to_string());

                            // 2. Try to find IAT (Auto-Search or Manual)
                            if let Some(proc) = &engine.active_process {
                                let base = proc.image_base;
                                // Read DOS & NT Headers to find Import Directory
                                if let Ok(dos) = crate::unpacker::pe::read_dos_header(
                                    proc.process_handle,
                                    base,
                                ) {
                                    if let Ok(nt) = crate::unpacker::pe::read_nt_headers64(
                                        proc.process_handle,
                                        base,
                                        dos.e_lfanew,
                                    ) {
                                        let import_rva =
                                            nt.OptionalHeader.DataDirectory[1].VirtualAddress;
                                        let import_size = nt.OptionalHeader.DataDirectory[1].Size;

                                        if import_rva != 0 {
                                            state.log(format!("[*] Found Import Directory at RVA: {:X}, Size: {:X}", import_rva, import_size));

                                            let iat_rva =
                                                nt.OptionalHeader.DataDirectory[12].VirtualAddress;
                                            let iat_size = nt.OptionalHeader.DataDirectory[12].Size;

                                            // Try Heuristic Search if standard IAT is empty or suspicious
                                            let mut use_heuristic = iat_rva == 0 && import_rva == 0;

                                            let mut imports = Vec::new();
                                            let mut success = false;

                                            if !use_heuristic {
                                                let target_rva =
                                                    if iat_rva != 0 { iat_rva } else { import_rva };
                                                let target_size = if iat_size != 0 {
                                                    iat_size
                                                } else {
                                                    import_size
                                                };

                                                state.log(format!("[*] Scanning Standard IAT at RVA: {:X} (Size: {:X})", target_rva, target_size));

                                                if let Ok(res) = importer.reconstruct_iat(
                                                    base + target_rva as u64,
                                                    target_size as usize,
                                                ) {
                                                    if !res.is_empty() {
                                                        imports = res;
                                                        success = true;
                                                    } else {
                                                        state.log("[!] Standard IAT scan returned 0 imports. Trying Heuristic...".to_string());
                                                        use_heuristic = true;
                                                    }
                                                } else {
                                                    use_heuristic = true;
                                                }
                                            }

                                            if use_heuristic {
                                                if let Ok(sections) =
                                                    crate::unpacker::pe::read_section_headers(
                                                        proc.process_handle,
                                                        base,
                                                        dos.e_lfanew,
                                                        nt.FileHeader.NumberOfSections,
                                                    )
                                                {
                                                    for section in sections {
                                                        let name =
                                                            String::from_utf8_lossy(&section.Name)
                                                                .trim_matches(char::from(0))
                                                                .to_string();
                                                        if name == ".rdata"
                                                            || name == ".idata"
                                                            || name == ".text"
                                                        {
                                                            let start = base
                                                                + section.VirtualAddress as u64;
                                                            let end =
                                                                start + section.VirtualSize as u64;
                                                            state.log(format!("[*] Heuristic scanning section {} ({:X}-{:X})...", name, start, end));

                                                            if let Ok((iat_start, iat_size)) =
                                                                importer
                                                                    .find_iat_heuristic(start, end)
                                                            {
                                                                state.log(format!("[*] Found potential IAT at {:X} (Size: {:X})", iat_start, iat_size));
                                                                if let Ok(res) = importer
                                                                    .reconstruct_iat(
                                                                        iat_start, iat_size,
                                                                    )
                                                                {
                                                                    if !res.is_empty() {
                                                                        imports.extend(res);
                                                                        success = true;
                                                                        break;
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            if success {
                                                state.log(format!(
                                                    "[✓] Reconstructed {} imports",
                                                    imports.len()
                                                ));
                                                state.analysis.reconstructed_imports = imports;
                                                state.ui.bottom_tab =
                                                    crate::ui::gui::state::BottomTab::Imports;
                                            } else {
                                                state.log("[Error] IAT Reconstruction failed (No imports found)".to_string());
                                            }
                                        } else {
                                            state.log(
                                                "[!] No Import Directory found in headers"
                                                    .to_string(),
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            handled = true;
        }
        _ => {}
    }
    handled
}

#[cfg(not(target_os = "windows"))]
pub fn handle_debug_action(_state: &mut AppState, _action: DebugAction) -> bool {
    false
}

#[cfg(target_os = "windows")]
pub fn attach(state: &mut AppState, pid: u32) -> bool {
    if !state.ui.dynamic_mode {
        return false;
    }

    if let Some(engine_lock) = &state.debug.titan_engine {
        if let Ok(mut engine) = engine_lock.write() {
            if let Err(e) = engine.attach(pid) {
                state.log(format!("[Error] TitanEngine Attach failed: {}", e));
            } else {
                state.log(format!("[*] TitanEngine Attached to PID {}", pid));
                state.debug.debug_state.attached_pid = Some(pid);
                state.debug.debug_state.status = crate::debug::types::DebugStatus::Suspended;
            }
        }
    }
    true
}

#[cfg(not(target_os = "windows"))]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub fn attach(_state: &mut AppState, _pid: u32) -> bool {
    false
}
