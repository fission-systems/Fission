//! Shared application state for the Fission GUI.
//!
//! Contains all state that needs to be shared across UI panels.
//! Organized into domain-specific sub-states for maintainability.

use std::collections::HashMap;
use std::time::Instant;

use crate::analysis::loader::{LoadedBinary, FunctionInfo};
use crate::analysis::disasm::DisassembledInstruction;

// ============================================================================
// Sub-state structures
// ============================================================================

/// UI-related state (tabs, visibility, layout)
/// UI-related state (tabs, visibility, layout)
pub struct UIState {
    /// Currently selected bottom tab
    pub bottom_tab: BottomTab,
    /// Active side bar activity
    pub active_activity: Activity,
    /// Side bar visible?
    pub sidebar_visible: bool,
    /// Bottom panel visible?
    pub panel_visible: bool,
    /// Open editor tabs
    pub open_tabs: Vec<EditorTab>,
    /// Currently active tab index
    pub active_tab_index: Option<usize>,
    /// Dynamic mode (on/off)
    pub dynamic_mode: bool,
    /// Show attach dialog
    pub show_attach_dialog: bool,
}

/// Analysis-related state (binary, functions, decompilation)
pub struct AnalysisState {
    /// Currently loaded binary (if any)
    pub loaded_binary: Option<std::sync::Arc<LoadedBinary>>,
    /// Selected function (for decompilation view)
    pub selected_function: Option<FunctionInfo>,
    /// Current decompiled C code
    pub decompiled_code: String,
    /// Current assembly instructions
    pub asm_instructions: Vec<DisassembledInstruction>,
    /// Is decompilation in progress?
    pub decompiling: bool,
    /// Decompile result cache (address -> result)
    pub decompile_cache: HashMap<u64, CachedDecompile>,
    /// Last loaded binary path (for recovery reload)
    pub last_binary_path: Option<String>,
    /// Extracted strings from binary
    pub extracted_strings: Vec<ExtractedString>,
    /// Filter for strings view
    pub strings_filter: String,
    /// Current offset in hex view
    pub hex_offset: u64,
}

/// Debug-related state (debugger, breakpoints, memory)
/// Debug-related state (debugger, breakpoints, memory)
pub struct DebugStateUI {
    /// Is debugger running?
    pub is_debugging: bool,
    /// Debugger state
    pub debug_state: crate::debug::types::DebugState,
    /// Cached process list for dialog
    pub process_list: Vec<crate::debug::types::ProcessInfo>,
    /// Pending debug control action from UI
    pub pending_debug_action: Option<DebugAction>,
    /// Pending breakpoint action from UI
    pub pending_bp_action: Option<DebugBpAction>,
    /// Temporary input for breakpoint address
    pub breakpoint_input: String,
    /// Pending memory read action
    pub pending_mem_read: Option<(u64, usize)>,
    /// Memory view address input (hex)
    pub mem_addr_input: String,
    /// Memory view length input (decimal)
    pub mem_len_input: String,
    /// Last memory dump text
    pub mem_dump: String,
    /// Time Travel Debugging timeline
    pub timeline: crate::debug::ttd::Timeline,
}

/// Script-related state (Python scripting)
pub struct ScriptState {
    /// Python script input code
    pub script_code: String,
    /// Script execution output
    pub script_output: Vec<String>,
    /// Is script currently executing?
    pub script_running: bool,
    /// Current script file path (for save/load)
    pub script_path: Option<String>,
}

// ============================================================================
// Cached decompile result
// ============================================================================

/// Cached decompile result for performance optimization
#[derive(Clone)]
pub struct CachedDecompile {
    pub c_code: String,
    pub asm_instructions: Vec<DisassembledInstruction>,
    #[allow(dead_code)]
    pub timestamp: Instant,
}

// ============================================================================
// Main AppState (composed of sub-states)
// ============================================================================

/// Main application state container
/// 
/// This struct holds all shared state that panels need to read/modify.
/// Organized into domain-specific sub-states for better maintainability.
pub struct AppState {
    /// Log buffer for the output console
    pub log_buffer: Vec<String>,
    /// Current command input in the integrated CLI
    pub cli_input: String,
    /// File dialog path (unused currently)
    pub file_dialog_path: String,

    /// UI state (tabs, visibility, layout)
    pub ui: UIState,
    /// Analysis state (binary, functions, decompilation)
    pub analysis: AnalysisState,
    /// Debug state (debugger, breakpoints, memory)
    pub debug: DebugStateUI,
    /// Script state (Python scripting)
    pub script: ScriptState,
    /// Plugin manager
    pub plugin_manager: crate::plugin::PluginManager,
    /// Plugin panel state
    pub plugin_panel_state: crate::ui::gui::panels::bottom_tabs::plugins::PluginPanelState,
}

// ============================================================================
// Enums and helper types
// ============================================================================

/// Activities in the Activity Bar
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Activity {
    #[default]
    Explorer,
    Search,
    Debug,
    Settings,
}

/// Tabs in the central editor area
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorTab {
    Assembly(String),   // Function name or "Main"
    Decompiled(String), // Function name
    HexView,
    Welcome,
}

impl EditorTab {
    pub fn title(&self) -> String {
        match self {
            EditorTab::Assembly(name) => format!("Asm: {}", name),
            EditorTab::Decompiled(name) => format!("C: {}", name),
            EditorTab::HexView => "Hex View".to_string(),
            EditorTab::Welcome => "Welcome".to_string(),
        }
    }
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
    Script,
    Timeline,
    Plugins,
}

// ============================================================================
// Default implementations
// ============================================================================

impl Default for AnalysisState {
    fn default() -> Self {
        Self {
            loaded_binary: None,
            selected_function: None,
            decompiled_code: "// Select a function to decompile".into(),
            asm_instructions: Vec::new(),
            decompiling: false,
            decompile_cache: HashMap::new(),
            last_binary_path: None,
            extracted_strings: Vec::new(),
            strings_filter: String::new(),
            hex_offset: 0,
        }
    }
}

impl Default for DebugStateUI {
    fn default() -> Self {
        Self {
            is_debugging: false,
            debug_state: crate::debug::types::DebugState::default(),
            process_list: Vec::new(),
            pending_debug_action: None,
            pending_bp_action: None,
            breakpoint_input: String::new(),
            pending_mem_read: None,
            mem_addr_input: String::new(),
            mem_len_input: "64".to_string(),
            mem_dump: String::new(),
            timeline: crate::debug::ttd::Timeline::default(),
        }
    }
}

impl Default for ScriptState {
    fn default() -> Self {
        Self {
            script_code: "# Fission Python Script\n# Use 'api' to access the Fission API\n\nbinary = api.get_binary()\nif binary:\n    api.log(f\"Loaded: {binary.name}\")\n    api.log(f\"Functions: {len(api.get_functions())}\")\nelse:\n    api.log(\"No binary loaded\")\n".into(),
            script_output: Vec::new(),
            script_running: false,
            script_path: None,
        }
    }
}

impl Default for UIState {
    fn default() -> Self {
        Self {
            bottom_tab: BottomTab::Console,
            active_activity: Activity::Explorer,
            sidebar_visible: true,
            panel_visible: true,
            open_tabs: vec![EditorTab::Welcome],
            active_tab_index: Some(0),
            dynamic_mode: true,
            show_attach_dialog: false,
        }
    }
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
            file_dialog_path: String::new(),
            ui: UIState::default(),
            analysis: AnalysisState::default(),
            debug: DebugStateUI::default(),
            script: ScriptState::default(),
            plugin_manager: crate::plugin::PluginManager::default(),
            plugin_panel_state: Default::default(),
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
