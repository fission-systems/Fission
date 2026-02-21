//! Fission Tauri — Library entry point.

mod commands;
mod dto;
mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
