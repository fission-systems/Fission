//! Fission Tauri — DTO types for JSON serialization over Tauri IPC.

use fission_core::{
    DEFAULT_DECOMP_TIMEOUT_MS, DEFAULT_L1_CACHE_SIZE, MAX_FUNCTION_SIZE,
    MAX_INSTRUCTIONS_PER_FUNCTION,
};
use serde::{Deserialize, Serialize};

/// Binary metadata sent to the frontend after loading.
#[derive(Debug, Clone, Serialize)]
pub struct BinaryInfo {
    pub name: String,
    pub path: String,
    pub arch: String,
    pub bits: u32,
    pub format: String,
    pub entry_point: String,
    pub section_count: usize,
    pub function_count: usize,
    pub import_count: usize,
    pub export_count: usize,
    pub image_base: String,
    pub detections: Vec<DetectionInfo>,
}

/// Static signature detections associated with the loaded binary.
#[derive(Debug, Clone, Serialize)]
pub struct DetectionInfo {
    pub detection_type: String,
    pub name: String,
    pub version: Option<String>,
    pub confidence: String,
    pub details: Option<String>,
}

/// Function information for the function list.
#[derive(Debug, Clone, Serialize)]
pub struct FunctionDto {
    pub address: String,
    pub name: String,
    pub size: u64,
    pub is_import: bool,
    pub is_export: bool,
    pub origin: Option<String>,
    pub kind: Option<String>,
    pub source_section: Option<String>,
    pub external_library: Option<String>,
    pub is_thunk_like: bool,
    /// Category: "import", "export", "thunk", "external", "debug", or "internal"
    pub category: String,
}

/// Decompilation result.
#[derive(Debug, Clone, Serialize)]
pub struct DecompileResult {
    pub code: String,
    pub function_name: String,
    pub address: String,
    pub engine_used: DecompilerEngineMode,
    pub fell_back: bool,
    pub fallback_reason: Option<String>,
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
    pub origin: Option<String>,
    pub kind: Option<String>,
    pub source_section: Option<String>,
    pub external_library: Option<String>,
    pub is_thunk_like: bool,
}

/// Export table entry.
#[derive(Debug, Clone, Serialize)]
pub struct ExportDto {
    pub address: String,
    pub name: String,
    pub ordinal: Option<u32>,
    pub forwarder: Option<String>,
    pub size: u64,
    pub origin: Option<String>,
    pub kind: Option<String>,
    pub source_section: Option<String>,
    pub external_library: Option<String>,
    pub is_thunk_like: bool,
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
    /// Full decompiler options (persisted separately from basic settings).
    #[serde(default)]
    pub decompiler_options: Option<DecompilerOptions>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            font_size: 14.0,
            decompile_style: "c-like".to_string(),
            simplify_level: 1,
            decompiler_options: None,
        }
    }
}

// ============================================================================
// Decompiler Options (Ghidra-level configuration)
// ============================================================================

/// Comprehensive decompiler options mirroring Ghidra's DecompileOptions.
///
/// Divided into four categories: Analysis, Post-Processing, Display, Performance.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DecompilerOptions {
    #[serde(default)]
    pub engine_mode: DecompilerEngineMode,
    pub analysis: AnalysisOptions,
    pub cpp_postprocess: CppPostProcessOptions,
    pub rust_postprocess: RustPostProcessOptions,
    pub display: DisplayOptions,
    pub performance: PerformanceOptions,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum DecompilerEngineMode {
    #[serde(alias = "mlil_preview")]
    Nir,
    #[serde(alias = "legacy")]
    #[default]
    Auto,
}

/// Ghidra engine analysis options (controlled via FFI set_feature).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisOptions {
    pub infer_pointers: bool,
    pub analyze_loops: bool,
    pub readonly_propagate: bool,
    pub record_jumploads: bool,
    pub allow_inline: bool,
    pub disable_toomanyinstructions_error: bool,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            infer_pointers: true,
            analyze_loops: true,
            readonly_propagate: true,
            record_jumploads: true,
            allow_inline: false,
            disable_toomanyinstructions_error: true,
        }
    }
}

