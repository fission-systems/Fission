use crate::ui::gui::panels::bottom_tabs::ScriptAction;
use crate::ui::gui::state::AppState;

pub fn handle_action(
    state: &mut AppState,
    action: ScriptAction,
    python_bridge: &mut crate::script::PythonBridge,
) {
    match action {
        ScriptAction::Execute(code) => {
            execute_python_script(state, &code, python_bridge);
        }
        ScriptAction::Load => {
            load_script_file(state);
        }
        ScriptAction::Save => {
            save_script_file(state);
        }
        ScriptAction::None => {}
    }
}

fn load_script_file(state: &mut AppState) {
    if let Some(path) = rfd::FileDialog::new()
        .add_filter("Python", &["py"])
        .add_filter("All Files", &["*"])
        .pick_file()
    {
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                state.script.script_code = content;
                state.script.script_path = Some(path.display().to_string());
                state
                    .script
                    .script_output
                    .push(format!("[✓] Loaded: {}", path.display()));
            }
            Err(e) => {
                state
                    .script
                    .script_output
                    .push(format!("[Error] Failed to load: {}", e));
            }
        }
    }
}

fn save_script_file(state: &mut AppState) {
    let default_path = state
        .script
        .script_path
        .as_ref()
        .map(|p| std::path::PathBuf::from(p))
        .unwrap_or_else(|| std::path::PathBuf::from("script.py"));

    if let Some(path) = rfd::FileDialog::new()
        .add_filter("Python", &["py"])
        .set_file_name(
            default_path
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or("script.py"),
        )
        .save_file()
    {
        match std::fs::write(&path, &state.script.script_code) {
            Ok(_) => {
                state.script.script_path = Some(path.display().to_string());
                state
                    .script
                    .script_output
                    .push(format!("[✓] Saved: {}", path.display()));
            }
            Err(e) => {
                state
                    .script
                    .script_output
                    .push(format!("[Error] Failed to save: {}", e));
            }
        }
    }
}

#[cfg(feature = "python")]
fn execute_python_script(
    state: &mut AppState,
    code: &str,
    python_bridge: &mut crate::script::PythonBridge,
) {
    state.script.script_running = true;
    state
        .script
        .script_output
        .push(format!(">>> Executing script..."));

    // Initialize Python if needed
    if let Err(e) = python_bridge.initialize() {
        state
            .script
            .script_output
            .push(format!("[Error] Failed to initialize Python: {}", e));
        state.script.script_running = false;
        return;
    }

    // Sync loaded binary to Python bridge
    python_bridge.set_binary(state.analysis.loaded_binary.clone());

    // Execute the script
    match python_bridge.run(code) {
        Ok(_) => {
            state
                .script
                .script_output
                .push("[✓] Script executed successfully".into());
        }
        Err(e) => {
            state.script.script_output.push(format!("[Error] {}", e));
        }
    }

    state.script.script_running = false;
}

#[cfg(not(feature = "python"))]
fn execute_python_script(
    state: &mut AppState,
    _code: &str,
    _python_bridge: &mut crate::script::PythonBridge,
) {
    state
        .script
        .script_output
        .push("[Error] Python support not enabled. Build with --features python".into());
}
