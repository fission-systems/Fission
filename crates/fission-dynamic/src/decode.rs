//! Architecture-aware instruction decoding for the debugger.
//!
//! Provides a unified [`InstructionDecoder`] trait backed by:
//!
//! - **[`SleighDecoder`]** (`sleigh_decode` feature): Ghidra Sleigh engine with
//!   full ISA coverage for all registered architectures (x86, ARM, MIPS, PPC, …).
//! - **[`FallbackX86Decoder`]**: Minimal x86-64 CALL detection from
//!   [`crate::x86_decode`]. Always available; used when Sleigh is not compiled in
//!   or fails to initialise.

use fission_core::{FissionError, Result as FissionResult};

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
/// All debugger surfaces consume this trait so that the backend (Sleigh vs.
/// fallback) can be swapped transparently.
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
// Sleigh-backed decoder
// ---------------------------------------------------------------------------

#[cfg(feature = "sleigh_decode")]
mod sleigh_impl {
    use super::*;
    use fission_sleigh::runtime::{DecodedFlowKind, RuntimeSleighFrontend};

    /// Instruction decoder powered by the Ghidra Sleigh engine.
    ///
    /// Supports every architecture registered in the Sleigh runtime registry.
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

        fn convert(
            insn: &fission_sleigh::runtime::DecodedInstruction,
        ) -> DebugInstruction {
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
                .map_err(|e| FissionError::debug(format!("Sleigh decode at 0x{:x}: {}", address, e)))?;
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
}

#[cfg(feature = "sleigh_decode")]
pub use sleigh_impl::SleighDecoder;

// ---------------------------------------------------------------------------
// Fallback x86-only decoder
// ---------------------------------------------------------------------------

/// Minimal x86-64 instruction decoder using byte-pattern matching.
///
/// Only detects CALL and RET instructions (for step-over). Does not provide
/// full disassembly text. Used when the `sleigh_decode` feature is disabled.
pub struct FallbackX86Decoder;

impl InstructionDecoder for FallbackX86Decoder {
    fn decode_one(&self, bytes: &[u8], address: u64) -> FissionResult<DebugInstruction> {
        if bytes.is_empty() {
            return Err(FissionError::debug("Empty bytes for decode"));
        }
        let (is_call, len) = crate::x86_decode::detect_call_instruction(bytes);
        let length = if len > 0 { len } else { 1 };
        // Detect RET (C3, C2, CB, CA)
        let is_return = matches!(bytes[0], 0xC3 | 0xC2 | 0xCB | 0xCA);
        Ok(DebugInstruction {
            address,
            length,
            mnemonic: if is_call {
                "call".into()
            } else if is_return {
                "ret".into()
            } else {
                "??".into()
            },
            operands: String::new(),
            bytes: bytes[..length.min(bytes.len())].to_vec(),
            is_call,
            is_return,
            is_branch: false,
            is_conditional: false,
            branch_target: None,
        })
    }

    fn decode_window(
        &self,
        bytes: &[u8],
        address: u64,
        limit: usize,
    ) -> FissionResult<Vec<DebugInstruction>> {
        let mut result = Vec::with_capacity(limit);
        let mut offset = 0;
        let mut addr = address;
        while offset < bytes.len() && result.len() < limit {
            let insn = self.decode_one(&bytes[offset..], addr)?;
            let step = insn.length;
            result.push(insn);
            offset += step;
            addr += step as u64;
        }
        Ok(result)
    }

    fn architecture(&self) -> &str {
        "x86-64 (fallback)"
    }
}

// ---------------------------------------------------------------------------
// Factory
// ---------------------------------------------------------------------------

/// Create the best available instruction decoder for the given architecture.
///
/// When the `sleigh_decode` feature is enabled, returns a [`SleighDecoder`].
/// Otherwise falls back to [`FallbackX86Decoder`].
pub fn create_decoder(language: &str) -> FissionResult<Box<dyn InstructionDecoder>> {
    #[cfg(feature = "sleigh_decode")]
    {
        match SleighDecoder::new(language) {
            Ok(d) => return Ok(Box::new(d)),
            Err(e) => {
                tracing::warn!(
                    "Sleigh decoder init failed for '{}': {}; falling back to x86",
                    language,
                    e
                );
            }
        }
    }
    let _ = language; // suppress unused warning without sleigh_decode
    Ok(Box::new(FallbackX86Decoder))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_detects_e8_call() {
        let decoder = FallbackX86Decoder;
        let bytes = [0xE8, 0x10, 0x00, 0x00, 0x00, 0x90];
        let insn = decoder.decode_one(&bytes, 0x1000).unwrap();
        assert!(insn.is_call);
        assert_eq!(insn.length, 5);
        assert_eq!(insn.mnemonic, "call");
    }

    #[test]
    fn fallback_detects_ret() {
        let decoder = FallbackX86Decoder;
        let insn = decoder.decode_one(&[0xC3], 0x1000).unwrap();
        assert!(insn.is_return);
        assert!(!insn.is_call);
        assert_eq!(insn.mnemonic, "ret");
    }

    #[test]
    fn fallback_decode_window_sequences() {
        let decoder = FallbackX86Decoder;
        // E8 rel32 (5 bytes) + C3 (1 byte)
        let bytes = [0xE8, 0x10, 0x00, 0x00, 0x00, 0xC3];
        let insns = decoder.decode_window(&bytes, 0x1000, 10).unwrap();
        assert_eq!(insns.len(), 2);
        assert!(insns[0].is_call);
        assert_eq!(insns[0].address, 0x1000);
        assert!(insns[1].is_return);
        assert_eq!(insns[1].address, 0x1005);
    }

    #[test]
    fn create_decoder_returns_fallback_without_sleigh() {
        // Without sleigh_decode feature, always returns fallback
        let decoder = create_decoder("x86-64").unwrap();
        // Should at least be able to decode something
        let insn = decoder.decode_one(&[0xC3], 0x1000).unwrap();
        assert!(insn.is_return);
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
