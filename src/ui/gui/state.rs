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

    // ========== Debug State ==========
    /// Debugger state
    pub debug_state: crate::debug::types::DebugState,
    /// Show attach dialog
    pub show_attach_dialog: bool,
    /// Cached process list for dialog
    pub process_list: Vec<crate::debug::types::ProcessInfo>,

    // ========== Bottom Panel Tab ==========
    /// Currently selected bottom tab
    pub bottom_tab: BottomTab,

    // ========== Hex View State ==========
    /// Current offset in hex view
    pub hex_offset: u64,

    // ========== Strings State ==========
    /// Extracted strings from binary
    pub extracted_strings: Vec<ExtractedString>,
    /// Filter for strings view
    pub strings_filter: String,

    /// Dynamic mode (on/off)
    pub dynamic_mode: bool,

    /// Pending debug control action from UI
    pub pending_debug_action: Option<DebugAction>,

    /// Pending breakpoint action from UI
    pub pending_bp_action: Option<DebugBpAction>,
    /// Temporary input for breakpoint address
    pub breakpoint_input: String,

    /// Memory view address input (hex)
    pub mem_addr_input: String,
    /// Memory view length input (decimal)
    pub mem_len_input: String,
    /// Last memory dump text
    pub mem_dump: String,
}

/// Debug control actions requested from UI
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugAction {
    Continue,
    Step,
}

/// Breakpoint actions requested from UI
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugBpAction {
    Add(u64),
    Remove(u64),
}
/// Extracted string from binary
#[derive(Clone)]
pub struct ExtractedString {
    /// Offset in binary
    pub offset: u64,
    /// String value
    pub value: String,
    /// String encoding type
    pub encoding: StringEncoding,
}

/// String encoding type
#[derive(Clone, Copy, PartialEq)]
pub enum StringEncoding {
    Ascii,
    Utf16Le,
}

/// Bottom panel tab selection
#[derive(Clone, Copy, PartialEq, Default)]
pub enum BottomTab {
    #[default]
    Console,
    HexView,
    Strings,
    Imports,
    Debug,
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
            // Debug state
            debug_state: crate::debug::types::DebugState::default(),
            show_attach_dialog: false,
            process_list: Vec::new(),
            // Bottom panel tab
            bottom_tab: BottomTab::Console,
            // Hex view state
            hex_offset: 0,
            // Strings state
            extracted_strings: Vec::new(),
            strings_filter: String::new(),
            dynamic_mode: true,
            pending_debug_action: None,
            pending_bp_action: None,
            breakpoint_input: String::new(),
            mem_addr_input: String::new(),
            mem_len_input: "64".to_string(),
            mem_dump: String::new(),
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
