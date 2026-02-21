// Fission Tauri - Type Definitions
// Mirrors the Rust DTO types from src-tauri/src/dto.rs

export interface BinaryInfo {
    name: string;
    path: string;
    arch: string;
    format: string;
    entry_point: string;
    section_count: number;
    function_count: number;
    image_base: string;
}

export interface FunctionDto {
    address: string;
    name: string;
    size: number;
    category: "import" | "export" | "internal";
}

export interface DecompileResult {
    code: string;
    function_name: string;
    address: string;
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
export type ActivityView = "explorer" | "search" | "debug" | "settings";

// Bottom panel tab
export type BottomTab = "console" | "strings" | "hex" | "imports" | "bookmarks" | "xrefs" | "search" | "cfg" | "debug" | "string-xrefs";

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
}

export interface ListingRow {
    address: string;
    bytes: string;
    mnemonic: string;
    operands: string;
    label: string | null;
    comment: string | null;
    row_type: "instruction" | "label" | "section";
}

export interface ListingInfo {
    entry_point: string;
    first_addr: string;
    last_addr: string;
    total_exec_bytes: number;
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
