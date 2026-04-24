use anyhow::Result;
use fission_pcode::PcodeOp;

use crate::compiler::{CompiledFrontend, EntrySpec};
use crate::runtime::spine::compiled_table;
use crate::runtime::{
    registry, DecodedInstruction, ExecutionEngineKey, RuntimeExecutionDetails, RuntimeSleighError,
};

pub(crate) fn decode_and_lift_with_details(
    entry: &EntrySpec,
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Result<(Vec<PcodeOp>, u64, RuntimeExecutionDetails)> {
    match registry::executable_engine_key_for_entry(entry) {
        Some(ExecutionEngineKey::CompiledTable) => {
            compiled_table::decode_and_lift_with_details(compiled, bytes, address)
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
            compiled_table::decode_instruction(compiled, bytes, address)
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
