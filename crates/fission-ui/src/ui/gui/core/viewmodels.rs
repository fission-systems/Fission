//! ViewModels - UI-specific state and presentation logic
//!
//! ViewModels hold transient UI state (input fields, filters, dialogs)
//! and provide methods for UI interactions. They are separate from domain models.

// ============================================================================
// Analysis ViewModels
// ============================================================================

/// ViewModel for Hex View panel - holds UI input state
#[derive(Default)]
pub struct HexViewModel {
    /// Current hex offset for viewing
    pub current_offset: u64,
    
    /// Patch offset input (hex string like "0x1000")
    pub patch_offset_input: String,
    
    /// Patch bytes input (hex string like "90 90 90")
    pub patch_bytes_input: String,
}

/// ViewModel for Strings panel - holds filter state
#[derive(Default)]
pub struct StringsViewModel {
    /// Filter text for strings view
    pub filter: String,
    
    /// Minimum string length to display
    pub min_length: usize,
}

impl StringsViewModel {
    pub fn new() -> Self {
        Self {
            filter: String::new(),
            min_length: 4,
        }
    }
}

/// ViewModel for Functions panel - holds rename dialog state
#[derive(Default)]
pub struct FunctionsViewModel {
    /// Rename dialog state: (address, current_input)
    pub rename_dialog: Option<(u64, String)>,
    
    /// Function filter text
    pub filter: String,
}

/// ViewModel for String Xrefs panel
#[derive(Default)]
pub struct StringXrefsViewModel {
    /// Search term for string xrefs
    pub search_term: String,
    
    /// Minimum string length for analysis
    pub min_length: usize,
}

impl StringXrefsViewModel {
    pub fn new() -> Self {
        Self {
            search_term: String::new(),
            min_length: 4,
        }
    }
}

// ============================================================================
// Debug ViewModels
// ============================================================================

/// ViewModel for Debug panel - holds UI input state
#[derive(Default)]
pub struct DebugViewModel {
    /// Breakpoint address input (hex string)
    pub breakpoint_input: String,
    
    /// Memory view address input (hex string)
    pub mem_addr_input: String,
    
    /// Memory view length input (decimal)
    pub mem_len_input: String,
    
    /// Process filter for attach dialog
    pub process_filter: String,
}

impl DebugViewModel {
    pub fn new() -> Self {
        Self {
            breakpoint_input: String::new(),
            mem_addr_input: String::new(),
            mem_len_input: "64".to_string(),
            process_filter: String::new(),
        }
    }
}

// ============================================================================
// Search ViewModels
// ============================================================================

/// ViewModel for Search panel
#[derive(Default)]
pub struct SearchViewModel {
    /// Search query text
    pub query: String,
    
    /// Search type (string, pattern, etc.)
    pub search_type: SearchType,
    
    /// Case sensitive search
    pub case_sensitive: bool,
}

#[derive(Default, Clone, Copy, PartialEq)]
pub enum SearchType {
    #[default]
    String,
    Pattern,
    Regex,
}

impl SearchViewModel {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            search_type: SearchType::String,
            case_sensitive: false,
        }
    }
}

// ============================================================================
// Composite ViewModel Container
// ============================================================================

/// Container for all panel-specific ViewModels
///
/// This struct aggregates all UI-specific state that was previously
/// mixed into domain models. Each ViewModel is responsible for one panel's UI state.
#[derive(Default)]
pub struct ViewModelContainer {
    pub hex: HexViewModel,
    pub strings: StringsViewModel,
    pub functions: FunctionsViewModel,
    pub string_xrefs: StringXrefsViewModel,
    pub debug: DebugViewModel,
    pub search: SearchViewModel,
}

impl ViewModelContainer {
    pub fn new() -> Self {
        Self {
            hex: HexViewModel::default(),
            strings: StringsViewModel::new(),
            functions: FunctionsViewModel::default(),
            string_xrefs: StringXrefsViewModel::new(),
            debug: DebugViewModel::new(),
            search: SearchViewModel::new(),
        }
    }
}
