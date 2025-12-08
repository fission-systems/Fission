//! Disassembly Engine
//!
//! In gRPC architecture, disassembly is provided by the Ghidra server.
//! This module provides local types and utilities for working with
//! disassembly data received from the server.

use thiserror::Error;

/// Disassembly errors
#[derive(Error, Debug)]
pub enum DisasmError {
    #[error("Invalid instruction at offset {offset:#x}")]
    InvalidInstruction { offset: u64 },

    #[error("Buffer too small")]
    BufferTooSmall,

    #[error("Unsupported architecture: {0}")]
    UnsupportedArch(String),

    #[error("Disassembly failed: {0}")]
    DisassemblyFailed(String),
}

/// Disassembly output format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyntaxFormat {
    #[default]
    Intel,
    Masm,
    Att,
}

/// CPU bitness
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Bitness {
    Bits16,
    Bits32,
    #[default]
    Bits64,
}

/// A single disassembled instruction
#[derive(Debug, Clone)]
pub struct DisassembledInstruction {
    pub address: u64,
    pub bytes: Vec<u8>,
    pub mnemonic: String,
    pub operands: String,
    pub length: usize,
}

impl DisassembledInstruction {
    /// Format instruction for display
    pub fn format(&self) -> String {
        format!("{:016x}  {}  {}", self.address, self.mnemonic, self.operands)
    }
}
