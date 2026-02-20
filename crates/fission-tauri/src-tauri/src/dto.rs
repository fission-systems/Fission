//! Fission Tauri — DTO types for JSON serialization over Tauri IPC.

use serde::Serialize;

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
}

/// Extracted string from binary.
#[derive(Debug, Clone, Serialize)]
pub struct StringDto {
    pub offset: String,
    pub value: String,
    pub encoding: String,
}
