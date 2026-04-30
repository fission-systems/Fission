/// Instruction length probe, cross-build/delay-slot bind-at-address, and context-commit resolution.

use anyhow::{anyhow, Result};
use std::sync::Arc;

use crate::compiler::CompiledFrontend;
use crate::runtime::native::NativeBackend;
use crate::runtime::spine::RuntimeConstructState;

/// Resolved deferred context commit: Ghidra's SleighParserContext.applyCommits().
///
/// Each entry is `(target_address, word_index, mask, context_word_value)`.
/// The caller should apply these to the context cache for future instruction decodes.
pub(crate) type ResolvedContextCommit = (u64, u32, u32, u32);

/// Decodes the instruction at `inst_next_offset` within `bytes` and returns its
/// length in bytes. Used by `InstNext2` for delay-slot architectures.
///
/// Ghidra: `SleighInstructionPrototype.getDelaySlotByteCount()` returns the byte count
/// of the delay-slot instruction by actually decoding it at `inst_next`.
pub(crate) fn decode_instruction_length(
    compiled: &CompiledFrontend,
    native: Option<&Arc<NativeBackend>>,
    bytes: &[u8],
    inst_next_address: u64,
    inst_next_byte_offset: usize,
) -> u32 {
    let remaining = match bytes.get(inst_next_byte_offset..) {
        Some(b) if !b.is_empty() => b,
        _ => return 0,
    };
    let ctx = match CompiledInstructionContext::parse(remaining, inst_next_address) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let mut ctx = ctx;
    ctx.context_register = compiled.default_context;
    ctx.context_known_mask = compiled.default_context_known_mask;
    let strategy = RuntimeDecodeStrategy::for_table(compiled, native, "instruction", &ctx);
    let candidates = match candidate_selections(compiled, &strategy, &ctx, inst_next_address) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    for selection in candidates {
        if !selection.constructor.runtime_ready {
            continue;
        }
        let strategy = RuntimeDecodeStrategy::for_table(compiled, native, "instruction", &ctx);
        if let Ok(decoded) = bind_instruction(compiled, strategy, &ctx, selection) {
            return decoded.length as u32;
        }
    }
    0
}

/// Bind (decode operands/handles for) the instruction at `target_address` using a
/// contiguous `memory_window` where index `0` corresponds to `memory_base`.
///
/// Used by Ghidra `PcodeEmit.appendCrossBuild` and delay-slot emission.
pub(crate) fn try_bind_runtime_state_at(
    compiled: &CompiledFrontend,
    native: Option<&Arc<NativeBackend>>,
    memory_window: &[u8],
    memory_base: u64,
    target_address: u64,
    context_register: u64,
    context_known_mask: u64,
) -> Result<RuntimeConstructState> {
    let offset = target_address
        .checked_sub(memory_base)
        .ok_or_else(|| {
            anyhow!(
                "bind target 0x{target_address:x} precedes memory base 0x{memory_base:x}"
            )
        })? as usize;
    let slice = memory_window.get(offset..).ok_or_else(|| {
        anyhow!(
            "bind target 0x{target_address:x} past memory window (base=0x{memory_base:x}, len={})",
            memory_window.len()
        )
    })?;
    let mut ctx = CompiledInstructionContext::parse(slice, target_address)?;
    ctx.context_register = context_register;
    ctx.context_known_mask = context_known_mask;
    let strategy = RuntimeDecodeStrategy::for_table(compiled, native, "instruction", &ctx);
    let candidates = candidate_selections(compiled, &strategy, &ctx, target_address)?;
    let mut first_err: Option<anyhow::Error> = None;
    for selection in candidates {
        if !selection.constructor.runtime_ready {
            continue;
        }
        let strategy = RuntimeDecodeStrategy::for_table(compiled, native, "instruction", &ctx);
        match bind_instruction(compiled, strategy, &ctx, selection) {
            Ok(state) => return Ok(state),
            Err(err) => {
                if first_err.is_none() {
                    first_err = Some(err);
                }
            }
        }
    }
    Err(first_err.unwrap_or_else(|| {
        anyhow!("decode bind failed at target_address=0x{target_address:x}")
    }))
}

/// Resolves `context_commits` from a decoded instruction into concrete
/// `(target_address, word_index, mask, value)` tuples for the caller to apply.
///
/// Ghidra algorithm (SleighParserContext.applyCommits):
/// - For each commit, look up the handle (by hand_index or built-in symbol).
/// - Extract the target address from the handle's offset.
/// - Read the current context bits at (word_index, mask).
/// - Return (target_addr, word_index, mask, value) for the caller to apply.
pub(crate) fn apply_context_commits(
    compiled: &CompiledFrontend,
    decoded: &RuntimeConstructState,
    instruction_address: u64,
    current_context: u64,
) -> Vec<ResolvedContextCommit> {
    let mut results = Vec::new();
    for commit in &decoded.context_commits {
        let target_addr = if commit.hand_index == u32::MAX {
            // Built-in symbol (e.g. `inst_next`): target = instruction start + length.
            instruction_address.saturating_add(decoded.length as u64)
        } else {
            // Resolve via the operand's fixed handle.
            let Some(handle) = decoded.handles.get(commit.hand_index as usize) else {
                continue;
            };
            let offset = if handle.fixed.offset_space.is_some() {
                handle.fixed.temp_offset
            } else {
                handle.fixed.offset_offset
            };
            // Ghidra `SleighParserContext.applyCommits`: constant-space offsets are scaled by
            // `curSpace.getAddressableUnitSize()` (the **instruction** address space). We use the
            // SLA default / CurSpace index and that space's `word_size`.
            if handle
                .fixed
                .space
                .as_ref()
                .map(|s| s.name == "const")
                .unwrap_or(false)
            {
                let cur_space_index = if compiled.sla_default_space_index != u64::MAX {
                    compiled.sla_default_space_index
                } else {
                    compiled.sla_default_cur_space_index().unwrap_or(0)
                };
                let addr_unit = compiled
                    .sla_spaces
                    .get(&cur_space_index)
                    .map(|s| s.word_size.max(1) as u64)
                    .unwrap_or(1);
                offset.wrapping_mul(addr_unit)
            } else {
                offset
            }
        };
        // Read current context bits at (word_index, mask) to get the value to commit.
        let value = match packed_context_word(current_context, commit.word_index) {
            Ok(word) => word & commit.mask,
            Err(_) => continue,
        };
        results.push((target_addr, commit.word_index, commit.mask, value));
    }
    results
}

/// Applies resolved context commits to a mutable context register.
/// Called before decoding an instruction at `address` to override context.
pub(crate) fn apply_pending_commits_to_context(
    context_register: &mut u64,
    context_known_mask: &mut u64,
    address: u64,
    pending: &[(u64, u32, u32, u32)],
) {
    for &(target_addr, word_index, mask, value) in pending {
        if target_addr == address {
            let _ = set_packed_context_word(context_register, word_index, value, mask);
            let _ = set_packed_context_word(context_known_mask, word_index, mask, mask);
        }
    }
}
