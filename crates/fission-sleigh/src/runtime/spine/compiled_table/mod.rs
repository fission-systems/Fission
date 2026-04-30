//! Transitional compiled-table executor for the common SLEIGH runtime spine.
//! The canonical owner mapping remains `DecisionNode -> RuntimeInstructionContext ->
//! RuntimeConstructState/RuntimeParserWalker -> RuntimeTemplateEvaluator -> RuntimePcodeEmitter`.

use anyhow::{anyhow, bail, Result};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use crate::compiler::{
    CompiledConstTpl, CompiledConstructTplKind, CompiledContextCommit, CompiledDecisionProbe,
    CompiledDisjointPattern, CompiledExecutableConstructor, CompiledFixedRegister,
    CompiledFrontend, CompiledHandleSelector, CompiledHandleTemplate, CompiledHandleTpl,
    CompiledOpTpl, CompiledOpTplOpcode, CompiledOperandDecodeStep, CompiledOperandSpec,
    CompiledPatternBlock, CompiledPatternExpression, CompiledPatternMatcher, CompiledSpaceRef,
    CompiledSpaceTpl, CompiledTemplateSource, CompiledTokenFieldRef, CompiledVarnodeTpl,
};
use crate::runtime::spine::{
    self, BoundOperand, DecisionProbeEvaluator, RuntimeConstructState, RuntimeFixedHandle,
    RuntimeHandle, RuntimeInstructionContext, RuntimePcodeEmitter, RuntimeSelection,
    RuntimeTemplateEvaluator, RuntimeTemplateExecutor,
};
use crate::runtime::{
    DecodedFlowKind, DecodedInstruction, DecodedReference, DecodedReferenceKind,
    RuntimeExecutionDetails, RuntimeSleighError,
};

mod context {
    use super::*;
    include!("context.rs");
}

mod legacy_token_policy {
    use super::*;
    include!("token.rs");
}

mod selection {
    use super::*;
    include!("selection.rs");
}

mod strategy {
    use super::*;
    include!("strategy.rs");
}

mod walker {
    use super::*;
    include!("walker.rs");
}

mod display {
    use super::*;
    include!("display.rs");
}

mod handles {
    use super::*;
    include!("handles.rs");
}

mod template_eval {
    use super::*;
    include!("template_eval.rs");
}

use context::*;
use display::*;
use handles::*;
use legacy_token_policy::*;
use selection::*;
use strategy::*;
use template_eval::*;
use walker::*;

#[cfg(test)]
mod tests;

use crate::runtime::native::NativeBackend;
use std::sync::Arc;

