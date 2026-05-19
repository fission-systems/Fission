//! Architecture-aware instruction decoding for the debugger.
//!
//! Provides a unified [`InstructionDecoder`] trait backed by the Ghidra Sleigh
//! engine ([`SleighDecoder`]) with full ISA coverage for all registered
//! architectures (x86, ARM, MIPS, PPC, RISCV, …).

use fission_core::{FissionError, Result as FissionResult};
use fission_sleigh::runtime::{DecodedFlowKind, RuntimeSleighFrontend};

// ---------------------------------------------------------------------------
// Public Types
// ---------------------------------------------------------------------------

/// A single decoded instruction from the debugger's perspective.
#[derive(Debug, Clone)]
pub struct DebugInstruction {
    /// Virtual address of this instruction.
    pub address: u64,
    /// Length in bytes.
    pub length: usize,
    /// Mnemonic (e.g. "call", "mov", "push").
    pub mnemonic: String,
    /// Operand text (e.g. "RAX, qword ptr [RBP + -0x8]").
    pub operands: String,
    /// Raw instruction bytes.
    pub bytes: Vec<u8>,
    /// Is this a CALL instruction?
    pub is_call: bool,
    /// Is this a RET instruction?
    pub is_return: bool,
    /// Is this a branch (JMP / Jcc)?
    pub is_branch: bool,
    /// Is this a conditional branch?
    pub is_conditional: bool,
    /// Direct branch / call target address (if available).
    pub branch_target: Option<u64>,
}

impl DebugInstruction {
    /// Full instruction text ("mnemonic operands").
    pub fn text(&self) -> String {
        if self.operands.is_empty() {
            self.mnemonic.clone()
        } else {
            format!("{} {}", self.mnemonic, self.operands)
        }
    }
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Trait for instruction decoding backends.
///
/// All debugger surfaces consume this trait so that the backend can be
/// swapped transparently.
pub trait InstructionDecoder: Send + Sync {
    /// Decode a single instruction at `address`.
    fn decode_one(&self, bytes: &[u8], address: u64) -> FissionResult<DebugInstruction>;

    /// Decode up to `limit` consecutive instructions starting at `address`.
    fn decode_window(
        &self,
        bytes: &[u8],
        address: u64,
        limit: usize,
    ) -> FissionResult<Vec<DebugInstruction>>;

    /// Human-readable architecture name (e.g. "x86-64").
    fn architecture(&self) -> &str;
}

// ---------------------------------------------------------------------------
// Sleigh-backed decoder (sole implementation)
// ---------------------------------------------------------------------------

/// Instruction decoder powered by the Ghidra Sleigh engine.
///
/// Supports every architecture registered in the Sleigh runtime registry
/// (x86, x86-64, ARM, AARCH64, MIPS, PowerPC, RISCV, SPARC, eBPF, …).
pub struct SleighDecoder {
    frontend: RuntimeSleighFrontend,
    arch_name: String,
}

impl SleighDecoder {
    /// Create a decoder for the given language name (e.g. "x86-64", "ARM8_le").
    pub fn new(language: &str) -> FissionResult<Self> {
        let frontend = RuntimeSleighFrontend::new_for_language(language)
            .map_err(|e| FissionError::debug(format!("Sleigh init for '{}': {}", language, e)))?;
        Ok(Self {
            arch_name: language.to_string(),
            frontend,
        })
    }

    /// Create a decoder from a binary load spec (auto-selects architecture).
    pub fn from_load_spec(
        spec: &fission_core::architecture::BinaryLoadSpec,
    ) -> FissionResult<Self> {
        let frontend = RuntimeSleighFrontend::new_for_load_spec(spec)
            .map_err(|e| FissionError::debug(format!("Sleigh init from load spec: {}", e)))?;
        let arch_name = frontend.language().to_string();
        Ok(Self {
            arch_name,
            frontend,
        })
    }

    /// Access the underlying Sleigh frontend (for p-code lift, etc.).
    pub fn frontend(&self) -> &RuntimeSleighFrontend {
        &self.frontend
    }

    fn convert(insn: &fission_sleigh::runtime::DecodedInstruction) -> DebugInstruction {
        DebugInstruction {
            address: insn.address,
            length: insn.length,
            mnemonic: insn.mnemonic.clone(),
            operands: insn.operands_text.clone(),
            bytes: insn.bytes.clone(),
            is_call: insn.flow_kind == DecodedFlowKind::Call,
            is_return: insn.flow_kind == DecodedFlowKind::Return,
            is_branch: matches!(
                insn.flow_kind,
                DecodedFlowKind::Jump | DecodedFlowKind::ConditionalJump
            ),
            is_conditional: insn.flow_kind == DecodedFlowKind::ConditionalJump,
            branch_target: insn.direct_target,
        }
    }
}

impl InstructionDecoder for SleighDecoder {
    fn decode_one(&self, bytes: &[u8], address: u64) -> FissionResult<DebugInstruction> {
        let decoded = self
            .frontend
            .decode_window(bytes, address, 1)
            .map_err(|e| {
                FissionError::debug(format!("Sleigh decode at 0x{:x}: {}", address, e))
            })?;
        decoded
            .into_iter()
            .next()
            .map(|insn| Self::convert(&insn))
            .ok_or_else(|| {
                FissionError::debug(format!("No instruction decoded at 0x{:016x}", address))
            })
    }

