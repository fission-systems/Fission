// Fission Tauri - Type Definitions
// Mirrors the Rust DTO types from src-tauri/src/dto.rs

export interface BinaryInfo {
    name: string;
    path: string;
    arch: string;
    bits: number;
    format: string;
    entry_point: string;
    section_count: number;
    function_count: number;
    import_count: number;
    export_count: number;
    image_base: string;
    detections: DetectionInfo[];
}

export interface DetectionInfo {
    detection_type: string;
    name: string;
    version: string | null;
    confidence: string;
    details: string | null;
}

export type FunctionCategory = "import" | "export" | "internal" | "thunk" | "external" | "debug";

export interface FunctionDto {
    address: string;
    name: string;
    size: number;
    is_import: boolean;
    is_export: boolean;
    origin: string | null;
    kind: string | null;
    source_section: string | null;
    external_library: string | null;
    is_thunk_like: boolean;
    category: FunctionCategory;
}

export interface DecompileResult {
    code: string;
    function_name: string;
    address: string;
    engine_used: DecompilerEngineMode;
    fell_back: boolean;
    fallback_reason?: string | null;
}

export interface AsmInstructionDto {
    address: string;
    bytes: string;
    mnemonic: string;
    operands: string;
    comment: string | null;
}

export interface StringDto {
    offset: string;
    value: string;
    encoding: string;
}

export interface ImportDto {
    address: string;
    name: string;
    library: string;
    ordinal: number | null;
    origin: string | null;
    kind: string | null;
    source_section: string | null;
    external_library: string | null;
    is_thunk_like: boolean;
}

export interface SectionDto {
    name: string;
    address: string;
    size: number;
    flags: string;
}

export interface XrefDto {
    from_address: string;
    to_address: string;
    xref_type: string;
    from_function: string | null;
    to_function: string | null;
    operand_index?: number | null;
    sleigh_kind?: string | null;
}

export interface BookmarkDto {
    address: string;
    label: string;
    function_name: string | null;
}

export interface GotoResult {
    address: string;
    function_name: string | null;
    found: boolean;
}

export interface HexRow {
    offset: string;
    hex: string[];
    ascii: string;
}

export interface HexViewData {
    rows: HexRow[];
    total_size: number;
}

export interface SearchResultDto {
    address: string;
    name: string;
    result_type: "function" | "string" | "address";
    context: string;
}

// Editor tab model
export interface EditorTab {
    id: string;
    title: string;
    type: "decompile" | "assembly" | "listing" | "hexview";
    address: string;
    functionName: string;
}

// Activity bar item
export type ActivityView = "explorer" | "search" | "debug" | "settings" | "plugins";

// Bottom panel tab
export type BottomTab = "console" | "strings" | "hex" | "imports" | "exports" | "bookmarks" | "xrefs" | "search" | "cfg" | "debug" | "string-xrefs" | "patches" | "notes" | "timeline";

// Plugin type
export type PluginType = "native" | "unknown";

// Plugin metadata
export interface PluginInfoDto {
    id: string;
    name: string;
    version: string;
    author: string;
    description: string;
    plugin_type: PluginType;
    enabled: boolean;
}

// Export table entry (PE exports, ELF/Mach-O functions flagged as export)
export interface ExportDto {
    address: string;
    name: string;
    ordinal: number | null;
    forwarder: string | null;
    size: number;
    origin: string | null;
    kind: string | null;
    source_section: string | null;
    external_library: string | null;
    is_thunk_like: boolean;
}

// Record of a byte patch applied to the in-memory binary
export interface PatchRecord {
    address: number;
    label: string;
    original: number[];
    patched: number[];
}

// ── Phase 5: String XRefs ────────────────────────────────────────────────────

export interface StringXrefCallsiteDto {
    from_address: string;
    from_function: string | null;
}

export interface StringXrefDto {
    string_address: string;
    string_value: string;
    refs: StringXrefCallsiteDto[];
}

