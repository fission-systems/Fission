//! Fission Tauri — Library entry point.

mod commands;
mod dto;
pub(crate) mod error;
pub(crate) mod menu;
mod services;
mod state;

#[cfg(feature = "allocator-mimalloc")]
#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(all(
    feature = "allocator-jemallocator",
    not(feature = "allocator-mimalloc")
))]
#[global_allocator]
static GLOBAL_ALLOCATOR: jemallocator::Jemalloc = jemallocator::Jemalloc;

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
            // ===================================================================
            // Binary Analysis Domain
            // ===================================================================
            // Binary loading & info
            commands::open_file,
            commands::get_binary_info,
            // Metadata extraction
            commands::get_functions,
            commands::get_imports,
            commands::get_exports,
            commands::get_sections,
            commands::get_strings,
            // Hex editing
            commands::get_hex_view,
            commands::patch_bytes,
            commands::save_patched_binary,
            // Listing view
            commands::get_listing_info,
            commands::get_listing_chunk,
            // ===================================================================
            // Code Analysis Domain
            // ===================================================================
            // Assembly & decompilation
            commands::get_assembly,
            commands::decompile_function,
            // Control flow graph
            commands::get_cfg,
            commands::export_cfg_dot,
            // Cross-references
            commands::get_xrefs,
            commands::get_string_xrefs,
            // User annotations
            commands::rename_function,
            commands::add_comment,
            commands::get_comments,
            commands::toggle_bookmark,
            commands::get_bookmarks,
            commands::goto_address,
            // Function analysis & scanning
            commands::analyze_functions,
            commands::deep_scan_functions,
            commands::run_fid,
            commands::export_analysis_json,
            // ===================================================================
            // Debugging Domain
            // ===================================================================
            // Runtime debugging
            commands::debug_get_state,
            commands::debug_attach,
            commands::debug_detach,
            commands::debug_continue,
            commands::debug_step,
            commands::debug_add_breakpoint,
            commands::debug_remove_breakpoint,
            commands::debug_read_memory,
            // Time-travel debugging
            commands::ttd_start,
            commands::ttd_stop,
            commands::ttd_status,
            commands::ttd_seek,
            commands::ttd_step,
            // ===================================================================
            // Workspace Domain
            // ===================================================================
            // Project management
            commands::save_project,
            commands::load_project,
            commands::save_snapshot,
            commands::load_snapshot,
            commands::get_git_branch,
            // Search
            commands::search_binary,
            // Settings
            commands::get_settings,
            commands::save_settings,
            commands::get_decompiler_options,
            commands::apply_decompiler_options,
            commands::clear_decompiler_cache,
            // ===================================================================
            // Extensions Domain
            // ===================================================================
            // Plugin system
            commands::load_plugin,
            commands::unload_plugin,
            commands::list_plugins,
            commands::enable_plugin,
            commands::disable_plugin,
            // DevTools
            commands::toggle_devtools,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| panic!("error while running tauri application: {}", e));
}
