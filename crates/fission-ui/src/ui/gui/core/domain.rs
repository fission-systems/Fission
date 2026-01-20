//! Domain Models - Pure business logic data structures
//!
//! These structures contain only domain data without UI concerns.
//! They can be tested independently and used in non-GUI contexts.

use std::sync::Arc;
use std::time::Instant;

pub use crate::analysis::disasm::DisassembledInstruction;
use fission_analysis::analysis::cfg::CfgSummary;
use fission_loader::loader::{FunctionInfo, LoadedBinary};

// ============================================================================
// Core Domain Types
// ============================================================================

/// Analysis domain model - pure data, no UI state
///
/// This struct contains only domain-level data about the binary being analyzed.
/// UI-specific state (input fields, filters, dialogs) is kept separate in ViewModels.
pub struct AnalysisDomain {
    /// Currently loaded binary (if any)
    pub loaded_binary: Option<Arc<LoadedBinary>>,

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

    /// Last loaded binary path (for recovery reload)
    pub last_binary_path: Option<String>,

    /// Extracted strings from binary
    pub extracted_strings: Vec<ExtractedString>,

    /// Detection results (packer/compiler/language)
    pub detection_result: Option<fission_loader::detector::DetectionResult>,

    /// Cross-references database
    pub xref_db: Option<crate::analysis::xrefs::XrefDatabase>,

    /// Call graph derived from cross-references
    pub call_graph: Option<crate::analysis::CallGraph>,

    /// User-defined function names (address -> custom name)
    pub user_function_names: std::collections::HashMap<u64, String>,

    /// User-defined comments (address -> comment string)
    pub user_comments: std::collections::HashMap<u64, String>,
    /// Bookmarked addresses: addr -> label
    pub bookmarks: std::collections::HashMap<u64, String>,

    /// Reconstructed imports (Dynamic Mode)
    pub reconstructed_imports: Vec<crate::unpacker::importer::ImportEntry>,

    /// String xref analysis results
    pub string_xref_results: Option<fission_analysis::analysis::string_xrefs::StringXrefAnalysis>,

    /// Minimum string length for xref analysis
    pub string_xref_min_len: usize,

    /// Current hex view offset
    pub hex_offset: usize,

    // Project-related state (multi-binary workspace)
    /// Current project folder path (if loaded from folder)
    pub project_folder: Option<String>,

    /// All binaries loaded in the project
    pub project_binaries: Vec<Arc<LoadedBinary>>,

    /// Currently selected binary index in project
    pub selected_binary_index: Option<usize>,

    /// Current CFG analysis result
    pub cfg_analysis: Option<CfgSummary>,
}

/// Debug domain model - pure debugger state, no UI inputs
pub struct DebugDomain {
    /// Is debugger running?
    pub is_debugging: bool,

    /// Debugger state
    pub debug_state: crate::debug::types::DebugState,

    /// Cached process list for dialog
    pub process_list: Vec<crate::debug::types::ProcessInfo>,

    /// Pending debug control action from UI
    pub pending_debug_action: Option<crate::ui::gui::core::state::DebugAction>,

    /// Pending breakpoint action from UI
    pub pending_bp_action: Option<crate::ui::gui::core::state::DebugBpAction>,

    /// Pending memory read action
    pub pending_mem_read: Option<(u64, usize)>,

    /// Last memory dump text
    pub mem_dump: String,

    /// Time Travel Debugging timeline
    pub timeline: crate::debug::ttd::Timeline,

    /// TitanEngine instance (Clean Room)
    pub titan_engine: Option<Arc<std::sync::RwLock<crate::unpacker::engine::TitanEngine>>>,
}

// ============================================================================
// Helper Types
// ============================================================================

/// Cached decompile result for performance optimization
#[derive(Clone)]
pub struct CachedDecompile {
    pub c_code: String,
    pub asm_instructions: Vec<DisassembledInstruction>,
    pub timestamp: Instant,
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

// ============================================================================
// Default Implementations
// ============================================================================

impl Default for AnalysisDomain {
    fn default() -> Self {
        use crate::core::config::CONFIG;
        use std::num::NonZeroUsize;

        let _cache_size = NonZeroUsize::new(CONFIG.analysis.decompile_cache_size)
            .unwrap_or_else(|| NonZeroUsize::new(100).expect("100 is non-zero"));

        Self {
            loaded_binary: None,
            selected_function: None,
            decompiled_code: "// Select a function to decompile".into(),
            asm_instructions: Vec::new(),
            decompiling: false,
            decompiler_context_loaded: false,
            last_binary_path: None,
            extracted_strings: Vec::new(),
            detection_result: None,
            xref_db: None,
            call_graph: None,
            user_function_names: std::collections::HashMap::new(),
            user_comments: std::collections::HashMap::new(),
            bookmarks: std::collections::HashMap::new(),
            reconstructed_imports: Vec::new(),
            string_xref_results: None,
            string_xref_min_len: 4,
            hex_offset: 0,
            project_folder: None,
            project_binaries: Vec::new(),
            selected_binary_index: None,
            cfg_analysis: None,
        }
    }
}

impl AnalysisDomain {
    /// Scan for missing logic by searching for function prologues in the binary code.
    /// This helps finding functions that are reached via obfuscated paths (indirect calls)
    /// and thus disconnected from the main call graph.
    pub fn scan_for_missing_functions(&mut self) -> usize {
        if let Some(ref mut binary_arc) = self.loaded_binary {
            // Arc::make_mut ensures we have unique ownership (CoW)
            // Then we call discover_functions_by_prologue on the LoadedBinary
            std::sync::Arc::make_mut(binary_arc).discover_functions_by_prologue()
        } else {
            0
        }
    }
}

impl Default for DebugDomain {
    fn default() -> Self {
        Self {
            is_debugging: false,
            debug_state: crate::debug::types::DebugState::default(),
            process_list: Vec::new(),
            pending_debug_action: None,
            pending_bp_action: None,
            pending_mem_read: None,
            mem_dump: String::new(),
            timeline: crate::debug::ttd::Timeline::default(),
            titan_engine: Some(Arc::new(std::sync::RwLock::new(
                crate::unpacker::engine::TitanEngine::new(),
            ))),
        }
    }
}
