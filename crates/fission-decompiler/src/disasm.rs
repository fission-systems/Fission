//! Whole-function raw disassembly -- a single shared primitive for anything
//! that wants "address, bytes, decoded instruction text" per instruction of
//! a function, independent of the P-code/NIR/decompile pipeline.
//!
//! Extracted from `fission-cli`'s `oneshot::disasm`/`oneshot::raw_pcode`
//! (which were `pub(super)`-scoped to that CLI module, so nothing else could
//! call them) so both the CLI's `disasm` subcommand and `fission-serve`'s
//! `/api/disasm/:session/:addr` handler can share one implementation instead
//! of the CLI reimplementing it and the server not having it at all.

use fission_core::PAGE_SIZE;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::{DecodeContract, RuntimeSleighFrontend};
use fission_static::analysis::control_flow_facts::decode_memory_context_for;

/// One decoded instruction, ready for direct GUI/API display.
#[derive(Debug, Clone)]
pub struct InstructionRow {
    pub address: u64,
    /// Space-separated lowercase hex byte pairs, e.g. "48 89 e5".
    pub bytes_hex: String,
    /// Decoded mnemonic + operands, e.g. "mov rbp, rsp".
    pub text: String,
    /// Direct call/branch target, when the instruction has one -- lets a
    /// caller (e.g. the GUI's click-to-navigate) jump there without having
    /// to parse `text`.
    pub target_addr: Option<u64>,
}

fn runtime_frontend_for_binary(binary: &LoadedBinary) -> Result<RuntimeSleighFrontend, String> {
    let load_spec = binary
        .load_spec()
        .ok_or_else(|| format!("missing Ghidra load spec for '{}'", binary.path))?;
    RuntimeSleighFrontend::new_for_load_spec(load_spec).map_err(|e| e.to_string())
}

/// Disassemble the whole function at `addr` (from its resolved start, not
/// necessarily `addr` itself). Falls back to guessing the function's size
/// from the next-higher known function address when the loader didn't
/// record one (e.g. a function discovered without a symbol-table size).
pub fn disassemble_function(binary: &LoadedBinary, addr: u64) -> Result<Vec<InstructionRow>, String> {
    let func = binary
        .function_at(addr)
        .ok_or_else(|| format!("No function found at address 0x{addr:x}"))?;
    let func_start = func.address;
    let mut func_size = func.size;
    if func_size == 0 {
        func_size = binary
            .functions
            .iter()
            .filter(|f| f.address > func_start)
            .map(|f| f.address)
            .min()
            .map(|next_addr| next_addr - func_start)
            .unwrap_or(PAGE_SIZE as u64);
    }
    let max_bytes = usize::try_from(func_size).unwrap_or(PAGE_SIZE).max(1);

    let frontend = runtime_frontend_for_binary(binary)?;
    let address_state = frontend.normalize_low_bit_code_address(func_start);
    let decode_addr = address_state.address;
    let max_bytes = binary
        .available_execution_bytes(decode_addr)
        .map(|available| max_bytes.min(available).max(1))
        .unwrap_or(max_bytes.max(1));
    let bytes = binary
        .view_bytes(decode_addr, max_bytes)
        .ok_or_else(|| format!("unable to read bytes at 0x{decode_addr:x}"))?;
    let memory_context = decode_memory_context_for(binary, decode_addr, max_bytes);
    let lifted = frontend
        .lift_raw_pcode_function_with_context_and_memory_context(
            bytes,
            decode_addr,
            DecodeContract::strict_function(max_bytes),
            &memory_context,
            address_state.context_override,
        )
        .map_err(|e| e.to_string())?;

    Ok(lifted
        .instructions
        .iter()
        .map(|instruction| InstructionRow {
            address: instruction.address,
            bytes_hex: instruction
                .bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join(" "),
            text: instruction.instruction_text(),
            target_addr: instruction.direct_target,
        })
        .collect())
}
