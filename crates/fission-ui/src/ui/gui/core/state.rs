//! Shared application state for the Fission GUI.
//!
//! Contains all state that needs to be shared across UI panels.
//! Organized into domain-specific sub-states for maintainability.
//!
//! ## Architecture
//!
//! - `FissionContext`: Core application context (event bus, plugins) - shared with non-GUI code
//! - `AppState`: GUI-specific state that wraps the context and adds UI-related fields
//! - `Domain Models`: Pure business logic data (domain.rs)
//! - `ViewModels`: UI-specific transient state (viewmodels.rs)

use std::sync::{Arc, RwLock};

use crate::app::context::FissionContext;
use fission_core::settings::SettingsState;
use fission_loader::loader::LoadedBinary;

// Import domain models and viewmodels (declared in mod.rs)
use super::domain::{AnalysisDomain, DebugDomain};
use super::viewmodels::ViewModelContainer;

// Re-export for convenience
pub use super::domain;
pub use super::viewmodels;

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
    /// Group xrefs by function (call graph style)
    pub xrefs_group_by_function: bool,
    /// Show string cross-references window
    pub show_string_xrefs_window: bool,
    /// Current cursor position (Line, Column)
    pub cursor_pos: Option<(usize, usize)>,
    /// Expanded folders in project explorer (path -> expanded state)
    pub expanded_folders: std::collections::HashSet<String>,
    /// Current memory usage in bytes
    pub memory_usage: u64,
    /// Current CPU usage percentage (0.0-100.0)
    pub cpu_usage: f32,
    /// Current git branch
    pub git_branch: String,
    /// Current progress (percentage 0.0-1.0, message)
    pub progress: Option<(f32, String)>,
    /// Navigation back stack (address history)
    pub back_stack: Vec<u64>,
    /// Navigation forward stack
    pub forward_stack: Vec<u64>,
    /// Pending navigation action: go back
    pub pending_nav_back: bool,
    /// Pending navigation action: go forward
    pub pending_nav_forward: bool,
    /// Currently highlighted symbol (e.g. clicked register or name)
    pub highlighted_symbol: Option<String>,
    /// Pending jump request (address)
    pub pending_jump: Option<u64>,
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
            xrefs_group_by_function: false,
            show_string_xrefs_window: false,
            cursor_pos: None,
            memory_usage: 0,
            cpu_usage: 0.0,
            git_branch: get_git_branch(),
            progress: None,
            expanded_folders: std::collections::HashSet::new(),
            back_stack: Vec::new(),
            forward_stack: Vec::new(),
            pending_nav_back: false,
            pending_nav_forward: false,
            highlighted_symbol: None,
            pending_jump: None,
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
///
/// **REFACTORED**: Pure domain data only. UI input fields moved to ViewModels.
pub struct AnalysisState {
    /// Domain data (business logic)
    pub domain: AnalysisDomain,
    // Legacy compatibility fields (deprecated - use domain directly)
    // These will be removed in a future version
}

impl AnalysisState {
    /// Get loaded binary (convenience accessor for backward compatibility)
    #[inline]
    pub fn loaded_binary(&self) -> &Option<Arc<LoadedBinary>> {
        &self.domain.loaded_binary
    }

    /// Get loaded binary (mutable, for backward compatibility)
    #[inline]
    pub fn loaded_binary_mut(&mut self) -> &mut Option<Arc<LoadedBinary>> {
        &mut self.domain.loaded_binary
    }

    /// Get extracted strings (convenience accessor)
    #[inline]
    pub fn extracted_strings(&self) -> &Vec<ExtractedString> {
        &self.domain.extracted_strings
    }

    /// Get extracted strings (mutable)
    #[inline]
    pub fn extracted_strings_mut(&mut self) -> &mut Vec<ExtractedString> {
        &mut self.domain.extracted_strings
    }

    /// Get user function names (convenience accessor)
    #[inline]
    pub fn user_function_names(&self) -> &std::collections::HashMap<u64, String> {
        &self.domain.user_function_names
    }

