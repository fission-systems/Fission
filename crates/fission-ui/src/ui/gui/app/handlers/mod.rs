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
    decomp_tx: &Sender<super::decomp_worker::DecompileRequest>,
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
            AsyncMessage::CfgAnalysisRequest { address } => {
                // Send CFG request to worker
                let request = super::decomp_worker::DecompileRequest::cfg_analysis(address);
                let _ = decomp_tx.send(request);
                state.log(format!("[*] CFG analysis started for 0x{:x}", address));
            }
            AsyncMessage::CfgAnalysisResult {
                address,
                block_count,
                edge_count,
                cyclomatic_complexity,
                max_nesting_depth,
                loops,
                blocks,
                dot_content,
            } => {
                use crate::ui::gui::panels::bottom_tabs::cfg::{
                    BlockDisplayInfo, CfgAnalysisResult, LoopDisplayInfo,
                };

                let loops_display: Vec<LoopDisplayInfo> = loops
                    .into_iter()
                    .map(|l| LoopDisplayInfo {
                        header: l.header,
                        kind: l.kind,
                        body: l.body,
                    })
                    .collect();

                let blocks_display: Vec<BlockDisplayInfo> = blocks
                    .into_iter()
                    .map(|b| BlockDisplayInfo {
                        index: b.index,
                        address: b.address,
                        is_entry: b.is_entry,
                        is_exit: b.is_exit,
                        successors: b.successors,
                    })
                    .collect();

                state.analysis.cfg_analysis = Some(CfgAnalysisResult {
                    block_count,
                    edge_count,
                    cyclomatic_complexity,
                    max_nesting_depth,
                    loops: loops_display,
                    blocks: blocks_display,
                    dot_content,
                });

                state.log(format!(
                    "[✓] CFG analysis complete for 0x{:x}: {} blocks, {} edges, complexity: {}",
                    address, block_count, edge_count, cyclomatic_complexity
                ));
            }
            AsyncMessage::CfgAnalysisError { address, error } => {
                state.log(format!(
                    "[!] CFG analysis failed for 0x{:x}: {}",
                    address, error
                ));
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
