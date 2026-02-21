//! Fission Tauri — DTO types for JSON serialization over Tauri IPC.

use serde::{Deserialize, Serialize};

/// Binary metadata sent to the frontend after loading.
#[derive(Debug, Clone, Serialize)]
pub struct BinaryInfo {
    pub name: String,
    pub path: String,
    pub arch: String,
    pub format: String,
    pub entry_point: String,
    pub section_count: usize,
    pub function_count: usize,
    pub image_base: String,
}

/// Function information for the function list.
#[derive(Debug, Clone, Serialize)]
pub struct FunctionDto {
    pub address: String,
    pub name: String,
    pub size: u64,
    /// Category: "import", "export", or "internal"
    pub category: String,
}

/// Decompilation result.
#[derive(Debug, Clone, Serialize)]
pub struct DecompileResult {
    pub code: String,
    pub function_name: String,
    pub address: String,
}

/// Single assembly instruction.
#[derive(Debug, Clone, Serialize)]
pub struct AsmInstructionDto {
    pub address: String,
    pub bytes: String,
    pub mnemonic: String,
    pub operands: String,
    /// Optional user comment on this address
    pub comment: Option<String>,
}

/// Extracted string from binary.
#[derive(Debug, Clone, Serialize)]
pub struct StringDto {
    pub offset: String,
    pub value: String,
    pub encoding: String,
}

/// Import entry (IAT/PLT).
#[derive(Debug, Clone, Serialize)]
pub struct ImportDto {
    pub address: String,
    pub name: String,
    pub library: String,
    pub ordinal: Option<u32>,
}

/// Section information.
#[derive(Debug, Clone, Serialize)]
pub struct SectionDto {
    pub name: String,
    pub address: String,
    pub size: u64,
    pub flags: String,
}

/// Cross-reference entry.
#[derive(Debug, Clone, Serialize)]
pub struct XrefDto {
    pub from_address: String,
    pub to_address: String,
    pub xref_type: String,
    pub from_function: Option<String>,
}

/// Bookmark entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkDto {
    pub address: String,
    pub label: String,
    pub function_name: Option<String>,
}

/// Goto result.
#[derive(Debug, Clone, Serialize)]
pub struct GotoResult {
    pub address: String,
    pub function_name: Option<String>,
    pub found: bool,
}

/// A row in the hex view (16 bytes per row).
#[derive(Debug, Clone, Serialize)]
pub struct HexRow {
    pub offset: String,
    pub hex: Vec<String>,
    pub ascii: String,
}

/// Hex view data.
#[derive(Debug, Clone, Serialize)]
pub struct HexViewData {
    pub rows: Vec<HexRow>,
    pub total_size: u64,
}

/// A single search result.
#[derive(Debug, Clone, Serialize)]
pub struct SearchResultDto {
    pub address: String,
    pub name: String,
    pub result_type: String, // "function", "string", "address"
    pub context: String,
}

// ============================================================================
// Project Save / Load
// ============================================================================

/// Complete Fission project file (.fprj), serialised to JSON.
/// Stores all user-generated annotations so they survive app restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FissionProject {
    /// Format version — bump when making breaking schema changes.
    pub version: u32,
    /// Absolute path to the analysed binary.
    pub binary_path: String,
    /// User comments keyed by hex address (e.g. `"0x401000"`).
    pub comments: std::collections::HashMap<String, String>,
    /// User-defined function renames keyed by hex address.
    pub renames: std::collections::HashMap<String, String>,
    /// User bookmarks.
    pub bookmarks: Vec<BookmarkDto>,
}

// ============================================================================
// Application Settings
// ============================================================================

/// Application-wide preferences persisted between sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// UI colour theme: `"dark"`, `"light"`, or `"system"`.
    pub theme: String,
    /// Editor / panel font size in pixels.
    pub font_size: f32,
    /// Decompiler output style: `"c-like"`, `"pseudo"`, or `"verbose"`.
    pub decompile_style: String,
    /// Decompiler simplification level 0-3 (0 = off, 3 = aggressive).
    pub simplify_level: u8,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            font_size: 14.0,
            decompile_style: "c-like".to_string(),
            simplify_level: 1,
        }
    }
}

