//! Shared application state for the Fission GUI.
//!
//! Contains all state that needs to be shared across UI panels.

use std::collections::HashMap;
use std::time::Instant;

use crate::analysis::loader::{LoadedBinary, FunctionInfo};
use crate::analysis::disasm::DisassembledInstruction;

/// Cached decompile result for performance optimization
#[derive(Clone)]
pub struct CachedDecompile {
    pub c_code: String,
    pub asm_instructions: Vec<DisassembledInstruction>,
    #[allow(dead_code)]
    pub timestamp: Instant,
}

/// Main application state container
/// 
/// This struct holds all shared state that panels need to read/modify.
pub struct AppState {
    /// Log buffer for the output console
    pub log_buffer: Vec<String>,

    /// Current command input in the integrated CLI
    pub cli_input: String,

    /// Currently loaded binary (if any)
    pub loaded_binary: Option<LoadedBinary>,

    /// Debugger running state
    pub is_debugging: bool,

    /// Selected function (for decompilation view)
    pub selected_function: Option<FunctionInfo>,

    /// Current decompiled C code
    pub decompiled_code: String,

    /// Current assembly instructions
    pub asm_instructions: Vec<DisassembledInstruction>,

    /// Is decompilation in progress?
    pub decompiling: bool,

    /// Server connection status
    pub server_connected: bool,

    /// File dialog path (unused currently)
    pub file_dialog_path: String,

    /// Decompile result cache (address -> result)
    pub decompile_cache: HashMap<u64, CachedDecompile>,

    /// Last loaded binary path (for recovery reload)
    pub last_binary_path: Option<String>,

    /// Server recovery in progress
    pub recovering: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            log_buffer: vec![
                "==============================================================".into(),
                "  Fission - Next-Gen Dynamic Instrumentation Platform".into(),
                "  \"Split the Binary, Fuse the Power.\"".into(),
                "==============================================================".into(),
                "".into(),
                "[*] Ready. Load a binary to begin analysis.".into(),
            ],
            cli_input: String::new(),
            loaded_binary: None,
            is_debugging: false,
            selected_function: None,
            decompiled_code: "// Select a function to decompile".into(),
            asm_instructions: Vec::new(),
            decompiling: false,
            server_connected: false,
            file_dialog_path: String::new(),
            decompile_cache: HashMap::new(),
            last_binary_path: None,
            recovering: false,
        }
    }
}

impl AppState {
    /// Add a log message to the output buffer
    pub fn log(&mut self, message: impl Into<String>) {
        self.log_buffer.push(message.into());
    }

    /// Clear the log buffer
    pub fn clear_logs(&mut self) {
        self.log_buffer.clear();
    }
}