    fn decode_window(
        &self,
        bytes: &[u8],
        address: u64,
        limit: usize,
    ) -> FissionResult<Vec<DebugInstruction>> {
        let decoded = self
            .frontend
            .decode_window(bytes, address, limit)
            .map_err(|e| FissionError::debug(format!("Sleigh decode window: {}", e)))?;
        Ok(decoded.iter().map(Self::convert).collect())
    }

    fn architecture(&self) -> &str {
        &self.arch_name
    }
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// Create a Sleigh-backed instruction decoder for the given architecture.
///
/// This is the canonical entry point. The decoder supports all architectures
/// registered in the Sleigh runtime registry.
pub fn create_decoder(language: &str) -> FissionResult<Box<dyn InstructionDecoder>> {
    let decoder = SleighDecoder::new(language)?;
    Ok(Box::new(decoder))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn try_sleigh() -> Option<SleighDecoder> {
        SleighDecoder::new("x86-64").ok()
    }

    #[test]
    fn sleigh_decoder_detects_call_e8() {
        let Some(decoder) = try_sleigh() else {
            eprintln!("skip: Sleigh x86-64 runtime not available");
            return;
        };
        // E8 rel32 — CALL near
        let bytes = [0xE8, 0x10, 0x00, 0x00, 0x00, 0x90];
        let insn = decoder.decode_one(&bytes, 0x1000).unwrap();
        assert!(insn.is_call, "E8 should be detected as CALL: {:?}", insn);
        assert_eq!(insn.length, 5);
        assert_eq!(insn.mnemonic.to_lowercase(), "call");
    }

    #[test]
    fn sleigh_decoder_detects_ret() {
        let Some(decoder) = try_sleigh() else {
            eprintln!("skip: Sleigh x86-64 runtime not available");
            return;
        };
        let insn = decoder.decode_one(&[0xC3], 0x1000).unwrap();
        assert!(insn.is_return, "C3 should be RET: {:?}", insn);
        assert_eq!(insn.length, 1);
    }

    #[test]
    fn sleigh_decoder_detects_conditional_branch() {
        let Some(decoder) = try_sleigh() else {
            eprintln!("skip: Sleigh x86-64 runtime not available");
            return;
        };
        // 74 05 — JE +5
        let bytes = [0x74, 0x05, 0x90, 0x90, 0x90, 0x90, 0x90];
        let insn = decoder.decode_one(&bytes, 0x1000).unwrap();
        assert!(insn.is_branch, "JE should be a branch: {:?}", insn);
        assert!(insn.is_conditional, "JE should be conditional: {:?}", insn);
        assert!(!insn.is_call);
    }

    #[test]
    fn sleigh_decoder_decode_window_multiple() {
        let Some(decoder) = try_sleigh() else {
            eprintln!("skip: Sleigh x86-64 runtime not available");
            return;
        };
        // NOP + NOP + RET
        let bytes = [0x90, 0x90, 0xC3];
        let insns = decoder.decode_window(&bytes, 0x1000, 10).unwrap();
        assert_eq!(insns.len(), 3);
        assert_eq!(insns[0].address, 0x1000);
        assert_eq!(insns[1].address, 0x1001);
        assert_eq!(insns[2].address, 0x1002);
        assert!(insns[2].is_return);
    }

    #[test]
    fn sleigh_decoder_call_rax_ff_d0() {
        let Some(decoder) = try_sleigh() else {
            eprintln!("skip: Sleigh x86-64 runtime not available");
            return;
        };
        // FF D0 — CALL RAX
        let bytes = [0xFF, 0xD0, 0x90];
        let insn = decoder.decode_one(&bytes, 0x1000).unwrap();
        assert!(insn.is_call, "FF D0 should be indirect CALL: {:?}", insn);
        assert_eq!(insn.length, 2);
    }

    #[test]
    fn sleigh_decoder_jmp_not_call() {
        let Some(decoder) = try_sleigh() else {
            eprintln!("skip: Sleigh x86-64 runtime not available");
            return;
        };
        // E9 rel32 — JMP near
        let bytes = [0xE9, 0x10, 0x00, 0x00, 0x00];
        let insn = decoder.decode_one(&bytes, 0x1000).unwrap();
        assert!(!insn.is_call, "E9 JMP should not be CALL: {:?}", insn);
        assert!(insn.is_branch, "E9 JMP should be a branch: {:?}", insn);
    }

    #[test]
    fn create_decoder_succeeds_for_x86_64() {
        let decoder = create_decoder("x86-64");
        // May fail if Sleigh .sla files are not available in the test environment
        if let Ok(d) = decoder {
            assert_eq!(d.architecture(), "x86-64");
        } else {
            eprintln!("skip: Sleigh x86-64 runtime not available for create_decoder");
        }
    }

    #[test]
    fn debug_instruction_text_formatting() {
        let insn = DebugInstruction {
            address: 0x1000,
            length: 5,
            mnemonic: "call".into(),
            operands: "0x2000".into(),
            bytes: vec![0xE8, 0x00, 0x10, 0x00, 0x00],
            is_call: true,
            is_return: false,
            is_branch: false,
            is_conditional: false,
            branch_target: Some(0x2000),
        };
        assert_eq!(insn.text(), "call 0x2000");

        let insn2 = DebugInstruction {
            mnemonic: "ret".into(),
            operands: String::new(),
            ..insn
        };
        assert_eq!(insn2.text(), "ret");
    }
}