    /// Get user function names (mutable)
    #[inline]
    pub fn user_function_names_mut(&mut self) -> &mut std::collections::HashMap<u64, String> {
        &mut self.domain.user_function_names
    }
}

/// Debug-related state (debugger, breakpoints, memory)
/// Debug-related state (debugger, breakpoints, memory)
///
/// **REFACTORED**: Pure domain data only. UI input fields moved to ViewModels.
pub struct DebugStateUI {
    /// Domain data (business logic)
    pub domain: DebugDomain,

    // Pending actions from UI (still need to be here for immediate processing)
    /// Pending debug control action from UI
    pub pending_debug_action: Option<DebugAction>,
    /// Pending breakpoint action from UI
    pub pending_bp_action: Option<DebugBpAction>,
    /// Pending memory read action
    pub pending_mem_read: Option<(u64, usize)>,
}

impl DebugStateUI {
    /// Get debug state (convenience accessor)
    pub fn debug_state(&self) -> &crate::debug::types::DebugState {
        &self.domain.debug_state
    }

    /// Get debug state (mutable)
    pub fn debug_state_mut(&mut self) -> &mut crate::debug::types::DebugState {
        &mut self.domain.debug_state
    }
}

/// Script-related state (Python scripting)
pub struct ScriptState {
    /// Python script input code
    pub script_code: String,
    /// Script execution output
    pub script_output: Vec<String>,
    /// Is script currently executing?
    pub is_executing: bool,
    /// Path of the currently loaded script file
    pub script_path: Option<String>,
}

// ============================================================================
// Main Application State
// ============================================================================

/// Main GUI application state.
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
    // ViewModels (UI-specific transient state)
    // =========================================================================
    /// ViewModels for all panels (input fields, filters, dialogs)
    /// This replaces the scattered UI state that was previously in domain models
    pub viewmodels: ViewModelContainer,

    // =========================================================================
    // UI-Specific Components
    // =========================================================================
    /// Plugin panel state
    pub plugin_panel_state: crate::ui::gui::panels::bottom_tabs::plugins::PluginPanelState,
    /// Undo/Redo Command Manager
    pub command_manager: crate::ui::gui::core::commands::CommandManager,
}

// Convenience accessors for backwards compatibility
impl AppState {
    /// Get the event bus (convenience accessor)
    #[inline]
    pub fn event_bus(&self) -> &Arc<crate::app::events::EventBus> {
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
    ReverseStep,
    ReverseContinue,
    Seek(u64),
}

/// Breakpoint actions requested from UI
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugBpAction {
    Add(u64),
    Remove(u64),
}

/// Extracted string from binary (re-exported from domain)
pub use domain::ExtractedString;

/// String encoding type (re-exported from domain)
pub use domain::StringEncoding;

/// Bottom panel tab selection
#[derive(Clone, Copy, PartialEq, Default)]
pub enum BottomTab {
    #[default]
    Console,
    HexView,
    Strings,
    Xrefs,
    Search,
    Bookmarks,
    Imports,
    Cfg,
    Debug,
    Script,
    Timeline,
}

// ============================================================================
// Default implementations
// ============================================================================

impl Default for AnalysisState {
    fn default() -> Self {
        Self {
            domain: AnalysisDomain::default(),
        }
    }
}

impl Default for DebugStateUI {
    fn default() -> Self {
        Self {
            domain: DebugDomain::default(),
            pending_debug_action: None,
            pending_bp_action: None,
            pending_mem_read: None,
        }
    }
}

impl Default for ScriptState {
    fn default() -> Self {
        Self {
            script_code: "# Fission Python Script\n# Use 'api' to access the Fission API\n\nbinary = api.get_binary()\nif binary:\n    api.log(f\"Loaded: {binary.name}\")\n    api.log(f\"Functions: {len(api.get_functions())}\")\nelse:\n    api.log(\"No binary loaded\")\n".into(),
            script_output: Vec::new(),
            is_executing: false,
            script_path: None,
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
            viewmodels: ViewModelContainer::new(),
            plugin_panel_state: Default::default(),
            command_manager: crate::ui::gui::core::commands::CommandManager::default(),
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
