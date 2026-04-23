pub mod x86;

use anyhow::Result;
use fission_pcode::PcodeOp;

use crate::compiler::{CompiledFrontend, EntrySpec};
use crate::runtime::{registry, DecodedInstruction, ExecutionProviderKey, RuntimeSleighError};

pub(crate) fn decode_and_lift(
    entry: &EntrySpec,
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Result<(Vec<PcodeOp>, u64)> {
    match registry::executable_provider_key_for_entry(entry) {
        Some(ExecutionProviderKey::X86_64Generated) if x86::supports_entry(entry) => {
            x86::decode_and_lift(compiled, bytes, address)
        }
        _ => Err(RuntimeSleighError::UnsupportedPcodeTemplate {
            language: entry.entry_id.clone(),
            reason: format!(
                "{} runtime status is executable, but no processor execution engine is registered",
                entry.entry_id
            ),
        }
        .into()),
    }
}

pub(crate) fn decode_instruction(
    entry: &EntrySpec,
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Result<DecodedInstruction> {
    match registry::executable_provider_key_for_entry(entry) {
        Some(ExecutionProviderKey::X86_64Generated) if x86::supports_entry(entry) => {
            x86::decode_instruction(compiled, bytes, address)
        }
        _ => Err(RuntimeSleighError::UnsupportedPcodeTemplate {
            language: entry.entry_id.clone(),
            reason: format!(
                "{} runtime status is executable, but no processor execution engine is registered",
                entry.entry_id
            ),
        }
        .into()),
    }
}
