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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
