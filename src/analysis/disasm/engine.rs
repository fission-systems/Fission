//! Disassembly Engine using Capstone
//!
//! Provides local disassembly capabilities for immediate feedback.

use thiserror::Error;
use capstone::prelude::*;

#[derive(Error, Debug)]
pub enum DisasmError {
    #[error("Capstone error: {0}")]
    CapstoneError(String),
    #[error("Unsupported architecture/mode")]
    UnsupportedArch,
}

impl From<capstone::Error> for DisasmError {
    fn from(err: capstone::Error) -> Self {
        DisasmError::CapstoneError(err.to_string())
    }
}

/// A single disassembled instruction structure optimized for UI rendering
#[derive(Debug, Clone)]
pub struct DisassembledInstruction {
    pub address: u64,
    pub bytes: Vec<u8>,
    pub mnemonic: String,
    pub operands: String,
    pub length: usize,
    /// Is this a jump/call/ret instruction?
    pub is_flow_control: bool,
}

impl DisassembledInstruction {
    /// Format detailed string with bytes
    pub fn format_full(&self) -> String {
        let mut bytes_str = String::new();
        for b in &self.bytes {
            use std::fmt::Write;
            write!(bytes_str, "{:02X} ", b).unwrap();
        }
        format!(
            "{:08X} | {:<24} | {:<6} {}",
            self.address, bytes_str, self.mnemonic, self.operands
        )
    }
}

pub struct DisasmEngine {
    cs: Capstone,
}

impl DisasmEngine {
    pub fn new(is_64bit: bool) -> Result<Self, DisasmError> {
        let mode = if is_64bit {
            capstone::arch::x86::ArchMode::Mode64
        } else {
            capstone::arch::x86::ArchMode::Mode32
        };

        let mut cs = Capstone::new()
            .x86()
            .mode(mode)
            .detail(true)
            .build()?;
            
        // Enable SKIPDATA to handle invalid bytes gracefully
        cs.set_skipdata(true)?;

        Ok(Self { cs })
    }

    /// Disassemble a byte slice starting at address
    pub fn disassemble(&self, bytes: &[u8], address: u64) -> Result<Vec<DisassembledInstruction>, DisasmError> {
        let insns = self.cs.disasm_all(bytes, address)?;
        
        let result = insns.iter().map(|insn| {
            let is_flow_control = if let Ok(detail) = self.cs.insn_detail(&insn) {
                let groups = detail.groups();
                groups.iter().any(|g| {
                    let g_u8: u8 = g.0;
                    g_u8 == capstone::InsnGroupType::CS_GRP_JUMP as u8 || 
                    g_u8 == capstone::InsnGroupType::CS_GRP_CALL as u8 || 
                    g_u8 == capstone::InsnGroupType::CS_GRP_RET as u8
                })
            } else {
                // Fallback heuristic if detail fails
                let m = insn.mnemonic().unwrap_or("");
                m.starts_with('j') || m.starts_with("call") || m.starts_with("ret")
            };

            DisassembledInstruction {
                address: insn.address(),
                bytes: insn.bytes().to_vec(),
                mnemonic: insn.mnemonic().unwrap_or("???").to_string(),
                operands: insn.op_str().unwrap_or("").to_string(),
                length: insn.len(),
                is_flow_control,
            }
        }).collect();

        Ok(result)
    }
}
