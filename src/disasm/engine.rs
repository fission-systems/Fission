//! Disassembly Engine - Ghidra Sleigh Backend
//!
//! Provides high-level disassembly interface using Ghidra Sleigh.
//! This replaces the previous iced-x86 based implementation.

use thiserror::Error;
use crate::decomp::{Decompiler, engine::StubDecompiler};

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
    Intel, // mov eax, [ebx+4]
    Masm,  // mov eax, DWORD PTR [ebx+4]
    Nasm,  // mov eax, dword [ebx+4]
    Gas,   // movl 4(%ebx), %eax
}

/// Target architecture bitness
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Bitness {
    Bit16 = 16,
    Bit32 = 32,
    #[default]
    Bit64 = 64,
}

/// A single disassembled instruction
#[derive(Debug, Clone)]
pub struct DisassembledInstruction {
    /// Address of the instruction
    pub address: u64,

    /// Raw bytes of the instruction
    pub bytes: Vec<u8>,

    /// Formatted instruction string
    pub mnemonic: String,

    /// Length of the instruction in bytes
    pub length: usize,

    /// Whether this is a branch instruction
    pub is_branch: bool,

    /// Whether this is a call instruction
    pub is_call: bool,

    /// Whether this is a return instruction
    pub is_ret: bool,

    /// Branch/call target address (if applicable)
    pub target_address: Option<u64>,
}

/// Main disassembly engine using Ghidra Sleigh
pub struct DisassemblyEngine {
    /// Target architecture bitness
    bitness: Bitness,

    /// Output syntax format
    syntax: SyntaxFormat,

    /// Stub decompiler for when native isn't available
    stub: StubDecompiler,
}

impl DisassemblyEngine {
    /// Create a new disassembly engine with default settings (64-bit, Intel syntax)
    pub fn new() -> Self {
        Self {
            bitness: Bitness::Bit64,
            syntax: SyntaxFormat::Intel,
            stub: StubDecompiler::new(),
        }
    }

    /// Create a new engine with specified bitness
    pub fn with_bitness(bitness: Bitness) -> Self {
        Self {
            bitness,
            syntax: SyntaxFormat::Intel,
            stub: StubDecompiler::new(),
        }
    }

    /// Set the output syntax format
    pub fn set_syntax(&mut self, syntax: SyntaxFormat) {
        self.syntax = syntax;
    }

    /// Set the target architecture bitness
    pub fn set_bitness(&mut self, bitness: Bitness) {
        self.bitness = bitness;
    }

    /// Disassemble a buffer of bytes
    /// 
    /// Note: This uses Ghidra Sleigh when available, otherwise returns stub output
    pub fn disassemble(&self, bytes: &[u8], base_address: u64) -> Vec<DisassembledInstruction> {
        // Try native Ghidra disassembler first
        if let Ok(decomp) = Decompiler::with_default_path() {
            if let Ok(result) = decomp.disassemble(bytes, base_address) {
                return self.parse_disassembly_text(&result.text, bytes, base_address);
            }
        }

        // Fallback to stub output
        self.disassemble_stub(bytes, base_address)
    }

    /// Parse Ghidra disassembly text output into structured instructions
    fn parse_disassembly_text(
        &self,
        text: &str,
        bytes: &[u8],
        base_address: u64,
    ) -> Vec<DisassembledInstruction> {
        let mut instructions = Vec::new();

        for line in text.lines() {
            // Parse lines like "401000:  push rbp"
            if let Some((addr_str, mnemonic)) = line.split_once(':') {
                if let Ok(address) = u64::from_str_radix(addr_str.trim(), 16) {
                    let mnemonic = mnemonic.trim().to_string();
                    let lower = mnemonic.to_lowercase();

                    let is_branch = lower.starts_with('j') && !lower.starts_with("jmp");
                    let is_call = lower.starts_with("call");
                    let is_ret = lower.starts_with("ret");

                    // Extract target address from branch/call instructions
                    let target_address = if is_branch || is_call || lower.starts_with("jmp") {
                        Self::extract_target_address(&mnemonic)
                    } else {
                        None
                    };

                    instructions.push(DisassembledInstruction {
                        address,
                        bytes: vec![], // Ghidra text output doesn't include bytes
                        mnemonic,
                        length: 0, // Unknown from text
                        is_branch,
                        is_call,
                        is_ret,
                        target_address,
                    });
                }
            }
        }

        instructions
    }

    /// Extract target address from instruction operand
    fn extract_target_address(mnemonic: &str) -> Option<u64> {
        // Look for hex address in operand
        let parts: Vec<&str> = mnemonic.split_whitespace().collect();
        if parts.len() >= 2 {
            let operand = parts[1];
            // Handle formats like "0x401000" or "loc_401000"
            if operand.starts_with("0x") {
                return u64::from_str_radix(&operand[2..], 16).ok();
            } else if operand.starts_with("loc_") {
                return u64::from_str_radix(&operand[4..], 16).ok();
            } else {
                // Try parsing as plain hex
                return u64::from_str_radix(operand, 16).ok();
            }
        }
        None
    }

    /// Generate stub disassembly when native engine unavailable
    fn disassemble_stub(&self, bytes: &[u8], base_address: u64) -> Vec<DisassembledInstruction> {
        let result = self.stub.disassemble(bytes, base_address);
        
        // Parse stub output into instructions (mostly just raw bytes)
        let mut instructions = Vec::new();
        
        for (i, byte) in bytes.iter().enumerate() {
            instructions.push(DisassembledInstruction {
                address: base_address + i as u64,
                bytes: vec![*byte],
                mnemonic: format!("db 0x{:02x}", byte),
                length: 1,
                is_branch: false,
                is_call: false,
                is_ret: false,
                target_address: None,
            });
        }

        instructions
    }

    /// Disassemble a single instruction at the given address
    pub fn disassemble_one(&self, bytes: &[u8], address: u64) -> Option<DisassembledInstruction> {
        let instructions = self.disassemble(bytes, address);
        instructions.into_iter().next()
    }

    /// Format instructions as a string table (for display)
    pub fn format_listing(&self, instructions: &[DisassembledInstruction]) -> String {
        let mut output = String::new();

        for inst in instructions {
            // Format: ADDRESS  BYTES                 INSTRUCTION
            let bytes_str: String = inst
                .bytes
                .iter()
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(" ");

            output.push_str(&format!(
                "{:016X}  {:24}  {}\n",
                inst.address, bytes_str, inst.mnemonic
            ));
        }

        output
    }
}

impl Default for DisassemblyEngine {
    fn default() -> Self {
        Self::new()
    }
}