/// C++ side post-processing options (controlled via FFI set_feature with "pp_" prefix).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CppPostProcessOptions {
    pub apply_struct_definitions: bool,
    pub iat_symbols: bool,
    pub strip_shadow_params: bool,
    pub smart_constants: bool,
    pub inline_strings: bool,
    pub constants: bool,
    pub guids: bool,
    pub unicode_strings: bool,
    pub interlocked_patterns: bool,
    pub xunknown_types: bool,
    pub seh_cleanup: bool,
    pub global_symbols: bool,
    pub internal_names: bool,
    pub struct_offsets: bool,
    pub fid_names: bool,
}

impl Default for CppPostProcessOptions {
    fn default() -> Self {
        Self {
            apply_struct_definitions: true,
            iat_symbols: true,
            strip_shadow_params: true,
            smart_constants: true,
            inline_strings: true,
            constants: true,
            guids: true,
            unicode_strings: true,
            interlocked_patterns: true,
            xunknown_types: true,
            seh_cleanup: true,
            global_symbols: true,
            internal_names: true,
            struct_offsets: true,
            fid_names: true,
        }
    }
}

/// Rust side **legacy text** post-processing toggles (mirrors core defaults).
///
/// Defaults are **all disabled**. Enable explicitly only for legacy compatibility experiments —
/// semantic shaping belongs in NIR/HIR, not string rewriting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustPostProcessOptions {
    pub clean_rust: bool,
    pub clean_go: bool,
    pub swift_demangle: bool,
    pub field_offsets: bool,
    pub insert_casts: bool,
    pub arithmetic_idioms: bool,
    pub temp_var_inlining: bool,
    pub stack_var_normalization: bool,
    pub piece_access_normalization: bool,
    pub deref_to_array: bool,
    pub bitop_to_logicop: bool,
    pub remove_dead_branches: bool,
    pub simplify_if: bool,
    pub while_to_for: bool,
    pub dead_assign_removal: bool,
    pub rename_induction_vars: bool,
    pub rename_semantic_vars: bool,
    pub loop_idioms: bool,
    pub switch_reconstruction: bool,
    pub mul_to_shift: bool,
    pub dwarf_names: bool,
    pub string_pointers: bool,
}

impl Default for RustPostProcessOptions {
    fn default() -> Self {
        Self {
            clean_rust: false,
            clean_go: false,
            swift_demangle: false,
            field_offsets: false,
            insert_casts: false,
            arithmetic_idioms: false,
            temp_var_inlining: false,
            stack_var_normalization: false,
            piece_access_normalization: false,
            deref_to_array: false,
            bitop_to_logicop: false,
            remove_dead_branches: false,
            simplify_if: false,
            while_to_for: false,
            dead_assign_removal: false,
            rename_induction_vars: false,
            rename_semantic_vars: false,
            loop_idioms: false,
            switch_reconstruction: false,
            mul_to_shift: false,
            dwarf_names: false,
            string_pointers: false,
        }
    }
}

/// Display/formatting options for decompiler output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayOptions {
    /// Maximum characters per line before wrapping
    pub max_line_width: u32,
    /// Indentation width in spaces
    pub indent_width: u8,
    /// Integer display format
    pub integer_format: String,
    /// Comment style
    pub comment_style: String,
    /// Show type casts
    pub show_casts: bool,
    /// Show namespace qualifiers
    pub show_namespaces: bool,
    /// Show line numbers
    pub show_line_numbers: bool,
}

impl Default for DisplayOptions {
    fn default() -> Self {
        Self {
            max_line_width: 100,
            indent_width: 2,
            integer_format: "best_fit".to_string(),
            comment_style: "c_style".to_string(),
            show_casts: true,
            show_namespaces: false,
            show_line_numbers: true,
        }
    }
}

/// Performance/limits options for the decompiler engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceOptions {
    /// Decompilation timeout in milliseconds
    pub timeout_ms: u64,
    /// Maximum function size in bytes
    pub max_function_size: u32,
    /// Maximum instructions before aborting
    pub max_instructions: u32,
    /// Number of functions to cache
    pub cache_size: u32,
}

