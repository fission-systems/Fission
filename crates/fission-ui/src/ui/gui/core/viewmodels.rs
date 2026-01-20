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

    /// Cached filter key (filter text + string count)
    pub cache_key: Option<(String, usize)>,

    /// Cached indices of filtered strings
    pub cached_indices: Vec<usize>,
}

impl StringsViewModel {
    pub fn new() -> Self {
        Self {
            filter: String::new(),
            min_length: 4,
            cache_key: None,
            cached_indices: Vec::new(),
        }
    }

    /// Check if cache needs refresh
    pub fn needs_refresh(&self, string_count: usize) -> bool {
        match &self.cache_key {
            Some((cached_filter, cached_count)) => {
                *cached_filter != self.filter || *cached_count != string_count
            }
            None => true,
        }
    }
}

/// ViewModel for Functions panel - holds rename dialog state and filters
#[derive(Default)]
pub struct FunctionsViewModel {
    /// Rename dialog state: (address, current_input)
    pub rename_dialog: Option<(u64, String)>,

    /// Comment dialog state: (address, current_input)
    pub comment_dialog: Option<(u64, String)>,

    /// Function name filter text (case-insensitive search)
    pub filter: String,

    /// Show import functions
    pub show_imports: bool,

    /// Show export functions
    pub show_exports: bool,

    /// Show internal (non-import, non-export) functions
    pub show_internals: bool,

    /// Cached filter key for invalidation (filter + toggles + function count)
    pub cache_key: Option<(String, bool, bool, bool, usize)>,

    /// Cached indices of filtered functions (indices into binary.functions)
    pub cached_indices: Vec<usize>,
}

impl FunctionsViewModel {
    pub fn new() -> Self {
        Self {
            rename_dialog: None,
            comment_dialog: None,
            filter: String::new(),
            show_imports: true,
            show_exports: true,
            show_internals: true,
            cache_key: None,
            cached_indices: Vec::new(),
        }
    }

    /// Check if cache is valid and return current key
    pub fn current_cache_key(&self, func_count: usize) -> (String, bool, bool, bool, usize) {
        (
            self.filter.clone(),
            self.show_imports,
            self.show_exports,
            self.show_internals,
            func_count,
        )
    }

    /// Check if cache needs refresh
    pub fn needs_refresh(&self, func_count: usize) -> bool {
        match &self.cache_key {
            Some(key) => *key != self.current_cache_key(func_count),
            None => true,
        }
    }
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

/// ViewModel for Listing View panel - continuous binary view
#[derive(Default, Clone)]
pub struct ListingViewModel {
    /// Current scroll offset address
    pub current_address: u64,
    /// Number of instructions to display
    pub display_count: usize,
    /// Address input for Go To
    pub goto_address_input: String,
}

impl ListingViewModel {
    pub fn new() -> Self {
        Self {
            current_address: 0,
            display_count: 100,
            goto_address_input: String::new(),
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
// Navigation ViewModels
// ============================================================================

/// ViewModel for Navigation - holds Go to Address dialog state
#[derive(Default)]
pub struct NavigationViewModel {
    /// Go to address dialog: Option<current_input>
    pub goto_address_input: Option<String>,
}

impl NavigationViewModel {
    pub fn new() -> Self {
        Self {
            goto_address_input: None,
        }
    }
}

#[derive(Default, Clone)]
pub struct DecomToken {
    pub text: String,
    pub color: eframe::egui::Color32,
    pub is_clickable: bool,
    /// If true, this token represents a function call that can be navigated to
    pub is_function_call: bool,
}

#[derive(Default, Clone)]
pub struct XrefCallSummary {
    pub addr: u64,
    pub label: String,
    pub count: usize,
}

#[derive(Default)]
pub struct XrefsViewModel {
    pub cache_key: Option<(String, u64, u64)>,
    pub callers: Vec<XrefCallSummary>,
    pub callees: Vec<XrefCallSummary>,
}

impl XrefsViewModel {
    pub fn clear(&mut self) {
        self.cache_key = None;
        self.callers.clear();
        self.callees.clear();
    }
}

/// ViewModel for Decompiled Code panel - holds tokenized cache
#[derive(Default)]
pub struct DecompileViewModel {
    /// Cached tokenized lines for the current function
    pub tokenized_lines: Vec<Vec<DecomToken>>,
}

impl DecompileViewModel {
    pub fn new() -> Self {
        Self {
            tokenized_lines: Vec::new(),
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
    pub xrefs: XrefsViewModel,
    pub debug: DebugViewModel,
    pub search: SearchViewModel,
    pub navigation: NavigationViewModel,
    pub decompile: DecompileViewModel,
    pub listing: ListingViewModel,
}

impl ViewModelContainer {
    pub fn new() -> Self {
        Self {
            hex: HexViewModel::default(),
            strings: StringsViewModel::new(),
            functions: FunctionsViewModel::default(),
            string_xrefs: StringXrefsViewModel::new(),
            xrefs: XrefsViewModel::default(),
            debug: DebugViewModel::new(),
            search: SearchViewModel::new(),
            navigation: NavigationViewModel::new(),
            decompile: DecompileViewModel::new(),
            listing: ListingViewModel::new(),
        }
    }
}
