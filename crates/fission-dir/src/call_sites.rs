//! The call instructions one function executes, read from its machine code.
//!
//! The ground-truth tier stops the emulator at each of these instead of
//! running the callee. They come from decoding the function itself, not from
//! the decompiled body: a call the decompiler dropped is exactly what the
//! comparison has to be able to see, so the list cannot be taken from the
//! thing being checked.

use anyhow::{Context, Result};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::{DecodeContract, DecodedFlowKind, RuntimeSleighFrontend};
use std::collections::{BTreeMap, BTreeSet};

/// Decoding stops after this many instructions; a function longer than this
/// keeps only the calls in its first part, and any later call simply runs.
const INSTRUCTION_LIMIT: usize = 20_000;

/// Bytes read for the decode.
const MAX_BYTES: usize = 1 << 20;

/// Every call instruction reachable in the function at `address`, mapped to
/// the address execution continues at once it returns -- or `None` for a call
/// that does not return: one to a function the static no-return analysis
/// proves never returns, or one the decoder found nothing after.
///
/// A stubbed call always returns, so a call to `exit` left mapped to its
/// fall-through would run on into whatever function the linker put next.
pub fn call_sites(binary: &LoadedBinary, address: u64) -> Result<BTreeMap<u64, Option<u64>>> {
    let load_spec = binary
        .load_spec()
        .context("binary lacks a load spec (can't select a Sleigh frontend)")?;
    let frontend = RuntimeSleighFrontend::new_for_load_spec(load_spec)?;
    let address_state = frontend.normalize_low_bit_code_address(address);
    let decode_addr = address_state.address;
    let context_override = fission_static::analysis::decode_context_for_address(
        binary,
        &frontend,
        address,
        address_state.context_override,
    );
    let max_bytes = binary
        .available_execution_bytes(decode_addr)
        .map(|available| MAX_BYTES.min(available).max(1))
        .unwrap_or(MAX_BYTES);
    let bytes = binary
        .view_bytes(decode_addr, max_bytes)
        .with_context(|| format!("unable to read bytes at {decode_addr:#x}"))?;
    let memory_context = fission_static::analysis::control_flow_facts::decode_memory_context_for(
        binary,
        decode_addr,
        max_bytes,
    );
    let decoded = frontend.lift_raw_pcode_function_with_context_and_memory_context(
        bytes,
        decode_addr,
        DecodeContract::decomp_function(INSTRUCTION_LIMIT),
        &memory_context,
        context_override,
    )?;
    let no_return = fission_static::analysis::noreturn::no_return_functions_for(binary);
    let decoded_at: BTreeSet<u64> = decoded
        .instructions
        .iter()
        .map(|insn| insn.address)
        .collect();
    Ok(decoded
        .instructions
        .iter()
        .filter(|insn| insn.flow_kind == DecodedFlowKind::Call)
        .map(|insn| {
            let next = insn.address.wrapping_add(insn.length as u64);
            let returns = decoded_at.contains(&next)
                && !insn
                    .direct_target
                    .is_some_and(|target| no_return.contains(&target));
            (insn.address, returns.then_some(next))
        })
        .collect())
}
