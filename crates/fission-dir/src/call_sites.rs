//! The call instructions one function executes, read from its machine code.
//!
//! The ground-truth tier stops the emulator at each of these instead of
//! running the callee. They come from decoding the function itself, not from
//! the decompiled body: a call the decompiler dropped is exactly what the
//! comparison has to be able to see, so the list cannot be taken from the
//! thing being checked.

use anyhow::{Context, Result, bail};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::{
    DecodeContract, DecodeStopReason, DecodedFlowKind, RuntimeSleighFrontend,
};
use std::collections::{BTreeMap, BTreeSet};

/// Decoding stops after this many instructions. A function longer than this
/// has no complete list of its calls, so it gets none (see [`call_sites`]).
const INSTRUCTION_LIMIT: usize = 20_000;

/// Bytes read for the decode.
const MAX_BYTES: usize = 1 << 20;

/// How control can leave one function's own code, read from its machine code.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MachineFlow {
    /// Every call instruction, mapped to the address execution continues at
    /// once it returns -- or `None` for a call proven never to return (a
    /// callee the static no-return analysis proves, or a site the decoder
    /// itself was told does not return).
    pub calls: BTreeMap<u64, Option<u64>>,
    /// Direct jumps to an address outside the function: tail calls. The
    /// callee's return is this function's return.
    pub tail_jumps: BTreeSet<u64>,
    /// The start of every instruction the decode reached. Execution anywhere
    /// else -- a switch case the single decode did not resolve, an indirect
    /// tail jump -- is code the stubs know nothing about.
    pub instructions: BTreeSet<u64>,
}

/// The calls, tail jumps and instructions of the function at `address`.
///
/// An error, not a partial answer, when the list cannot be complete: a decode
/// cut off at [`INSTRUCTION_LIMIT`], a call whose continuation was not decoded
/// although nothing proves it does not return, or a callee-cleanup ABI whose
/// stack adjustment a stub cannot know. A call missing from the list runs its
/// real callee on one side only, and a call wrongly taken for no-return ends
/// the run early and hides everything after it -- either can pass a wrong tree.
pub fn call_sites(binary: &LoadedBinary, address: u64) -> Result<MachineFlow> {
    if !binary.is_64bit && binary.format.to_ascii_uppercase().contains("PE") {
        bail!("32-bit PE: a stubbed __stdcall callee would leave its arguments on the stack");
    }
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
    // The decompiler's window: the function's own size, or the distance to
    // the next function. A wider one decodes on through a tail `jmp` into the
    // callee, whose calls then stop the machine where the tree has none.
    let window = binary
        .function_at(address)
        .map(|f| usize::try_from(f.size).unwrap_or(0))
        .filter(|size| *size > 0)
        .or_else(|| {
            fission_decompiler::pipeline::rust_sleigh::bounds::next_function_distance(
                binary, address,
            )
        })
        .unwrap_or(MAX_BYTES);
    let max_bytes = binary
        .available_execution_bytes(decode_addr)
        .map(|available| available.min(window).min(MAX_BYTES).max(1))
        .unwrap_or_else(|| window.min(MAX_BYTES).max(1));
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
    if decoded.stop_reason == DecodeStopReason::InstructionLimit {
        bail!("decode stopped at the {INSTRUCTION_LIMIT}-instruction limit");
    }
    let no_return = fission_static::analysis::noreturn::no_return_functions_for(binary);
    let noreturn_sites: BTreeSet<u64> = memory_context.noreturn_callsites.iter().copied().collect();
    let instructions: BTreeSet<u64> = decoded
        .instructions
        .iter()
        .map(|insn| insn.address)
        .collect();
    let mut calls = BTreeMap::new();
    let mut tail_jumps = BTreeSet::new();
    for insn in &decoded.instructions {
        match insn.flow_kind {
            DecodedFlowKind::Call => {
                let proven_no_return = noreturn_sites.contains(&insn.address)
                    || insn
                        .direct_target
                        .is_some_and(|target| no_return.contains(&target));
                let next = insn.address.wrapping_add(insn.length as u64);
                let continuation = if proven_no_return {
                    None
                } else if instructions.contains(&next) {
                    Some(next)
                } else {
                    bail!(
                        "call at {:#x}: its continuation was not decoded, and nothing proves \
                         the callee does not return",
                        insn.address
                    );
                };
                calls.insert(insn.address, continuation);
            }
            DecodedFlowKind::Jump => {
                if insn
                    .direct_target
                    .is_some_and(|target| !instructions.contains(&target))
                {
                    tail_jumps.insert(insn.address);
                }
            }
            _ => {}
        }
    }
    Ok(MachineFlow {
        calls,
        tail_jumps,
        instructions,
    })
}
