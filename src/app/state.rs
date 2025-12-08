//! Shared application state
//!
//! Contains state that is shared between CLI and GUI modes.

use crate::analysis::loader::{LoadedBinary, FunctionInfo};

/// Shared application state
pub struct AppState {
    /// Currently loaded binary
    pub binary: Option<LoadedBinary>,
    /// Selected function for decompilation
    pub selected_function: Option<FunctionInfo>,
    /// Current decompiled C code
    pub decompiled_code: String,
    /// Is a decompilation in progress?
    pub decompiling: bool,
    /// Server connection status
    pub server_connected: bool,
    /// Is debugging active?
    pub is_debugging: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            binary: None,
            selected_function: None,
            decompiled_code: "// Select a function to decompile".into(),
            decompiling: false,
            server_connected: false,
            is_debugging: false,
        }
    }
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a binary is loaded
    pub fn has_binary(&self) -> bool {
        self.binary.is_some()
    }

    /// Get binary path if loaded
    pub fn binary_path(&self) -> Option<&str> {
        self.binary.as_ref().map(|b| b.path.as_str())
    }

    /// Get function count
    pub fn function_count(&self) -> usize {
        self.binary.as_ref().map(|b| b.functions.len()).unwrap_or(0)
    }
}
