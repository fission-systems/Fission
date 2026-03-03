/// Address type used throughout Fission
pub type Address = u64;

/// Architecture enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Architecture {
    X86,
    X64,
    Arm,
    Arm64,
    Unknown,
}

/// Metadata about a binary
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BinaryInfo {
    pub path: String,
    pub filename: String,
    pub architecture: Architecture,
    pub base_address: Address,
    pub entry_point: Address,
    pub size: usize,
    pub md5: String,
    pub sha256: String,
}

/// Metadata about a function (used in analysis/patching context)
/// Note: This is different from common::types::FunctionInfo which is used for loading
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FunctionMetadata {
    pub name: String,
    pub start_address: Address,
    pub end_address: Address,
    pub size: usize,
    pub is_imported: bool,
    pub is_exported: bool,
    pub library: Option<String>,
    pub signature: Option<String>,
}

/// Common patch types for quick application
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum QuickPatch {
    /// NOP instruction (0x90)
    Nop,
    /// JE to JNE (0x74 -> 0x75)
    JeToJne,
    /// JNE to JE (0x75 -> 0x74)
    JneToJe,
    /// Short JMP (0xEB)
    JmpShort,
    /// Return (0xC3)
    Ret,
    /// JZ to JMP (0x74 XX -> 0xEB XX)
    JzToJmp,
}

impl QuickPatch {
    /// Get the bytes for this quick patch
    pub fn bytes(&self) -> Vec<u8> {
        match self {
            QuickPatch::Nop => vec![0x90],
            QuickPatch::JeToJne => vec![0x75],
            QuickPatch::JneToJe => vec![0x74],
            QuickPatch::JmpShort => vec![0xEB],
            QuickPatch::Ret => vec![0xC3],
            QuickPatch::JzToJmp => vec![0xEB],
        }
    }

    /// Get description of this quick patch
    pub fn description(&self) -> &'static str {
        match self {
            QuickPatch::Nop => "NOP (No Operation)",
            QuickPatch::JeToJne => "JE → JNE (Invert Jump if Equal)",
            QuickPatch::JneToJe => "JNE → JE (Invert Jump if Not Equal)",
            QuickPatch::JmpShort => "JMP (Unconditional Short Jump)",
            QuickPatch::Ret => "RET (Return from Function)",
            QuickPatch::JzToJmp => "JZ → JMP (Always Jump)",
        }
    }
}
