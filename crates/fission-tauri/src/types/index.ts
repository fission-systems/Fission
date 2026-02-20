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
}

export interface StringDto {
    offset: string;
    value: string;
    encoding: string;
}

// Editor tab model
export interface EditorTab {
    id: string;
    title: string;
    type: "decompile" | "assembly";
    address: string;
    functionName: string;
}

// Activity bar item
export type ActivityView = "explorer" | "search" | "debug";

// Bottom panel tab
export type BottomTab = "console" | "strings";
