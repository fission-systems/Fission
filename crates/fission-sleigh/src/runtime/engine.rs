use anyhow::Result;
use fission_pcode::PcodeOp;

use crate::compiler::{CompiledFrontend, EntrySpec};
use crate::runtime::{registry, DecodedInstruction, ExecutionEngineKey, RuntimeSleighError};

#[path = "processors/x86/generated.rs"]
mod generated_runtime_holdout;

pub(crate) fn decode_and_lift(
    entry: &EntrySpec,
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Result<(Vec<PcodeOp>, u64)> {
    match registry::executable_engine_key_for_entry(entry) {
        Some(ExecutionEngineKey::CompiledTable) => {
            generated_runtime_holdout::decode_and_lift(compiled, bytes, address)
        }
        _ => Err(RuntimeSleighError::UnsupportedPcodeTemplate {
            language: entry.entry_id.clone(),
            reason: format!(
                "{} runtime status is executable, but no shared execution engine is registered",
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
    match registry::executable_engine_key_for_entry(entry) {
        Some(ExecutionEngineKey::CompiledTable) => {
            generated_runtime_holdout::decode_instruction(compiled, bytes, address)
        }
        _ => Err(RuntimeSleighError::UnsupportedPcodeTemplate {
            language: entry.entry_id.clone(),
            reason: format!(
                "{} runtime status is executable, but no shared execution engine is registered",
                entry.entry_id
            ),
        }
        .into()),
    }
}