pub(crate) fn decode_and_lift_with_details(
    compiled: &CompiledFrontend,
    native: Option<&Arc<NativeBackend>>,
    bytes: &[u8],
    address: u64,
) -> Result<(Vec<PcodeOp>, u64, RuntimeExecutionDetails)> {
    let mut ctx = CompiledInstructionContext::parse(bytes, address)?;
    ctx.context_register = compiled.default_context;
    ctx.context_known_mask = compiled.default_context_known_mask;

    let strategy = RuntimeDecodeStrategy::for_table(compiled, native, "instruction", &ctx);
    let candidates = candidate_selections(compiled, &strategy, &ctx, address)?;
    let mut first_error: Option<anyhow::Error> = None;

    for selection in candidates {
        if !selection.constructor.runtime_ready {
            let err = unsupported_constructor_error(compiled, selection.constructor);
            if first_error.is_none() {
                first_error = Some(err.into());
            }
            continue;
        }

        let decoded = match bind_instruction(compiled, strategy, &ctx, selection) {
            Ok(decoded) => decoded,
            Err(err) => {
                if first_error.is_none() {
                    first_error = Some(typed_template_resolution_error(compiled, err));
                }
                continue;
            }
        };

        match emit_pcode_for_state_with_bytes(compiled, address, bytes, &decoded) {
            Ok((ops, details)) => return Ok((ops, decoded.length as u64, details)),
            Err(err) => {
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
        }
    }

    Err(first_error.unwrap_or_else(|| {
        RuntimeSleighError::DecodeNoMatch {
            language: compiled.entry_id.clone(),
            address,
        }
        .into()
    }))
}

/// Decodes a single instruction with optional context register override.
///
/// `context_override` – when `Some((ctx, mask))`, applies `ctx` bits (selected by `mask`)
/// over the default context before matching. Used by multi-instruction loops to propagate
/// `ContextCommit` / `globalset` effects across instruction boundaries.
pub(crate) fn decode_instruction_with_context(
    compiled: &CompiledFrontend,
    native: Option<&Arc<NativeBackend>>,
    bytes: &[u8],
    address: u64,
    context_override: Option<(u64, u64)>,
) -> Result<DecodedInstruction> {
    let mut ctx = CompiledInstructionContext::parse(bytes, address)?;
    ctx.context_register = compiled.default_context;
    ctx.context_known_mask = compiled.default_context_known_mask;
    if let Some((override_ctx, override_mask)) = context_override {
        ctx.context_register = (ctx.context_register & !override_mask) | (override_ctx & override_mask);
        ctx.context_known_mask |= override_mask;
    }
    decode_instruction_inner(compiled, native, bytes, address, ctx)
}

pub(crate) fn decode_instruction(
    compiled: &CompiledFrontend,
    native: Option<&Arc<NativeBackend>>,
    bytes: &[u8],
    address: u64,
) -> Result<DecodedInstruction> {
    let mut ctx = CompiledInstructionContext::parse(bytes, address)?;
    ctx.context_register = compiled.default_context;
    ctx.context_known_mask = compiled.default_context_known_mask;
    decode_instruction_inner(compiled, native, bytes, address, ctx)
}

fn decode_instruction_inner(
    compiled: &CompiledFrontend,
    native: Option<&Arc<NativeBackend>>,
    bytes: &[u8],
    address: u64,
    ctx: CompiledInstructionContext<'_>,
) -> Result<DecodedInstruction> {

    let strategy = RuntimeDecodeStrategy::for_table(compiled, native, "instruction", &ctx);
    let candidates = candidate_selections(compiled, &strategy, &ctx, address)?;
    let mut fallback_state = None;
    let mut first_error: Option<anyhow::Error> = None;

    for selection in candidates {
        if crate::runtime::diagnostics::terminal_reselect_trace_enabled() {
            eprintln!(
                "[decode-instruction] ctor={} mnemonic={} source={} ctx=0x{:016x} known=0x{:016x}",
                selection.constructor_index,
                selection.constructor.mnemonic,
                selection.constructor.source,
                ctx.context_register,
                ctx.context_known_mask,
            );
        }
        if !selection.constructor.runtime_ready {
            let err = unsupported_constructor_error(compiled, selection.constructor);
            if first_error.is_none() {
                first_error = Some(err.into());
            }
            continue;
        }

        let decoded = match bind_instruction(compiled, strategy, &ctx, selection.clone()) {
            Ok(decoded) => decoded,
            Err(err) => {
                if first_error.is_none() {
                    first_error = Some(typed_template_resolution_error(compiled, err));
                }
                continue;
            }
        };

        match emit_pcode_for_state(compiled, address, &decoded) {
            Ok(_) => {
                return decoded_instruction_from_state(compiled, address, bytes, &ctx, decoded);
            }
            Err(err) => {
                if fallback_state.is_none() {
                    fallback_state = Some(decoded);
                }
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
        }
    }

    if let Some(decoded) = fallback_state {
        return decoded_instruction_from_state(compiled, address, bytes, &ctx, decoded);
    }

    return Err(first_error.unwrap_or_else(|| {
        RuntimeSleighError::DecodeNoMatch {
            language: compiled.entry_id.clone(),
            address,
        }
        .into()
    }));
}

fn typed_template_resolution_error(
    compiled: &CompiledFrontend,
    err: anyhow::Error,
) -> anyhow::Error {
    let rendered = err.to_string();
    let lower = rendered.to_ascii_lowercase();
    if lower.contains("tokenfield")
        || lower.contains("token field")
        || lower.contains("handle")
        || lower.contains("varnode")
        || lower.contains("construct_tpl")
        || lower.contains("template")
    {
        RuntimeSleighError::UnsupportedPcodeTemplate {
            language: compiled.entry_id.clone(),
            reason: format!("operand_template_resolution_failed: {rendered}"),
        }
        .into()
    } else {
        err
    }
}

/// Decodes the instruction at `inst_next_offset` within `bytes` and returns its
/// length in bytes. Used by `InstNext2` for delay-slot architectures.
///
/// Ghidra: `SleighInstructionPrototype.getDelaySlotByteCount()` returns the byte count
/// of the delay-slot instruction by actually decoding it at `inst_next`.
pub(super) fn decode_instruction_length(
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
        if let Ok(decoded) = bind_instruction(compiled, strategy, &ctx, selection) {
            return decoded.length as u32;
        }
    }
    0
}

/// Resolved deferred context commit: Ghidra's SleighParserContext.applyCommits().
///
/// Each entry is `(target_address, word_index, mask, context_word_value)`.
/// The caller should apply these to the context cache for future instruction decodes.
pub(crate) type ResolvedContextCommit = (u64, u32, u32, u32);

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
            // Ghidra: if handle.offset_space.type == CONSTANT, multiply by ram addr_unit
            let offset = if handle.fixed.offset_space.is_some() {
                handle.fixed.temp_offset
            } else {
                handle.fixed.offset_offset
            };
            if handle
                .fixed
                .space
                .as_ref()
                .map(|s| s.name == "const")
                .unwrap_or(false)
            {
                let addr_unit = compiled
                    .sla_spaces
                    .values()
                    .find(|s| s.name == "ram" || (s.name != "const" && s.name != "unique" && s.name != "register"))
                    .map(|s| s.word_size as u64)
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

fn decoded_instruction_from_state(
    compiled: &CompiledFrontend,
    address: u64,
    bytes: &[u8],
    ctx: &CompiledInstructionContext<'_>,
    decoded: RuntimeConstructState,
) -> Result<DecodedInstruction> {
    let length = decoded.length;
    let (mnemonic, operands_text) = render_instruction_display(&decoded);
    let direct_target = decoded.operands.first().and_then(|operand| match operand {
        BoundOperand::Relative { target } => Some(*target),
        _ => None,
    });
    let flow_kind = flow_kind_for_state(&decoded);
    let references = decoded_references(
        address,
        length,
        flow_kind,
        &decoded.operands,
        &decoded.handles,
    );
    let pending_context_commits =
        apply_context_commits(compiled, &decoded, address, ctx.context_register);
    Ok(DecodedInstruction {
        address,
        bytes: bytes.get(..length).unwrap_or(bytes).to_vec(),
        length,
        mnemonic,
        operands_text,
        flow_kind,
        direct_target,
        references,
        pending_context_commits,
    })
}