// ============================================================================
// CFG (Control Flow Graph)
// ============================================================================

/// A single basic block node in the CFG.
#[derive(Debug, Clone, Serialize)]
pub struct CfgNode {
    /// Block index (unique within the CFG)
    pub id: usize,
    /// Start address in hex (e.g. `"0x401000"`)
    pub start_address: String,
    /// End address in hex (exclusive)
    pub end_address: String,
    /// Assembly instructions in this block (`"mnemonic operands"` format)
    pub instructions: Vec<String>,
    /// Whether this is the function entry block
    pub is_entry: bool,
    /// Whether this block ends with a return / unconditional exit
    pub is_exit: bool,
}

/// A directed edge between two CFG nodes.
#[derive(Debug, Clone, Serialize)]
pub struct CfgEdge {
    pub from: usize,
    pub to: usize,
    /// `"unconditional"`, `"true"` (branch taken), `"false"` (fall-through)
    pub kind: String,
}

/// Complete CFG response returned by `get_cfg`.
#[derive(Debug, Clone, Serialize)]
pub struct CfgDto {
    pub function_name: String,
    pub function_address: String,
    pub nodes: Vec<CfgNode>,
    pub edges: Vec<CfgEdge>,
}

// ============================================================================
// Listing View
// ============================================================================

/// A single row in the linear listing view.
#[derive(Debug, Clone, Serialize)]
pub struct ListingRow {
    /// Hex address of this row (e.g. `"0x401000"`)
    pub address: String,
    /// Hex bytes (e.g. `"55 48 89 e5"`) — empty for label/section rows
    pub bytes: String,
    /// Mnemonic — empty for label/section rows
    pub mnemonic: String,
    /// Operands — empty for label/section rows
    pub operands: String,
    /// Function label starting at this address (if any)
    pub label: Option<String>,
    /// User comment at this address (if any)
    pub comment: Option<String>,
    /// `"instruction"` | `"label"` | `"section"`
    pub row_type: String,
}

/// Metadata about the full listing, returned by `get_listing_info`.
#[derive(Debug, Clone, Serialize)]
pub struct ListingInfo {
    /// Hex address of the binary entry point
    pub entry_point: String,
    /// Hex start address of the first executable section
    pub first_addr: String,
    /// Hex end address of the last executable section (exclusive)
    pub last_addr: String,
    /// Total byte size of all executable sections (used to estimate scroll size)
    pub total_exec_bytes: u64,
}

// ============================================================================
// Debug
// ============================================================================

/// Debugger session status (mirrors fission_analysis::debug::types::DebugStatus).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DebugStatusDto {
    #[default]
    Detached,
    Attaching,
    Running,
    Suspended,
    Terminated,
}

/// CPU register snapshot (x64).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegisterStateDto {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub rflags: u64,
}

/// Single breakpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakpointInfoDto {
    pub address: String,
    pub enabled: bool,
}

/// Complete debug session state returned by `debug_get_state`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DebugStateDto {
    pub status: DebugStatusDto,
    pub attached_pid: Option<u32>,
    pub breakpoints: Vec<BreakpointInfoDto>,
    pub registers: Option<RegisterStateDto>,
    pub last_event: Option<String>,
    pub events: Vec<String>,
}

// ── Phase 5: String XRefs ────────────────────────────────────────────────────

/// A single code location that references a string.
#[derive(Debug, Clone, Serialize)]
pub struct StringXrefCallsiteDto {
    pub from_address: String,
    pub from_function: Option<String>,
}

/// A string (found in the binary) together with every code location that
/// references its virtual address.
#[derive(Debug, Clone, Serialize)]
pub struct StringXrefDto {
    pub string_address: String,
    pub string_value: String,
    pub refs: Vec<StringXrefCallsiteDto>,
}
