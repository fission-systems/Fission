//! Disassembly Engine using iced-x86
//!
//! Provides fast, accurate x86/x64 disassembly for immediate feedback.

use iced_x86::{Decoder, DecoderOptions, Formatter, Instruction, IntelFormatter, Mnemonic};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DisasmError {
    #[error("Disassembly error: {0}")]
    DisassemblyError(String),
    #[error("Unsupported architecture/mode")]
    UnsupportedArch,
}

/// A single disassembled instruction structure optimized for UI rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        use std::fmt::Write;
        for b in &self.bytes {
            let _ = write!(bytes_str, "{:02X} ", b);
        }
        format!(
            "{:08X} | {:<24} | {:<6} {}",
            self.address, bytes_str, self.mnemonic, self.operands
        )
    }
}

pub struct DisasmEngine {
    bitness: u32,
}

impl DisasmEngine {
    pub fn new(is_64bit: bool) -> Result<Self, DisasmError> {
        let bitness = if is_64bit { 64 } else { 32 };
        Ok(Self { bitness })
    }

    /// Disassemble a byte slice starting at address
    pub fn disassemble(
        &self,
        bytes: &[u8],
        address: u64,
    ) -> Result<Vec<DisassembledInstruction>, DisasmError> {
        let mut decoder = Decoder::with_ip(self.bitness, bytes, address, DecoderOptions::NONE);
        let mut formatter = IntelFormatter::new();

        // Customize formatter for readability
        formatter.options_mut().set_uppercase_mnemonics(false);
        formatter.options_mut().set_uppercase_registers(false);
        formatter
            .options_mut()
            .set_space_after_operand_separator(true);
        formatter.options_mut().set_hex_prefix("0x");
        formatter.options_mut().set_hex_suffix("");

        let estimated_count = bytes.len() / 4;
        let mut results = Vec::with_capacity(estimated_count.max(16));
        let mut instruction = Instruction::default();
        let mut output = String::with_capacity(64);

        let mut offset = 0usize;
        while decoder.can_decode() {
            decoder.decode_out(&mut instruction);

            let insn_len = instruction.len();
            let insn_bytes = if offset + insn_len <= bytes.len() {
                bytes[offset..offset + insn_len].to_vec()
            } else {
                vec![]
            };
            offset += insn_len;

            output.clear();
            formatter.format(&instruction, &mut output);

            let (mnemonic, operands) = if let Some(space_idx) = output.find(' ') {
                (
                    output[..space_idx].to_string(),
                    output[space_idx + 1..].to_string(),
                )
            } else {
                (std::mem::take(&mut output), String::new())
            };

            let is_flow_control = is_flow_control_mnemonic(instruction.mnemonic());

            results.push(DisassembledInstruction {
                address: instruction.ip(),
                bytes: insn_bytes,
                mnemonic,
                operands,
                length: insn_len,
                is_flow_control,
            });
        }

        Ok(results)
    }

    /// Discover call targets by scanning code for CALL instructions
    pub fn discover_call_targets(&self, bytes: &[u8], base_address: u64) -> Vec<u64> {
        use std::collections::HashSet;

        let mut decoder = Decoder::with_ip(self.bitness, bytes, base_address, DecoderOptions::NONE);
        let mut instruction = Instruction::default();
        let mut targets: HashSet<u64> = HashSet::new();

        while decoder.can_decode() {
            decoder.decode_out(&mut instruction);

            if instruction.mnemonic() == Mnemonic::Call && instruction.is_call_near() {
                let target = instruction.near_branch_target();
                if target != 0 && target >= base_address {
                    targets.insert(target);
                }
            }
        }

        let mut result: Vec<u64> = targets.into_iter().collect();
        result.sort();
        result
    }
}

fn is_flow_control_mnemonic(mnemonic: Mnemonic) -> bool {
    use Mnemonic::*;
    matches!(
        mnemonic,
        Jmp | Jo
            | Jno
            | Jb
            | Jae
            | Je
            | Jne
            | Jbe
            | Ja
            | Js
            | Jns
            | Jp
            | Jnp
            | Jl
            | Jge
            | Jle
            | Jg
            | Jcxz
            | Jecxz
            | Jrcxz
            | Call
            | Ret
            | Retf
            | Iret
            | Iretd
            | Iretq
            | Loop
            | Loope
            | Loopne
            | Syscall
            | Sysret
            | Sysenter
            | Sysexit
            | Int
            | Int1
            | Int3
    )
}
