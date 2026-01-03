//! Shared application state for the Fission GUI.
//!
//! Contains all state that needs to be shared across UI panels.
//! Organized into domain-specific sub-states for maintainability.
//!
//! ## Architecture
//!
//! - `FissionContext`: Core application context (event bus, plugins) - shared with non-GUI code
//! - `AppState`: GUI-specific state that wraps the context and adds UI-related fields

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use lru::LruCache;

use crate::analysis::disasm::DisassembledInstruction;
use crate::analysis::loader::{FunctionInfo, LoadedBinary};
use crate::config::CONFIG;
use crate::core::context::FissionContext;

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
    /// Show cross-references window
    pub show_xrefs_window: bool,
    /// Selected address for xrefs viewing
    pub selected_xref_addr: Option<u64>,
    /// Current cursor position (Line, Column)
    pub cursor_pos: Option<(usize, usize)>,
    /// Current memory usage in bytes
    pub memory_usage: u64,
    /// Current git branch
    pub git_branch: String,
    /// Current progress (percentage 0.0-1.0, message)
    pub progress: Option<(f32, String)>,
}

impl Default for UIState {
    fn default() -> Self {
        Self {
            bottom_tab: BottomTab::Console,
            active_activity: Activity::Explorer,
            sidebar_visible: true,
            panel_visible: true,
            open_tabs: Vec::new(),
            active_tab_index: None,
            dynamic_mode: false,
            show_attach_dialog: false,
            show_xrefs_window: false,
            selected_xref_addr: None,
            cursor_pos: None,
            memory_usage: 0,
            git_branch: get_git_branch(),
            progress: None,
        }
    }
}

fn get_git_branch() -> String {
    std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
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
    /// Is the decompiler currently analyzing?
    pub decompiling: bool,
    /// Has the binary been loaded into the decompiler's persistent context?
    pub decompiler_context_loaded: bool,
    /// Cache of decompiled functions (LRU)
    pub decompile_cache: LruCache<u64, CachedDecompile>,
    /// Last loaded binary path (for recovery reload)
    pub last_binary_path: Option<String>,
    /// Extracted strings from binary
    pub extracted_strings: Vec<ExtractedString>,
    /// Filter for strings view
    pub strings_filter: String,
    /// Current offset in hex view
    pub hex_offset: u64,
    /// Patch offset input (hex string)
    pub patch_offset_input: String,
    /// Patch bytes input (hex string like "90 90 90")
    pub patch_bytes_input: String,
    /// Detection results (packer/compiler/language)
    pub detection_result: Option<crate::analysis::detector::DetectionResult>,
    /// Cross-references database
    pub xref_db: Option<crate::analysis::xrefs::XrefDatabase>,
    /// User-defined function names (address -> custom name)
    pub user_function_names: std::collections::HashMap<u64, String>,
    /// Rename dialog state: (address, current_input)
    pub rename_dialog: Option<(u64, String)>,
    /// Reconstructed imports (Dynamic Mode)
    pub reconstructed_imports: Vec<crate::unpacker::importer::ImportEntry>,
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
    /// Process filter for attach dialog
    pub process_filter: String,
    /// TitanEngine instance (Clean Room)
    pub titan_engine: Option<Arc<RwLock<crate::unpacker::engine::TitanEngine>>>,
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

/// Settings and preferences state
#[derive(serde::Serialize, serde::Deserialize)]
pub struct SettingsState {
    /// UI Theme mode (Light/Dark/Auto)
    pub theme_mode: ThemeMode,
    /// UI Scale factor (0.5 to 2.0)
    pub ui_scale: f32,
    /// Show developer tools?
    pub show_dev_tools: bool,
    /// Code Editor font size
    pub editor_font_size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum ThemeMode {
    #[default]
    Dark,
    Light,
    System,
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
///
/// ## Design Notes
///
/// - `ctx`: Core application context (shared with non-GUI components)
/// - Domain states (`analysis`, `debug`, `script`): Organized by feature area
/// - `ui`: Pure GUI state (tabs, visibility, layout)
/// - `settings`: User preferences (persisted to disk)
pub struct AppState {
    // =========================================================================
    // Core Context (shared infrastructure)
    // =========================================================================
    /// Core application context (event bus, plugins, etc.)
    /// This is the bridge between GUI and core systems.
    pub ctx: FissionContext,

    // =========================================================================
    // Console / CLI State
    // =========================================================================
    /// Log buffer for the output console
    pub log_buffer: Vec<String>,
    /// Current command input in the integrated CLI
    pub cli_input: String,
    /// File dialog path (unused currently)
    pub file_dialog_path: String,

    // =========================================================================
    // Domain-Specific States
    // =========================================================================
    /// UI state (tabs, visibility, layout)
    pub ui: UIState,
    /// Analysis state (binary, functions, decompilation)
    pub analysis: AnalysisState,
    /// Debug state (debugger, breakpoints, memory)
    pub debug: DebugStateUI,
    /// Script state (Python scripting)
    pub script: ScriptState,
    /// Settings state
    pub settings: SettingsState,

    // =========================================================================
    // UI-Specific Components
    // =========================================================================
    /// Plugin panel state
    pub plugin_panel_state: crate::ui::gui::panels::bottom_tabs::plugins::PluginPanelState,
    /// Undo/Redo Command Manager
    pub command_manager: crate::ui::gui::commands::CommandManager,
}

// Convenience accessors for backwards compatibility
impl AppState {
    /// Get the event bus (convenience accessor)
    #[inline]
    pub fn event_bus(&self) -> &Arc<crate::core::events::EventBus> {
        &self.ctx.event_bus
    }

    /// Get the plugin manager (convenience accessor)
    #[inline]
    pub fn plugin_manager(&self) -> &Arc<RwLock<crate::plugin::PluginManager>> {
        &self.ctx.plugin_manager
    }
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
    Plugins,
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
    StepInto,
    StepOver,
    Detach,
    Dump,
    ImportRec,
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
        // Use configured cache size or default to 100
        let cache_size = NonZeroUsize::new(CONFIG.analysis.decompile_cache_size)
            .unwrap_or_else(|| NonZeroUsize::new(100).expect("100 is non-zero"));

        Self {
            loaded_binary: None,
            selected_function: None,
            decompiled_code: "// Select a function to decompile".into(),
            asm_instructions: Vec::new(),
            decompiling: false,
            decompiler_context_loaded: false,
            decompile_cache: LruCache::new(cache_size),
            last_binary_path: None,
            extracted_strings: Vec::new(),
            strings_filter: String::new(),
            hex_offset: 0,
            patch_offset_input: String::new(),
            patch_bytes_input: String::new(),
            detection_result: None,
            xref_db: None,
            user_function_names: std::collections::HashMap::new(),
            rename_dialog: None,
            reconstructed_imports: Vec::new(),
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
            process_filter: String::new(),
            titan_engine: Some(Arc::new(RwLock::new(
                crate::unpacker::engine::TitanEngine::new(),
            ))),
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

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            theme_mode: ThemeMode::Dark,
            ui_scale: 1.5,
            show_dev_tools: false,
            editor_font_size: 14,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        // Create the core context first
        let ctx = FissionContext::new();

        Self {
            ctx,
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
            settings: crate::core::config_store::load(),
            plugin_panel_state: Default::default(),
            command_manager: crate::ui::gui::commands::CommandManager::default(),
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
