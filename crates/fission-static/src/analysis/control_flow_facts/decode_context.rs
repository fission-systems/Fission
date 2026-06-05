//! Shared decode-memory-context helper used by decompiler and benchmark harnesses.

use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::DecodeMemoryContext;

use super::control_flow_facts_for;

/// Build the production `DecodeMemoryContext` slice for one function entry.
pub fn decode_memory_context_for(
    binary: &LoadedBinary,
    entry_address: u64,
    max_bytes: usize,
) -> DecodeMemoryContext {
    let facts = control_flow_facts_for(binary);
    facts.decode_context_for(binary, entry_address, max_bytes)
}

/// Compute a conservative byte window for `entry_address`.
pub fn function_max_bytes(binary: &LoadedBinary, entry_address: u64, fallback: usize) -> usize {
    let inner = binary.inner();
    if let Some(&idx) = inner.function_addr_index.get(&entry_address) {
        if let Some(info) = inner.functions.get(idx) {
            if info.size > 0 {
                return info.size as usize;
            }
        }
    }

    if let Some(end) = control_flow_facts_for(binary)
        .function_extents
        .get(&entry_address)
        .copied()
    {
        if end > entry_address {
            return end.saturating_sub(entry_address) as usize;
        }
    }

    let mut next = entry_address.saturating_add(fallback as u64);
    for info in &inner.functions {
        if info.address > entry_address && info.address < next {
            next = info.address;
        }
    }
    next.saturating_sub(entry_address) as usize
}
