mod x86_64;

use anyhow::Result;
use fission_pcode::PcodeOp;

use crate::compiler::{CompiledFrontend, EntrySpec};
use crate::runtime::{DecodedInstruction, ExecutionProviderKey, RuntimeSleighError};

pub(crate) trait ExecutionProvider {
    fn key(&self) -> ExecutionProviderKey;
    fn supports(&self, entry: &EntrySpec, compiled: &CompiledFrontend) -> bool;
    fn decode_and_lift(
        &self,
        entry: &EntrySpec,
        compiled: &CompiledFrontend,
        bytes: &[u8],
        address: u64,
    ) -> Result<(Vec<PcodeOp>, u64)>;
    fn decode_instruction(
        &self,
        entry: &EntrySpec,
        compiled: &CompiledFrontend,
        bytes: &[u8],
        address: u64,
    ) -> Result<DecodedInstruction>;
}

pub(crate) fn provider_for_key(key: ExecutionProviderKey) -> &'static dyn ExecutionProvider {
    match key {
        ExecutionProviderKey::X86_64Generated => &x86_64::X86_64_GENERATED_PROVIDER,
    }
}

pub(crate) fn decode_and_lift(
    entry: &EntrySpec,
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Result<(Vec<PcodeOp>, u64)> {
    let Some(key) = crate::runtime::registry::executable_provider_key_for_entry(entry) else {
        return Err(RuntimeSleighError::UnsupportedPcodeTemplate {
            language: entry.entry_id.clone(),
            reason: format!(
                "{} runtime status is executable, but no execution provider key is registered",
                entry.entry_id
            ),
        }
        .into());
    };

    let provider = provider_for_key(key);
    if !provider.supports(entry, compiled) {
        return Err(RuntimeSleighError::UnsupportedPcodeTemplate {
            language: entry.entry_id.clone(),
            reason: format!(
                "{} runtime provider {:?} does not support this entry",
                entry.entry_id,
                provider.key()
            ),
        }
        .into());
    }
    provider.decode_and_lift(entry, compiled, bytes, address)
}

pub(crate) fn decode_instruction(
    entry: &EntrySpec,
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Result<DecodedInstruction> {
    let Some(key) = crate::runtime::registry::executable_provider_key_for_entry(entry) else {
        return Err(RuntimeSleighError::UnsupportedPcodeTemplate {
            language: entry.entry_id.clone(),
            reason: format!(
                "{} runtime status is executable, but no execution provider key is registered",
                entry.entry_id
            ),
        }
        .into());
    };

    let provider = provider_for_key(key);
    if !provider.supports(entry, compiled) {
        return Err(RuntimeSleighError::UnsupportedPcodeTemplate {
            language: entry.entry_id.clone(),
            reason: format!(
                "{} runtime provider {:?} does not support this entry",
                entry.entry_id,
                provider.key()
            ),
        }
        .into());
    }
    provider.decode_instruction(entry, compiled, bytes, address)
}