// Application settings
export interface AppSettings {
    theme: "dark" | "light" | "system";
    font_size: number;
    decompile_style: "c-like" | "pseudo" | "verbose";
    simplify_level: number;
    decompiler_options?: DecompilerOptions;
}

// ============================================================================
// Decompiler Options (Ghidra-level configuration)
// ============================================================================

export interface DecompilerOptions {
    engine_mode: DecompilerEngineMode;
    analysis: AnalysisOptions;
    cpp_postprocess: CppPostProcessOptions;
    rust_postprocess: RustPostProcessOptions;
    display: DisplayOptions;
    performance: PerformanceOptions;
}

export type DecompilerEngineMode = "nir" | "auto";

/** Ghidra engine analysis options (controlled via FFI set_feature). */
export interface AnalysisOptions {
    infer_pointers: boolean;
    analyze_loops: boolean;
    readonly_propagate: boolean;
    record_jumploads: boolean;
    allow_inline: boolean;
    disable_toomanyinstructions_error: boolean;
}

/** C++ side post-processing options (controlled via FFI set_feature with "pp_" prefix). */
export interface CppPostProcessOptions {
    apply_struct_definitions: boolean;
    iat_symbols: boolean;
    strip_shadow_params: boolean;
    smart_constants: boolean;
    inline_strings: boolean;
    constants: boolean;
    guids: boolean;
    unicode_strings: boolean;
    interlocked_patterns: boolean;
    xunknown_types: boolean;
    seh_cleanup: boolean;
    global_symbols: boolean;
    internal_names: boolean;
    struct_offsets: boolean;
    fid_names: boolean;
}

/** Rust side post-processing options (individual pass toggles). */
export interface RustPostProcessOptions {
    clean_rust: boolean;
    clean_go: boolean;
    swift_demangle: boolean;
    field_offsets: boolean;
    insert_casts: boolean;
    arithmetic_idioms: boolean;
    temp_var_inlining: boolean;
    stack_var_normalization: boolean;
    piece_access_normalization: boolean;
    deref_to_array: boolean;
    bitop_to_logicop: boolean;
    remove_dead_branches: boolean;
    simplify_if: boolean;
    while_to_for: boolean;
    dead_assign_removal: boolean;
    rename_induction_vars: boolean;
    rename_semantic_vars: boolean;
    loop_idioms: boolean;
    switch_reconstruction: boolean;
    mul_to_shift: boolean;
    dwarf_names: boolean;
    string_pointers: boolean;
}

/** Display/formatting options for decompiler output. */
export interface DisplayOptions {
    max_line_width: number;
    indent_width: number;
    integer_format: "hex" | "decimal" | "best_fit";
    comment_style: "c_style" | "cpp_style";
    show_casts: boolean;
    show_namespaces: boolean;
    show_line_numbers: boolean;
}

/** Performance/limits options for the decompiler engine. */
export interface PerformanceOptions {
    timeout_ms: number;
    /** Saved for compatibility; Rust-Sleigh uses core `RustSleighDecompileConfig` (same as CLI). */
    max_function_size: number;
    /** Saved for compatibility; Rust-Sleigh instruction budget follows CLI defaults. */
    max_instructions: number;
    cache_size: number;
}

/** Creates default DecompilerOptions matching Rust defaults. */
export function defaultDecompilerOptions(): DecompilerOptions {
    return {
        engine_mode: "auto",
        analysis: {
            infer_pointers: true,
            analyze_loops: true,
            readonly_propagate: true,
            record_jumploads: true,
            allow_inline: false,
            disable_toomanyinstructions_error: true,
        },
        cpp_postprocess: {
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
        },
        rust_postprocess: {
            clean_rust: true,
            clean_go: true,
            swift_demangle: true,
            field_offsets: true,
            insert_casts: true,
            arithmetic_idioms: true,
            temp_var_inlining: true,
            stack_var_normalization: true,
            piece_access_normalization: true,
            deref_to_array: true,
            bitop_to_logicop: true,
            remove_dead_branches: true,
            simplify_if: true,
            while_to_for: true,
            dead_assign_removal: true,
            rename_induction_vars: true,
            rename_semantic_vars: true,
            loop_idioms: true,
            switch_reconstruction: true,
            mul_to_shift: true,
            dwarf_names: true,
            string_pointers: true,
        },
        display: {
            max_line_width: 100,
            indent_width: 2,
            integer_format: "best_fit",
            comment_style: "c_style",
            show_casts: true,
            show_namespaces: false,
            show_line_numbers: true,
        },
        performance: {
            timeout_ms: 30000,
            max_function_size: 65536,
            max_instructions: 100000,
            cache_size: 10,
        },
    };
}

