use crate::error::CmdResult;

/// Toggle the WebView developer tools (like browser F12).
#[tauri::command]
pub fn toggle_devtools(window: tauri::WebviewWindow) -> CmdResult<()> {
    if window.is_devtools_open() {
        window.close_devtools();
    } else {
        window.open_devtools();
    }
    Ok(())
}
