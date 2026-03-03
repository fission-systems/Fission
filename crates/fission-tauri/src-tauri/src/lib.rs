//! Fission Tauri — Library entry point.

mod commands;
mod dto;
pub(crate) mod error;
pub(crate) mod menu;
mod state;

use menu::ids;
use state::AppState;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            let handle = app.handle().clone();
            let (menu, handles) = menu::build_menu(&handle)?;
            app.set_menu(menu)?;
            // Store handles in AppState for dynamic enable/disable
            let state: tauri::State<'_, AppState> = handle.state::<AppState>();
            let _ = state.menu_handles.set(handles);
            Ok(())
        })
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                // DevTools — handle directly in Rust (no WebView round-trip needed)
                ids::TOGGLE_DEVTOOLS => {
                    if let Some(w) = app.get_webview_window("main") {
                        if w.is_devtools_open() {
                            w.close_devtools();
                        } else {
                            w.open_devtools();
                        }
                    }
                }
                // Everything else → forward to the WebView as a "menu-action" event
                _ => {
                    let _ = app.emit("menu-action", id.to_string());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::open_file,
            commands::get_functions,
            commands::decompile_function,
            commands::get_assembly,
            commands::get_strings,
            commands::get_binary_info,
            // Phase 1: new commands
            commands::get_imports,
            commands::get_exports,
            commands::get_sections,
            commands::rename_function,
            commands::add_comment,
            commands::get_comments,
            commands::toggle_bookmark,
            commands::get_bookmarks,
            commands::goto_address,
            // Phase 2: hex / search / xrefs
            commands::get_hex_view,
            commands::patch_bytes,
            commands::save_patched_binary,
            commands::search_binary,
            commands::get_xrefs,
            // Phase 1b: project save/load + settings
            commands::save_project,
            commands::load_project,
            commands::get_settings,
            commands::save_settings,
            commands::get_decompiler_options,
            commands::apply_decompiler_options,
            commands::clear_decompiler_cache,
            // Phase 2: CFG analysis
            commands::get_cfg,
            commands::export_cfg_dot,
            // Phase 3: Listing view
            commands::get_listing_info,
            commands::get_listing_chunk,
            // Phase 4: Debug
            commands::debug_get_state,
            commands::debug_attach,
            commands::debug_detach,
            commands::debug_continue,
            commands::debug_step,
            commands::debug_add_breakpoint,
            commands::debug_remove_breakpoint,
            // Phase 5: String XRefs
            commands::get_string_xrefs,
            // Phase 6: Analyze / Deep Scan
            commands::analyze_functions,
            commands::deep_scan_functions,
            // Plugin system
            commands::load_plugin,
            commands::unload_plugin,
            commands::list_plugins,
            commands::enable_plugin,
            commands::disable_plugin,
            // Snapshot
            commands::save_snapshot,
            commands::load_snapshot,
            // System utilities
            commands::get_git_branch,
            // DevTools
            commands::toggle_devtools,
            // Phase 3: Function Identification (FID)
            commands::run_fid,
            // Phase 8: Analysis JSON Export
            commands::export_analysis_json,
            // Phase 4: Debug Memory Dump
            commands::debug_read_memory,
            // Phase 5: TTD (Time Travel Debugging)
            commands::ttd_start,
            commands::ttd_stop,
            commands::ttd_status,
            commands::ttd_seek,
            commands::ttd_step,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
