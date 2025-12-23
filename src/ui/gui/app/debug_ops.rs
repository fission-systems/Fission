//! Debug operations - Process attach/detach, debug actions, breakpoints.

use eframe::egui;
use crate::ui::gui::state::{AppState, DebugAction, DebugBpAction};

#[cfg(target_os = "windows")]
use crate::debug::PlatformDebugger;
#[cfg(target_os = "windows")]
use crate::debug::Debugger;

/// Handle a debug event from the event loop
pub fn handle_debug_event(state: &mut AppState, evt: crate::debug::types::DebugEvent) {
    use crate::debug::types::DebugEvent::*;
    match evt {
        ProcessCreated { pid, main_thread_id } => {
            state.debug.debug_state.attached_pid = Some(pid);
            state.debug.debug_state.main_thread_id = Some(main_thread_id);
            state.debug.debug_state.last_thread_id = Some(main_thread_id);
            state.debug.debug_state.status = crate::debug::types::DebugStatus::Running;
            state.log(format!("[*] Process created pid={} tid={}", pid, main_thread_id));
        }
        ProcessExited { exit_code } => {
            state.debug.debug_state.status = crate::debug::types::DebugStatus::Terminated;
            state.log(format!("[*] Process exited code={}", exit_code));
        }
        ThreadCreated { thread_id } => {
            state.log(format!("[*] Thread created tid={}", thread_id));
        }
        ThreadExited { thread_id } => {
            state.log(format!("[*] Thread exited tid={}", thread_id));
        }
        DllLoaded { base_address, name } => {
            state.log(format!("[*] DLL loaded {name} @0x{base_address:016x}"));
        }
        BreakpointHit { address, thread_id } => {
            state.debug.debug_state.status = crate::debug::types::DebugStatus::Suspended;
            state.debug.debug_state.last_thread_id = Some(thread_id);
            state.debug.debug_state.last_event = Some(format!("BP hit 0x{address:016x} tid={thread_id}"));
            state.log(state.debug.debug_state.last_event.clone().unwrap_or_default());
        }
        SingleStep { thread_id } => {
            state.debug.debug_state.status = crate::debug::types::DebugStatus::Suspended;
            state.debug.debug_state.last_thread_id = Some(thread_id);
            state.debug.debug_state.last_event = Some(format!("[*] Single step tid={}", thread_id));
            state.log(state.debug.debug_state.last_event.clone().unwrap_or_default());
        }
        Exception { code, address, first_chance, .. } => {
            state.debug.debug_state.status = crate::debug::types::DebugStatus::Suspended;
            state.debug.debug_state.last_event = Some(format!(
                "[!] Exception code=0x{:x} addr=0x{:016x} first_chance={}",
                code, address, first_chance
            ));
            state.log(state.debug.debug_state.last_event.clone().unwrap_or_default());
        }
    }
}