// CFG types
export interface CfgNode {
    id: number;
    start_address: string;
    end_address: string;
    instructions: string[];
    is_entry: boolean;
    is_exit: boolean;
}

export interface CfgEdge {
    from: number;
    to: number;
    kind: "unconditional" | "true" | "false";
}

export interface CfgDto {
    function_name: string;
    function_address: string;
    nodes: CfgNode[];
    edges: CfgEdge[];
    /** Number of basic blocks */
    block_count: number;
    /** Number of CFG edges */
    edge_count: number;
    /** McCabe cyclomatic complexity V(G) = E – N + 2 */
    cyclomatic_complexity: number;
}

export interface ListingRow {
    address: string;
    bytes: string;
    mnemonic: string;
    operands: string;
    label: string | null;
    comment: string | null;
    row_type: "instruction" | "label" | "section";
    /** Mnemonic category for syntax highlighting */
    mnemonic_type: "call" | "jmp" | "cjmp" | "ret" | "nop" | "push_pop" | "mov" | "cmp" | "int" | "normal" | "";
}

export interface ListingInfo {
    entry_point: string;
    first_addr: string;
    last_addr: string;
    total_exec_bytes: number;
}

// ──────────────────────────────────────────────── Function Identification ──────

export interface FidMatchDto {
    address: string;
    name: string;
    previous_name: string;
}

export interface FidResultDto {
    matched: number;
    total_scanned: number;
    fidbf_attempted: number;
    fidbf_loaded: number;
    fidbf_failed: number;
    matches: FidMatchDto[];
}

// ──────────────────────────────────────────────────────────────── Debug ──────

export type DebugStatusDto =
    | "detached"
    | "attaching"
    | "running"
    | "suspended"
    | "terminated";

export interface RegisterStateDto {
    rax: number; rbx: number; rcx: number; rdx: number;
    rsi: number; rdi: number; rbp: number; rsp: number;
    r8: number;  r9: number;  r10: number; r11: number;
    r12: number; r13: number; r14: number; r15: number;
    rip: number; rflags: number;
}

export interface BreakpointInfoDto {
    address: string;
    enabled: boolean;
}

export interface DebugStateDto {
    status: DebugStatusDto;
    attached_pid: number | null;
    breakpoints: BreakpointInfoDto[];
    registers: RegisterStateDto | null;
    last_event: string | null;
    events: string[];
}

// ──────────────────────────────────────────────────────── TTD (Time Travel) ──

export interface TtdSnapshotDto {
    step: number;
    thread_id: number;
    rip: string;
    rax: number; rbx: number; rcx: number; rdx: number;
    rsp: number; rbp: number; rsi: number; rdi: number;
    rflags: number;
}

export interface TtdStateDto {
    is_recording: boolean;
    snapshot_count: number;
    /** [min_step, max_step] or null when no snapshots */
    step_range: [number, number] | null;
    current_step: number | null;
    current_snapshot: TtdSnapshotDto | null;
}

// ──────────────────────────────────────────────────────── App-level types ──

/** Represents a single undoable / redoable annotation action */
export interface UndoableAction {
    type: "rename" | "comment";
    address: string;
    previousValue: string;
    newValue: string;
}

/** Serialised project file (*.fprj) */
export interface FissionProject {
    binary_path: string;
    comments: Record<string, string>;
    renames: Record<string, string>;
    bookmarks: BookmarkDto[];
}
