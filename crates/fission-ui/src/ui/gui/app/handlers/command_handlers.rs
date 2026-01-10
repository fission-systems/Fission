//! Individual command handlers for CLI commands.

use crossbeam_channel::Sender;

use crate::ui::gui::core::messages::AsyncMessage;
use crate::ui::gui::core::state::AppState;

/// Handle 'help' or '?' command
pub fn handle_help(state: &mut AppState) {
    state.log("Available commands:");
    state.log("  load <path>  : Load a binary for analysis");
    state.log("  funcs        : List functions");
    state.log("  clear        : Clear console");
    state.log("  exit         : Quit Fission");
}

/// Handle 'funcs' or 'functions' command
pub fn handle_list_functions(state: &mut AppState) {
    if let Some(ref binary) = state.analysis.domain.loaded_binary {
        let funcs: Vec<_> = binary
            .functions
            .iter()
            .map(|f| (f.address, f.name.clone()))
            .collect();
        state.log(format!("[*] {} functions:", funcs.len()));
        for (addr, name) in funcs {
            state.log(format!("  0x{:08x} {}", addr, name));
        }
    } else {
        state.log("[!] No binary loaded");
    }
}

/// Handle 'clear' command
pub fn handle_clear(state: &mut AppState) {
    state.clear_logs();
    state.log("[*] Console cleared");
}

/// Handle 'exit' or 'quit' command
pub fn handle_exit() {
    std::process::exit(0);
}

/// Handle 'undo' command
pub fn handle_undo(state: &mut AppState) {
    let mut mgr = std::mem::take(&mut state.command_manager);
    match mgr.undo(state) {
        Ok(msg) => state.log(format!("[✓] {}", msg)),
        Err(e) => state.log(format!("[!] Undo failed: {}", e)),
    }
    state.command_manager = mgr;
}

/// Handle 'redo' command
pub fn handle_redo(state: &mut AppState) {
    let mut mgr = std::mem::take(&mut state.command_manager);
    match mgr.redo(state) {
        Ok(msg) => state.log(format!("[✓] {}", msg)),
        Err(e) => state.log(format!("[!] Redo failed: {}", e)),
    }
    state.command_manager = mgr;
}

/// Handle 'plugin load <path>' command
pub fn handle_plugin_load(state: &mut AppState, path: &str) {
    let result = if let Ok(mut mgr) = state.plugin_manager().write() {
        match mgr.load_plugin(path) {
            Ok(id) => Some(Ok(id)),
            Err(e) => Some(Err(e)),
        }
    } else {
        None
    };

    match result {
        Some(Ok(id)) => state.log(format!("[✓] Plugin loaded: {}", id)),
        Some(Err(e)) => state.log(format!("[!] Failed to load plugin: {}", e)),
        None => state.log("[!] Failed to lock plugin manager"),
    }
}

/// Handle 'plugin list' command
pub fn handle_plugin_list(state: &mut AppState) {
    let plugins = if let Ok(mgr) = state.plugin_manager().read() {
        let mut p: Vec<_> = mgr.list_plugins().into_iter().cloned().collect();
        p.sort_by_key(|p| p.id.clone());
        p
    } else {
        Vec::new()
    };

    state.log("[*] Loaded Plugins:");

    if plugins.is_empty() {
        state.log("    (none)");
    } else {
        for plugin in plugins {
            state.log(format!(
                "    - {} ({}) v{} [{}]",
                plugin.name,
                plugin.id,
                plugin.version,
                if plugin.enabled {
                    "Enabled"
                } else {
                    "Disabled"
                }
            ));
        }
    }
}

/// Handle 'patch <addr> <bytes...>' command
pub fn handle_patch(state: &mut AppState, cmd: &str) {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.len() < 3 {
        state.log("[!] Usage: patch <address> <hex_byte1> [hex_byte2 ...]");
        return;
    }

    let addr_str = parts[1].trim_start_matches("0x");
    match u64::from_str_radix(addr_str, 16) {
        Ok(addr) => {
            let mut bytes = Vec::new();
            let mut valid = true;

            for s in &parts[2..] {
                match u8::from_str_radix(s, 16) {
                    Ok(b) => bytes.push(b),
                    Err(_) => {
                        state.log(format!("[!] Invalid byte: {}", s));
                        valid = false;
                        break;
                    }
                }
            }

            if valid {
                let command = Box::new(crate::ui::gui::core::commands::PatchBytesCommand {
                    address: addr,
                    old_bytes: Vec::new(),
                    new_bytes: bytes,
                });

                let mut mgr = std::mem::take(&mut state.command_manager);
                if let Err(e) = mgr.execute(command, state) {
                    state.log(format!("[!] Patch failed: {}", e));
                }
                state.command_manager = mgr;
            }
        }
        Err(_) => state.log(format!("[!] Invalid address: {}", parts[1])),
    }
}

/// Handle 'rename <addr> <new_name>' command
pub fn handle_rename(state: &mut AppState, cmd: &str) {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.len() != 3 {
        state.log("[!] Usage: rename <address> <new_name>");
        return;
    }

    let addr_str = parts[1].trim_start_matches("0x");
    match u64::from_str_radix(addr_str, 16) {
        Ok(addr) => {
            let new_name = parts[2].to_string();
            let command = Box::new(crate::ui::gui::core::commands::RenameFunctionCommand {
                address: addr,
                old_name: String::new(), // Will be filled by execute
                new_name,
            });

            let mut mgr = std::mem::take(&mut state.command_manager);
            if let Err(e) = mgr.execute(command, state) {
                state.log(format!("[!] Rename failed: {}", e));
            }
            state.command_manager = mgr;
        }
        Err(_) => state.log(format!("[!] Invalid address: {}", parts[1])),
    }
}

/// Handle 'load <path>' command
pub fn handle_load(state: &mut AppState, tx: Sender<AsyncMessage>, path: &str) {
    super::super::file_ops::load_binary(state, tx, path);
}

/// Handle unknown command
pub fn handle_unknown(state: &mut AppState, cmd: &str) {
    state.log(format!("[!] Unknown command: {}", cmd));
}