/// Attach to a process (Windows builds only)
#[cfg(target_os = "windows")]
pub fn attach_to_process(
    state: &mut AppState,
    debugger: &mut Option<PlatformDebugger>,
    dbg_event_rx: &mut Option<std::sync::mpsc::Receiver<crate::debug::types::DebugEvent>>,
    dbg_stop_tx: &mut Option<std::sync::mpsc::Sender<()>>,
    pid: u32,
) {
    let dbg = debugger.get_or_insert_with(PlatformDebugger::default);
    state.log(format!("[*] Attaching to PID {}...", pid));
    match dbg.attach(pid) {
        Ok(_) => {
            state.debug.is_debugging = true;
            state.debug.debug_state = dbg.state().clone();
            state.log(format!("[✓] Attached to PID {}", pid));

            // Start event loop
            let (tx_evt, rx_evt) = std::sync::mpsc::channel();
            let (tx_stop, rx_stop) = std::sync::mpsc::channel();
            *dbg_event_rx = Some(rx_evt);
            *dbg_stop_tx = Some(tx_stop);
            crate::debug::windows::start_event_loop(pid, tx_evt, rx_stop);
        }
        Err(e) => {
            state.debug.is_debugging = false;
            state.log(format!("[✗] Attach failed: {}", e));
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn attach_to_process(state: &mut AppState, _pid: u32) {
    state.log("[!] Debug attach is only supported on Windows builds right now.");
}

/// Detach from the current process (Windows builds only)
#[cfg(target_os = "windows")]
pub fn detach_process(
    state: &mut AppState,
    debugger: &mut Option<PlatformDebugger>,
    dbg_stop_tx: &mut Option<std::sync::mpsc::Sender<()>>,
) {
    if let Some(dbg) = debugger.as_mut() {
        if let Some(pid) = dbg.attached_pid() {
            state.log(format!("[*] Detaching from PID {}...", pid));
        } else {
            state.log("[!] Not attached to any process");
            return;
        }

        match dbg.detach() {
            Ok(_) => {
                state.debug.is_debugging = false;
                state.debug.debug_state = dbg.state().clone();
                state.ui.show_attach_dialog = false;
                state.log("[*] Detached from process");
                if let Some(stop) = dbg_stop_tx.take() {
                    let _ = stop.send(());
                }
            }
            Err(e) => {
                state.log(format!("[✗] Detach failed: {}", e));
            }
        }
    } else {
        state.log("[!] Debugger not initialized");
    }
}

#[cfg(not(target_os = "windows"))]
pub fn detach_process(state: &mut AppState) {
    state.log("[!] Debug detach is only supported on Windows builds right now.");
}

/// Handle debug control actions (Windows only)
#[cfg(target_os = "windows")]
pub fn handle_debug_action(
    state: &mut AppState,
    debugger: &mut Option<PlatformDebugger>,
    action: DebugAction,
) {
    if !state.ui.dynamic_mode {
        state.log("[!] Debug control is disabled in static mode");
        return;
    }
    if let Some(dbg) = debugger.as_mut() {
        let result = match action {
            DebugAction::Continue => dbg.continue_execution(),
            DebugAction::Step => dbg.single_step(),
        };
        if let Err(e) = result {
            state.log(format!("[✗] Debug action failed: {}", e));
        } else {
            state.log("[*] Debug action sent");
        }
    } else {
        state.log("[!] Debugger not initialized");
    }
}

#[cfg(not(target_os = "windows"))]
pub fn handle_debug_action(state: &mut AppState, _action: DebugAction) {
    state.log("[!] Debug control is only supported on Windows builds right now.");
}

/// Handle breakpoint actions (Windows only)
#[cfg(target_os = "windows")]
pub fn handle_bp_action(
    state: &mut AppState,
    debugger: &mut Option<PlatformDebugger>,
    action: DebugBpAction,
) {
    if !state.ui.dynamic_mode {
        state.log("[!] Breakpoints are disabled in static mode");
        return;
    }
    if let Some(dbg) = debugger.as_mut() {
        let result = match action {
            DebugBpAction::Add(addr) => dbg.set_sw_breakpoint(addr),
            DebugBpAction::Remove(addr) => dbg.remove_sw_breakpoint(addr),
        };
        match result {
            Ok(_) => state.log("[*] Breakpoint action applied"),
            Err(e) => state.log(format!("[✗] Breakpoint action failed: {}", e)),
        }
    } else {
        state.log("[!] Debugger not initialized");
    }
}

#[cfg(not(target_os = "windows"))]
pub fn handle_bp_action(state: &mut AppState, _action: DebugBpAction) {
    state.log("[!] Breakpoints are only supported on Windows builds right now.");
}

/// Render "Attach to Process" dialog - returns selected process info for binary loading
pub fn render_attach_dialog(state: &mut AppState, ctx: &egui::Context) -> Option<crate::debug::types::ProcessInfo> {
    use crate::ui::gui::theme::catppuccin;
    
    if !state.ui.show_attach_dialog {
        return None;
    }

    let mut open = state.ui.show_attach_dialog;
    let mut selected_process = None;

    egui::Window::new("🔗 Attach to Process")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(550.0)
        .default_height(400.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            // Controls bar
            ui.horizontal(|ui| {
                if ui.button("🔄 Refresh").clicked() {
                    state.debug.process_list = crate::debug::enumerate_processes();
                }
                
                ui.separator();
                
                // Search filter
                ui.label("🔍");
                ui.add(
                    egui::TextEdit::singleline(&mut state.debug.process_filter)
                        .desired_width(150.0)
                        .hint_text("Filter...")
                );
                
                ui.separator();
                
                ui.label(egui::RichText::new(format!("{} processes", state.debug.process_list.len()))
                    .color(catppuccin::SUBTEXT0).small());
            });
            
            ui.separator();
            
            // Process list with fixed columns
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    egui::Grid::new("process_list_grid")
                        .striped(true)
                        .num_columns(4)
                        .min_col_width(0.0)
                        .show(ui, |ui| {
                            // Header
                            ui.label(egui::RichText::new("PID").strong().color(catppuccin::SUBTEXT1));
                            ui.label(egui::RichText::new("Name").strong().color(catppuccin::SUBTEXT1));
                            ui.label(egui::RichText::new("Path").strong().color(catppuccin::SUBTEXT1));
                            ui.label("");
                            ui.end_row();
                            
                            let filter = state.debug.process_filter.to_lowercase();
                            
                            for process in &state.debug.process_list {
                                // Apply filter
                                if !filter.is_empty() {
                                    let matches_name = process.name.to_lowercase().contains(&filter);
                                    let matches_pid = process.pid.to_string().contains(&filter);
                                    let matches_path = process.exe_path.as_ref()
                                        .map_or(false, |p| p.to_lowercase().contains(&filter));
                                    
                                    if !matches_name && !matches_pid && !matches_path {
                                        continue;
                                    }
                                }
                                
                                ui.push_id(process.pid, |ui| {
                                    // PID (fixed width)
                                    ui.add_sized([60.0, 18.0], 
                                        egui::Label::new(egui::RichText::new(format!("{}", process.pid))
                                            .monospace().color(catppuccin::SAPPHIRE)));
                                    
                                    // Name (fixed width)
                                    let name_display = if process.name.len() > 25 {
                                        format!("{}...", &process.name[..22])
                                    } else {
                                        process.name.clone()
                                    };
                                    ui.add_sized([180.0, 18.0],
                                        egui::Label::new(egui::RichText::new(name_display)
                                            .color(catppuccin::TEXT)));
                                    
                                    // Path (truncated, tooltip shows full)
                                    let path_display = if let Some(ref path) = process.exe_path {
                                        // Show just filename or last part
                                        let short = std::path::Path::new(path)
                                            .file_name()
                                            .and_then(|s| s.to_str())
                                            .unwrap_or(path);
                                        short.to_string()
                                    } else {
                                        "—".to_string()
                                    };
                                    let path_label = ui.add_sized([200.0, 18.0],
                                        egui::Label::new(egui::RichText::new(&path_display)
                                            .color(catppuccin::OVERLAY1).small()));
                                    if let Some(ref path) = process.exe_path {
                                        path_label.on_hover_text(path);
                                    }
                                    
                                    // Attach button
                                    if ui.add_sized([60.0, 18.0],
                                        egui::Button::new(egui::RichText::new("Attach")
                                            .color(catppuccin::GREEN))
                                    ).clicked() {
                                        selected_process = Some(process.clone());
                                    }
                                    
                                    ui.end_row();
                                });
                            }
                        });
                });
                
            ui.separator();
            
            // Hint
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("💡 Tip: Attach will also load the binary for static analysis")
                    .color(catppuccin::OVERLAY0).small().italics());
            });
        });

    state.ui.show_attach_dialog = open;
    selected_process
}

