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
    CompiledSubtableDefinition, CompiledTemplateSource, CompiledVarnodeTpl,
};
use crate::runtime::spine::{
    self, BoundOperand, DecisionProbeEvaluator, RuntimeConstructState, RuntimeFixedHandle,
    RuntimeHandle, RuntimeInstructionContext, RuntimePcodeEmitter, RuntimeSelection,
    RuntimeTemplateEvaluator, RuntimeTemplateExecutor,
};
use crate::runtime::{
    DecodedFlowKind, DecodedInstruction, DecodedReference, DecodedReferenceKind,
    PackedContextOverride, RuntimeExecutionDetails, RuntimeSleighError,
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

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static BIND_CACHE: RefCell<HashMap<(u64, u64, u64), Result<RuntimeConstructState, String>>> = RefCell::new(HashMap::new());
}

pub(crate) fn clear_bind_cache() {
    BIND_CACHE.with(|cache| cache.borrow_mut().clear());
}

pub(crate) fn decode_and_lift_with_details(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Result<(Vec<PcodeOp>, u64, RuntimeExecutionDetails)> {
    decode_and_lift_with_context_override(compiled, bytes, address, None)
}

pub(crate) fn decode_and_lift_with_context_override(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
    context_override: Option<PackedContextOverride>,
) -> Result<(Vec<PcodeOp>, u64, RuntimeExecutionDetails)> {
    clear_bind_cache();
    let mut ctx = CompiledInstructionContext::parse(bytes, address)?;
    ctx.context_register = compiled.default_context;
    ctx.context_known_mask = compiled.default_context_known_mask;
    if let Some(context_override) = context_override {
        context_override.apply_to(&mut ctx.context_register, &mut ctx.context_known_mask);
    }

    let strategy = RuntimeDecodeStrategy::for_table();
    let candidates = candidate_selections(compiled, &ctx, address)?;
    let mut first_error: Option<anyhow::Error> = None;

    for selection in candidates {
        if !selection.constructor.runtime_ready {
            let err = unsupported_constructor_error(compiled, selection.constructor);
            if first_error.is_none() {
                first_error = Some(err.into());
            }
            continue;
        }

        let strategy = RuntimeDecodeStrategy::for_table();
        let decoded = match bind_instruction(compiled, strategy, &ctx, selection) {
            Ok(decoded) => decoded,
            Err(err) => {
                if first_error.is_none() {
                    first_error = Some(typed_template_resolution_error(compiled, err));
                }
                continue;
            }
        };

        let pending_context_commits =
            apply_context_commits(compiled, &decoded, address, decoded.context_register)?;

        let decoded_length = checked_runtime_length_u64(decoded.length, "decoded instruction")?;

        match emit_pcode_for_state_with_bytes(
            compiled,
            address,
            bytes,
            address,
            &decoded,
            FlowEmitOptions {
                instruction_length: Some(decoded_length),
                instruction_context_register: ctx.context_register,
                instruction_context_known_mask: ctx.context_known_mask,
                ..Default::default()
            },
        ) {
            Ok((ops, mut details)) => {
                details.pending_context_commits = pending_context_commits;
                return Ok((ops, decoded_length, details));
            }
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
/// `context_override` applies packed context bits over the default context before matching.
/// Used by multi-instruction loops to propagate `ContextCommit` / `globalset` effects across
/// instruction boundaries.
pub(crate) fn decode_instruction_with_context(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
    context_override: Option<PackedContextOverride>,
) -> Result<DecodedInstruction> {
    let mut ctx = CompiledInstructionContext::parse(bytes, address)?;
    ctx.context_register = compiled.default_context;
    ctx.context_known_mask = compiled.default_context_known_mask;
    if let Some(context_override) = context_override {
        context_override.apply_to(&mut ctx.context_register, &mut ctx.context_known_mask);
    }
    decode_instruction_inner(compiled, bytes, address, ctx)
}

pub(crate) fn decode_instruction(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Result<DecodedInstruction> {
    let mut ctx = CompiledInstructionContext::parse(bytes, address)?;
    ctx.context_register = compiled.default_context;
    ctx.context_known_mask = compiled.default_context_known_mask;
    decode_instruction_inner(compiled, bytes, address, ctx)
}

fn decode_instruction_inner(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
    ctx: CompiledInstructionContext<'_>,
) -> Result<DecodedInstruction> {
    clear_bind_cache();
    let strategy = RuntimeDecodeStrategy::for_table();
    let candidates = candidate_selections(compiled, &ctx, address)?;
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

        let strategy = RuntimeDecodeStrategy::for_table();
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
    let strategy = RuntimeDecodeStrategy::for_table();
    let candidates = candidate_selections(compiled, &ctx, inst_next_address)?;
    let mut first_err: Option<anyhow::Error> = None;
    for selection in candidates {
        if !selection.constructor.runtime_ready {
            continue;
        }
        let strategy = RuntimeDecodeStrategy::for_table();
        match bind_instruction(compiled, strategy, &ctx, selection) {
            Ok(decoded) => return checked_runtime_length_u32(decoded.length, "delay-slot decode"),
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
    memory_window: &[u8],
    memory_base: u64,
    target_address: u64,
    context_register: u64,
    context_known_mask: u64,
) -> Result<RuntimeConstructState> {
    let key = (target_address, context_register, context_known_mask);
    if let Some(cached) = BIND_CACHE.with(|cache| cache.borrow().get(&key).cloned()) {
        return cached.map_err(|err_str| anyhow!("{}", err_str));
    }

    let res = (|| -> Result<RuntimeConstructState> {
        let offset = checked_memory_window_offset(memory_base, target_address)?;
        let slice = memory_window.get(offset..).ok_or_else(|| {
            anyhow!(
                "bind target 0x{target_address:x} past memory window (base=0x{memory_base:x}, len={})",
                memory_window.len()
            )
        })?;
        let mut ctx = CompiledInstructionContext::parse(slice, target_address)?;
        ctx.context_register = context_register;
        ctx.context_known_mask = context_known_mask;
        let strategy = RuntimeDecodeStrategy::for_table();
        let candidates = candidate_selections(compiled, &ctx, target_address)?;
        let mut first_err: Option<anyhow::Error> = None;
        for selection in candidates {
            if !selection.constructor.runtime_ready {
                continue;
            }
            let strategy = RuntimeDecodeStrategy::for_table();
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
    })();

    BIND_CACHE.with(|cache| {
        cache.borrow_mut().insert(
            key,
            res.as_ref().map(|s| s.clone()).map_err(|e| e.to_string()),
        );
    });
    res
}

fn checked_memory_window_offset(memory_base: u64, target_address: u64) -> Result<usize> {
    let offset = target_address.checked_sub(memory_base).ok_or_else(|| {
        anyhow!("bind target 0x{target_address:x} precedes memory base 0x{memory_base:x}")
    })?;
    usize::try_from(offset).map_err(|_| {
        anyhow!("bind target offset {offset} does not fit usize for memory window indexing")
    })
}

fn checked_runtime_length_u32(length: usize, role: &str) -> Result<u32> {
    u32::try_from(length).map_err(|_| anyhow!("{role} length {length} exceeds u32"))
}

fn checked_runtime_length_u64(length: usize, role: &str) -> Result<u64> {
    u64::try_from(length).map_err(|_| anyhow!("{role} length {length} exceeds u64"))
}

fn checked_context_commit_handle_index(hand_index: u32) -> Result<usize> {
    usize::try_from(hand_index)
        .map_err(|_| anyhow!("context commit handle index {hand_index} does not fit usize"))
}

fn context_commit_handle_is_const_space(handle: &RuntimeHandle) -> Result<bool> {
    Ok(handle
        .fixed
        .space
        .as_ref()
        .ok_or_else(|| anyhow!("context commit handle missing primary space metadata"))?
        .name
        == "const")
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
                let decoded_length =
                    checked_runtime_length_u64(decoded.length, "context commit decoded")?;
                instruction_address
                    .checked_add(decoded_length)
                    .ok_or_else(|| anyhow!("context commit InstNext address overflowed"))?
            }
            CompiledContextCommitTarget::OperandHandle { hand_index } => {
                let hand_index = checked_context_commit_handle_index(hand_index)?;
                let handle = decoded.handles.get(hand_index).ok_or_else(|| {
                    anyhow!("context commit references missing operand handle {hand_index}")
                })?;
                // Ghidra SleighParserContext.applyCommits(): context-set addresses
                // are resolved from hand.offset_offset. If the resolved handle
                // lives in the constant space, scale by the current address
                // space's addressable unit size.
                let offset = handle.fixed.offset_offset;
                if context_commit_handle_is_const_space(handle)? {
                    let cur_space_index = compiled.sla_default_cur_space_index()?;
                    let cur_space = compiled.sla_spaces.get(&cur_space_index).ok_or_else(|| {
                        anyhow!("CurSpace index {cur_space_index} missing from sla_spaces")
                    })?;
                    if cur_space.word_size == 0 {
                        bail!(
                            "SLA CurSpace {} has word_size=0 for context commit",
                            cur_space.name
                        );
                    }
                    let addr_unit = u64::from(cur_space.word_size);
                    offset
                        .checked_mul(addr_unit)
                        .ok_or_else(|| anyhow!("context commit address-unit scaling overflowed"))?
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
) -> Result<()> {
    for &(target_addr, word_index, mask, value) in pending {
        if target_addr == address {
            set_packed_context_word(context_register, word_index, value, mask)?;
            set_packed_context_word(context_known_mask, word_index, mask, mask)?;
        }
    }
    Ok(())
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
    let flow_kind = flow_kind_for_state(&decoded);
    let references = decoded_references(address, length, flow_kind, &decoded.handles);
    let direct_target = first_flow_target(&decoded, address, length).or_else(|| {
        references
            .iter()
            .find_map(|reference| match reference.kind {
                DecodedReferenceKind::BranchTarget | DecodedReferenceKind::CallTarget => {
                    Some(reference.target)
                }
                _ => None,
            })
    });
    let pending_context_commits =
        apply_context_commits(compiled, &decoded, address, decoded.context_register)?;
    let instruction_bytes = bytes.get(..length).ok_or_else(|| {
        anyhow!(
            "decoded instruction length {length} exceeds available byte window {}",
            bytes.len()
        )
    })?;
    Ok(DecodedInstruction {
        address,
        bytes: instruction_bytes.to_vec(),
        length,
        mnemonic,
        operands_text,
        flow_kind,
        direct_target,
        references,
        pending_context_commits,
    })
}
