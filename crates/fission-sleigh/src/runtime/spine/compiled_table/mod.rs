//! Compiled-table executor for the common SLEIGH runtime spine.
//! The canonical owner mapping remains `DecisionNode -> RuntimeInstructionContext ->
//! RuntimeConstructState/RuntimeParserWalker -> RuntimeTemplateEvaluator -> RuntimePcodeEmitter`.

use anyhow::{anyhow, bail, Result};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use crate::compiler::{
    CompiledConstTpl, CompiledConstructTplKind, CompiledContextCommitTarget, CompiledDecisionProbe,
    CompiledDisjointPattern, CompiledExecutableConstructor, CompiledFrontend,
    CompiledHandleSelector, CompiledHandleTemplate, CompiledHandleTpl, CompiledOpTpl,
    CompiledOpTplOpcode, CompiledOperandDecodeStep, CompiledOperandSpec, CompiledPatternBlock,
    CompiledPatternExpression, CompiledPatternMatcher, CompiledSpaceRef, CompiledSpaceTpl,
    CompiledTemplateSource, CompiledVarnodeTpl,
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

mod context;
mod display;
mod feature_audit;
mod handles;
mod runtime_index;
mod selection;
mod strategy;
mod template_eval;
mod token;
mod walker;

pub use feature_audit::{audit_sla_template_features, SlaTemplateFeatureAudit};
pub use template_eval::{FlowEmitOptions, RuntimeFlowOverride};

use context::*;
use display::*;
use handles::*;
use runtime_index::*;
use selection::*;
use strategy::*;
use template_eval::*;
use token::*;
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

        let strategy = RuntimeDecodeStrategy::for_table(compiled, native, "instruction", &ctx);
        let decoded = match bind_instruction(compiled, strategy, &ctx, selection) {
            Ok(decoded) => decoded,
            Err(err) => {
                if first_error.is_none() {
                    first_error = Some(typed_template_resolution_error(compiled, err));
                }
                continue;
            }
        };

        match emit_pcode_for_state_with_bytes(
            compiled,
            native,
            address,
            bytes,
            address,
            &decoded,
            FlowEmitOptions {
                instruction_context_register: ctx.context_register,
                instruction_context_known_mask: ctx.context_known_mask,
                ..Default::default()
            },
        ) {
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
        ctx.context_register =
            (ctx.context_register & !override_mask) | (override_ctx & override_mask);
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

        let strategy = RuntimeDecodeStrategy::for_table(compiled, native, "instruction", &ctx);
        let decoded = match bind_instruction(compiled, strategy, &ctx, selection.clone()) {
            Ok(decoded) => decoded,
            Err(err) => {
                if first_error.is_none() {
                    first_error = Some(typed_template_resolution_error(compiled, err));
                }
                continue;
            }
        };

        match emit_pcode_for_state(
            compiled,
            native,
            address,
            bytes,
            address,
            &decoded,
            FlowEmitOptions {
                instruction_context_register: ctx.context_register,
                instruction_context_known_mask: ctx.context_known_mask,
                ..Default::default()
            },
        ) {
            Ok(_) => {
                return decoded_instruction_from_state(compiled, address, bytes, &ctx, decoded);
            }
            Err(err) => {
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
        }
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
) -> Result<u32> {
    let remaining = match bytes.get(inst_next_byte_offset..) {
        Some(b) if !b.is_empty() => b,
        _ => {
            bail!("delay-slot instruction at 0x{inst_next_address:x} is outside the memory window")
        }
    };
    let ctx = CompiledInstructionContext::parse(remaining, inst_next_address)?;
    let mut ctx = ctx;
    ctx.context_register = compiled.default_context;
    ctx.context_known_mask = compiled.default_context_known_mask;
    let strategy = RuntimeDecodeStrategy::for_table(compiled, native, "instruction", &ctx);
    let candidates = candidate_selections(compiled, &strategy, &ctx, inst_next_address)?;
    let mut first_err: Option<anyhow::Error> = None;
    for selection in candidates {
        if !selection.constructor.runtime_ready {
            continue;
        }
        let strategy = RuntimeDecodeStrategy::for_table(compiled, native, "instruction", &ctx);
        match bind_instruction(compiled, strategy, &ctx, selection) {
            Ok(decoded) => return Ok(decoded.length as u32),
            Err(err) => {
                if first_err.is_none() {
                    first_err = Some(err);
                }
            }
        }
    }
    Err(first_err.unwrap_or_else(|| {
        anyhow!("unable to decode delay-slot instruction at 0x{inst_next_address:x}")
    }))
}

/// Bind (decode operands/handles for) the instruction at `target_address` using a
/// contiguous `memory_window` where index `0` corresponds to `memory_base`.
///
/// Used by Ghidra `PcodeEmit.appendCrossBuild` and delay-slot emission.
pub(super) fn try_bind_runtime_state_at(
    compiled: &CompiledFrontend,
    native: Option<&Arc<NativeBackend>>,
    memory_window: &[u8],
    memory_base: u64,
    target_address: u64,
    context_register: u64,
    context_known_mask: u64,
) -> Result<RuntimeConstructState> {
    let offset = target_address.checked_sub(memory_base).ok_or_else(|| {
        anyhow!("bind target 0x{target_address:x} precedes memory base 0x{memory_base:x}")
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
    Err(first_err
        .unwrap_or_else(|| anyhow!("decode bind failed at target_address=0x{target_address:x}")))
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
/// - For each commit, look up the handle or built-in target symbol.
/// - Extract the target address from the handle's offset.
/// - Read the current context bits at (word_index, mask).
/// - Return (target_addr, word_index, mask, value) for the caller to apply.
pub(crate) fn apply_context_commits(
    compiled: &CompiledFrontend,
    decoded: &RuntimeConstructState,
    instruction_address: u64,
    current_context: u64,
) -> Result<Vec<ResolvedContextCommit>> {
    let mut results = Vec::new();
    for commit in &decoded.context_commits {
        let target_addr = match commit.target {
            CompiledContextCommitTarget::InstStart => instruction_address,
            CompiledContextCommitTarget::InstNext => {
                instruction_address.saturating_add(decoded.length as u64)
            }
            CompiledContextCommitTarget::OperandHandle { hand_index } => {
                let handle = decoded.handles.get(hand_index as usize).ok_or_else(|| {
                    anyhow!("context commit references missing operand handle {hand_index}")
                })?;
                // Ghidra: if handle.offset_space.type == CONSTANT, multiply by ram addr_unit.
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
                        .find(|s| {
                            s.name == "ram"
                                || (s.name != "const" && s.name != "unique" && s.name != "register")
                        })
                        .map(|s| s.word_size as u64)
                        .unwrap_or(1);
                    offset.wrapping_mul(addr_unit)
                } else {
                    offset
                }
            }
        };
        // Read current context bits at (word_index, mask) to get the value to commit.
        let value = packed_context_word(current_context, commit.word_index)? & commit.mask;
        results.push((target_addr, commit.word_index, commit.mask, value));
    }
    Ok(results)
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
    let (mnemonic, operands_text) = render_instruction_display(&decoded)?;
    let direct_target = first_relative_target(&decoded);
    let flow_kind = flow_kind_for_state(&decoded);
    let references = decoded_references(address, length, flow_kind, &decoded.handles);
    let pending_context_commits =
        apply_context_commits(compiled, &decoded, address, ctx.context_register)?;
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
