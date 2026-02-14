//! Message and command handlers (routing layer).

use crossbeam_channel::{Receiver, Sender};

use crate::ui::gui::core::messages::AsyncMessage;
use crate::ui::gui::core::state::AppState;

mod command_handlers;
pub mod message_handlers;

/// Process pending async messages from background threads
pub fn process_messages(
    state: &mut AppState,
    rx: &Receiver<AsyncMessage>,
    tx: &Sender<AsyncMessage>,
    decomp_tx: &Sender<super::decomp_worker::WorkerRequest>,
    req_id: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    #[cfg(target_os = "windows")] dbg_event_rx: &mut Option<
        crossbeam_channel::Receiver<crate::debug::types::DebugEvent>,
    >,
) {
    while let Ok(msg) = rx.try_recv() {
        match msg {
            AsyncMessage::BinaryLoaded(Ok(binary)) => {
                message_handlers::handle_binary_loaded(state, binary, decomp_tx);
            }
            AsyncMessage::BinaryLoaded(Err(e)) => {
                message_handlers::handle_binary_load_error(state, e);
            }
            AsyncMessage::DecompileResult { address, c_code } => {
                message_handlers::handle_decompile_result(state, address, c_code);
            }
            AsyncMessage::DecompileError { address, error } => {
                message_handlers::handle_decompile_error(state, address, error);
            }
            AsyncMessage::FileSelected(Some(path)) => {
                message_handlers::handle_file_selected(state, tx.clone(), path);
            }
            AsyncMessage::FileSelected(None) => {
                // User cancelled
            }
            AsyncMessage::FolderSelected(Some(path)) => {
                message_handlers::handle_folder_selected(state, tx.clone(), path);
            }
            AsyncMessage::FolderSelected(None) => {
                // User cancelled
            }
            AsyncMessage::ProjectLoaded { path, binaries } => {
                message_handlers::handle_project_loaded(state, path, binaries, decomp_tx);
            }
            AsyncMessage::DebugEvent(evt) => {
                message_handlers::handle_debug_event_wrapper(state, evt);
            }
            AsyncMessage::Event(evt) => {
                message_handlers::handle_fission_event(state, evt);
            }
            AsyncMessage::SaveSnapshot(path) => {
                message_handlers::handle_save_snapshot(state, path);
            }
            AsyncMessage::LoadSnapshot(path) => {
                message_handlers::handle_load_snapshot(state, tx.clone(), path);
            }
            AsyncMessage::SaveProject(path) => {
                message_handlers::handle_save_project(state, path);
            }
            AsyncMessage::LoadProject(path) => {
                message_handlers::handle_load_project(state, path, decomp_tx, req_id);
            }
            AsyncMessage::DecompilerContextLoaded => {
                crate::core::logging::info("Decompiler context initialized");
            }

            AsyncMessage::CfgAnalysisRequest { address } => {
                // Send CFG request to worker
                let request =
                    super::decomp_worker::WorkerRequest::cfg_analysis(address, String::new());
                let _ = decomp_tx.send(request);
                state.log(format!("[*] CFG analysis started for 0x{:x}", address));
            }
            AsyncMessage::CfgAnalysisResult(summary) => {
                let addr = summary.function_address.clone();
                let blocks = summary.block_count;
                let edges = summary.edge_count;
                let complexity = summary.cyclomatic_complexity;

                state.analysis.domain.cfg_analysis = Some(summary);

                state.log(format!(
                    "[✓] CFG analysis complete for {}: {} blocks, {} edges, complexity: {}",
                    addr, blocks, edges, complexity
                ));
            }
            AsyncMessage::CfgAnalysisError { address, error } => {
                state.log(format!(
                    "[!] CFG analysis failed for 0x{:x}: {}",
                    address, error
                ));
            }
            AsyncMessage::DecompilerContextError { error, suggestion } => {
                state.log(format!("[!] Decompiler Error: {}", error));
                if let Some(fix) = suggestion {
                    state.log(format!("[?] Suggestion: {}", fix));
                }
                // Mark decompiler as not loaded so user can retry
                state.analysis.domain.decompiler_context_loaded = false;
            }
            AsyncMessage::WorkerHeartbeat {
                worker_id,
                is_alive,
            } => {
                if !is_alive {
                    state.log(format!(
                        "[!] Decompiler worker {} is not responding",
                        worker_id
                    ));
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
            message_handlers::handle_debug_event_wrapper(state, evt);
        }
    }
}

/// Process a CLI command
pub fn process_command(state: &mut AppState, tx: Sender<AsyncMessage>, cmd: &str) {
    match cmd {
        "help" | "?" => command_handlers::handle_help(state),
        "funcs" | "functions" => command_handlers::handle_list_functions(state),
        "clear" => command_handlers::handle_clear(state),
        "exit" | "quit" => command_handlers::handle_exit(),
        "undo" => command_handlers::handle_undo(state),
        "redo" => command_handlers::handle_redo(state),
        _ if cmd.starts_with("plugin load ") => {
            let path = cmd.trim_start_matches("plugin load ").trim();
            command_handlers::handle_plugin_load(state, path);
        }
        _ if cmd.starts_with("plugin list") => {
            command_handlers::handle_plugin_list(state);
        }
        _ if cmd.starts_with("patch ") => {
            command_handlers::handle_patch(state, cmd);
        }
        _ if cmd.starts_with("rename ") => {
            command_handlers::handle_rename(state, cmd);
        }
        _ if cmd.starts_with("load ") => {
            let path = cmd.trim_start_matches("load ").trim();
            command_handlers::handle_load(state, tx, path);
        }
        _ => {
            command_handlers::handle_unknown(state, cmd);
        }
    }
}
