use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

mod aarch64;
mod common;
mod x86;

use common::UNIQUE_SPACE_ID;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchKind {
    Aarch64,
    X86,
}

#[derive(Debug, Clone)]
pub struct SleighLifter {
    arch: ArchKind,
}

impl SleighLifter {
    pub fn spec_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("specs/languages")
    }

    pub fn spec_path_for(language_name: &str) -> PathBuf {
        Self::spec_dir().join(format!("{}.slaspec", language_name))
    }

    pub fn new_for_language(language_name: &str) -> Result<Self> {
        let spec_path = Self::spec_path_for(language_name);
        if !spec_path.exists() {
            bail!(
                "Sleigh spec not found for language '{}': {}",
                language_name,
                spec_path.display()
            );
        }

        let arch = if language_name.starts_with("AARCH64") {
            ArchKind::Aarch64
        } else {
            // Keep x86-family as the default fallback path for now.
            ArchKind::X86
        };

        Ok(Self { arch })
    }

    pub fn new(spec_path: &Path) -> Result<Self> {
        let language = spec_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| anyhow!("Invalid Sleigh spec path: {}", spec_path.display()))?;
        Self::new_for_language(language)
    }

    pub fn decode_and_lift(&self, bytes: &[u8], address: u64) -> Result<Vec<PcodeOp>> {
        let (ops, _) = self.decode_and_lift_with_len(bytes, address)?;
        Ok(ops)
    }

    pub fn decode_and_lift_with_len(&self, bytes: &[u8], address: u64) -> Result<(Vec<PcodeOp>, u64)> {
        if bytes.is_empty() {
            bail!("No instruction bytes available at 0x{:x}", address);
        }

        let decoded_len = self.decode_len(bytes)?;
        let decoded_len_usize = usize::try_from(decoded_len).context("decoded_len does not fit usize")?;
        let insn = &bytes[..decoded_len_usize];

        let mut ops = Vec::with_capacity(8);
        ops.push(self.emit_trace_copy(insn, address));
        match self.arch {
            ArchKind::Aarch64 => {
                let mut sem = aarch64::decode_semantic(insn, address);
                let has_cf = sem.iter().any(|op| {
                    matches!(
                        op.opcode,
                        PcodeOpcode::Branch
                            | PcodeOpcode::CBranch
                            | PcodeOpcode::BranchInd
                            | PcodeOpcode::Return
                            | PcodeOpcode::Call
                            | PcodeOpcode::CallInd
                    )
                });
                ops.append(&mut sem);
                if !has_cf {
                    if let Some(mut flow) = self.decode_control_flow(insn, address, decoded_len)? {
                        ops.append(&mut flow);
                    }
                }
            }
            ArchKind::X86 => {
                if let Some(mut flow) = self.decode_control_flow(insn, address, decoded_len)? {
                    ops.append(&mut flow);
                }
            }
        }

        Ok((ops, decoded_len))
    }

    fn decode_len(&self, bytes: &[u8]) -> Result<u64> {
        match self.arch {
            ArchKind::Aarch64 => {
                if bytes.len() < 4 {
                    bail!("AArch64 needs 4 bytes, got {}", bytes.len());
                }
                Ok(4)
            }
            ArchKind::X86 => x86::decode_len(bytes),
        }
    }

    fn emit_trace_copy(&self, insn: &[u8], address: u64) -> PcodeOp {
        let mut raw = 0u64;
        for (idx, b) in insn.iter().take(8).enumerate() {
            raw |= (*b as u64) << (idx * 8);
        }

        let const_raw = if raw > i64::MAX as u64 {
            i64::MAX
        } else {
            raw as i64
        };

        PcodeOp {
            seq_num: 0,
            opcode: PcodeOpcode::Copy,
            address,
            output: Some(Varnode {
                space_id: UNIQUE_SPACE_ID,
                offset: 0x7000_0000_0000_0000u64.wrapping_add(address),
                size: 8,
                is_constant: false,
                constant_val: 0,
            }),
            inputs: vec![Varnode::constant(const_raw, 8)],
            asm_mnemonic: Some("INSN_RAW".to_string()),
        }
    }

    fn decode_control_flow(&self, insn: &[u8], address: u64, decoded_len: u64) -> Result<Option<Vec<PcodeOp>>> {
        match self.arch {
            ArchKind::Aarch64 => Ok(aarch64::decode_control(insn, address)),
            ArchKind::X86 => Ok(x86::decode_control(insn, address, decoded_len).map(|op| vec![op])),
        }
    }
}
