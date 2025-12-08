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
            state.debug_state.attached_pid = Some(pid);
            state.debug_state.main_thread_id = Some(main_thread_id);
            state.debug_state.last_thread_id = Some(main_thread_id);
            state.debug_state.status = crate::debug::types::DebugStatus::Running;
            state.log(format!("[*] Process created pid={} tid={}", pid, main_thread_id));
        }
        ProcessExited { exit_code } => {
            state.debug_state.status = crate::debug::types::DebugStatus::Terminated;
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
            state.debug_state.status = crate::debug::types::DebugStatus::Suspended;
            state.debug_state.last_thread_id = Some(thread_id);
            state.debug_state.last_event = Some(format!("BP hit 0x{address:016x} tid={thread_id}"));
            state.log(state.debug_state.last_event.clone().unwrap_or_default());
        }
        SingleStep { thread_id } => {
            state.debug_state.status = crate::debug::types::DebugStatus::Suspended;
            state.debug_state.last_thread_id = Some(thread_id);
            state.debug_state.last_event = Some(format!("[*] Single step tid={}", thread_id));
            state.log(state.debug_state.last_event.clone().unwrap_or_default());
        }
        Exception { code, address, first_chance, .. } => {
            state.debug_state.status = crate::debug::types::DebugStatus::Suspended;
            state.debug_state.last_event = Some(format!(
                "[!] Exception code=0x{:x} addr=0x{:016x} first_chance={}",
                code, address, first_chance
            ));
            state.log(state.debug_state.last_event.clone().unwrap_or_default());
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
            state.is_debugging = true;
            state.debug_state = dbg.state().clone();
            state.log(format!("[✓] Attached to PID {}", pid));

            // Start event loop
            let (tx_evt, rx_evt) = std::sync::mpsc::channel();
            let (tx_stop, rx_stop) = std::sync::mpsc::channel();
            *dbg_event_rx = Some(rx_evt);
            *dbg_stop_tx = Some(tx_stop);
            crate::debug::windows::start_event_loop(pid, tx_evt, rx_stop);
        }
        Err(e) => {
            state.is_debugging = false;
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
                state.is_debugging = false;
                state.debug_state = dbg.state().clone();
                state.show_attach_dialog = false;
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
    if !state.dynamic_mode {
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
    if !state.dynamic_mode {
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

/// Render "Attach to Process" dialog
pub fn render_attach_dialog(state: &mut AppState, ctx: &egui::Context) -> Option<u32> {
    if !state.show_attach_dialog {
        return None;
    }

    let mut open = state.show_attach_dialog;
    let mut attached_pid = None;

    egui::Window::new("Attach to Process")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_width(400.0)
        .default_height(500.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("🔄 Refresh").clicked() {
                    state.process_list = crate::debug::enumerate_processes();
                }
                ui.label(format!("{} processes found", state.process_list.len()));
            });
            
            ui.separator();
            
            // Process list
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("process_list")
                    .striped(true)
                    .num_columns(3)
                    .show(ui, |ui| {
                        ui.strong("PID");
                        ui.strong("Name");
                        ui.strong("Action");
                        ui.end_row();

                        for process in &state.process_list {
                            ui.label(format!("{}", process.pid));
                            ui.label(&process.name);
                            if ui.button("Attach").clicked() {
                                attached_pid = Some(process.pid);
                            }
                            ui.end_row();
                        }
                    });
            });
        });

    state.show_attach_dialog = open;
    attached_pid
}