impl Default for PerformanceOptions {
    fn default() -> Self {
        Self {
            timeout_ms: DEFAULT_DECOMP_TIMEOUT_MS,
            max_function_size: MAX_FUNCTION_SIZE as u32,
            max_instructions: MAX_INSTRUCTIONS_PER_FUNCTION,
            cache_size: DEFAULT_L1_CACHE_SIZE as u32,
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
    /// Number of basic blocks (= nodes.len())
    pub block_count: usize,
    /// Number of CFG edges (= edges.len())
    pub edge_count: usize,
    /// McCabe cyclomatic complexity V(G) = E – N + 2
    pub cyclomatic_complexity: usize,
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
    /// Mnemonic category for syntax-highlighting:
    /// `"call"` | `"jmp"` | `"cjmp"` | `"ret"` | `"nop"` |
    /// `"push_pop"` | `"mov"` | `"cmp"` | `"int"` | `"normal"`
    #[serde(default)]
    pub mnemonic_type: String,
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

// ── Plugin System ────────────────────────────────────────────────────────────

/// Plugin type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PluginTypeDto {
    Native,
    Unknown,
}

/// Plugin metadata sent to the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct PluginInfoDto {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub plugin_type: PluginTypeDto,
    pub enabled: bool,
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

// ============================================================================
// Phase 8: Analysis Export
// ============================================================================

/// A single function entry in the exported analysis JSON.
#[derive(Debug, Clone, Serialize)]
pub struct ExportedFunctionDto {
    pub address: String,
    pub name: String,
    pub is_renamed: bool,
}

/// Root document written to the exported `.json` file.
#[derive(Debug, Clone, Serialize)]
pub struct AnalysisExportDto {
    /// Format version — bump on breaking schema changes.
    pub version: u32,
    /// Unix timestamp (seconds) when the export was created.
    pub exported_at: u64,
    /// Binary file name (basename).
    pub binary_name: String,
    /// Absolute path to the binary.
    pub binary_path: String,
    /// Simple fingerprint: `"bytes:<size>"`.
    pub binary_fingerprint: String,
    /// All detected functions with current names applied.
    pub functions: Vec<ExportedFunctionDto>,
    /// User comments keyed by hex address (e.g. `"0x401000"`).
    pub comments: std::collections::HashMap<String, String>,
    /// User bookmarks.
    pub bookmarks: Vec<BookmarkDto>,
}

// ============================================================================
// Phase 5: TTD (Time Travel Debugging)
// ============================================================================

/// Register state at a single recorded TTD step (x64 subset).
#[derive(Debug, Clone, Serialize)]
pub struct TtdSnapshotDto {
    pub step: u64,
    pub thread_id: u32,
    pub rip: String,
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsp: u64,
    pub rbp: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rflags: u64,
}

/// Current state of the TTD timeline, returned by `ttd_status` and `ttd_seek`.
#[derive(Debug, Clone, Serialize)]
pub struct TtdStateDto {
    pub is_recording: bool,
    pub snapshot_count: usize,
    /// `[min_step, max_step]` or `null` when no snapshots exist.
    pub step_range: Option<[u64; 2]>,
    /// Current seek position (null when not in replay mode).
    pub current_step: Option<u64>,
    /// Register state at the current seek position (null when no snapshot).
    pub current_snapshot: Option<TtdSnapshotDto>,
}

// ============================================================================
// FID (Function Identification)
// ============================================================================

/// A single function matched by the FID signature scanner.
#[derive(Debug, Clone, Serialize)]
pub struct FidMatchDto {
    pub address: String,
    pub name: String,
    pub previous_name: String,
}

/// Result returned by `run_fid`.
#[derive(Debug, Clone, Serialize)]
pub struct FidResultDto {
    pub matched: usize,
    pub total_scanned: usize,
    pub fidbf_attempted: usize,
    pub fidbf_loaded: usize,
    pub fidbf_failed: usize,
    pub matches: Vec<FidMatchDto>,
}

// ── Debug conversions ─────────────────────────────────────────────────────────

impl From<fission_ttd::RegisterState> for RegisterStateDto {
    fn from(r: fission_ttd::RegisterState) -> Self {
        Self {
            rax: r.rax,
            rbx: r.rbx,
            rcx: r.rcx,
            rdx: r.rdx,
            rsi: r.rsi,
            rdi: r.rdi,
            rbp: r.rbp,
            rsp: r.rsp,
            r8: r.r8,
            r9: r.r9,
            r10: r.r10,
            r11: r.r11,
            r12: r.r12,
            r13: r.r13,
            r14: r.r14,
            r15: r.r15,
            rip: r.rip,
            rflags: r.rflags,
        }
    }
}
