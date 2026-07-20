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
    let (_, ops, decoded_length, details) = decode_instruction_and_lift_with_context_override(
        compiled,
        bytes,
        address,
        context_override,
    )?;
    Ok((ops, decoded_length, details))
}

pub(crate) fn decode_instruction_and_lift_with_context_override(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
    context_override: Option<PackedContextOverride>,
) -> Result<(
    DecodedInstruction,
    Vec<PcodeOp>,
    u64,
    RuntimeExecutionDetails,
)> {
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
                details.userops = compiled.userops.clone();
                let instruction =
                    decoded_instruction_from_state(compiled, address, bytes, &ctx, decoded)?;
                return Ok((instruction, ops, decoded_length, details));
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
/// Decode `bytes` at `address` and return the raw constructor-resolution
/// tree (`RuntimeConstructState`) instead of the flattened `DecodedInstruction`.
///
/// Mirrors the candidate-selection + `bind_instruction` steps of
/// `decode_instruction_and_lift_with_context_override` but skips p-code
/// emission -- callers that need the raw tree (e.g. `instruction_pattern_mask`
/// for FID hashing) don't need a successful lift, only a successful bind.
pub(crate) fn decode_instruction_raw_state(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Result<RuntimeConstructState> {
    clear_bind_cache();
    let mut ctx = CompiledInstructionContext::parse(bytes, address)?;
    ctx.context_register = compiled.default_context;
    ctx.context_known_mask = compiled.default_context_known_mask;

    let candidates = candidate_selections(compiled, &ctx, address)?;
    let mut first_error: Option<anyhow::Error> = None;
    for selection in candidates {
        if !selection.constructor.runtime_ready {
            continue;
        }
        let strategy = RuntimeDecodeStrategy::for_table();
        match bind_instruction(compiled, strategy, &ctx, selection) {
            Ok(decoded) => return Ok(decoded),
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

/// Compute the FID-style "instruction mask" for a decoded instruction: 0xFF
/// on bytes (bits, but SLEIGH pattern blocks are byte-granular in practice)
/// that constructor pattern matching actually constrained to select this
/// specific decode -- opcode/discriminating bytes -- and 0x00 on bytes that
/// are pure operand-field data never checked during constructor selection.
///
/// Mirrors Ghidra's `InstructionPrototype.getInstructionMask()` (consumed by
/// `MessageDigestFidHasher.hash()` to mask operand bytes out of the FID
/// "full hash"): the union, across every constructor in the decode tree
/// (root plus every operand that resolved through a subtable), of that
/// constructor's *instruction*-relative `CompiledPatternBlock` -- never the
/// context-register pattern, which FID's instruction mask excludes.
///
/// `length` is the decoded instruction's total byte length (`state.length`
/// on the root node); the returned `Vec<u8>` has exactly that many entries.
pub(crate) fn instruction_pattern_mask(root: &RuntimeConstructState, length: usize) -> Vec<u8> {
    let mut mask = vec![0u8; length];
    accumulate_pattern_mask(root, &mut mask);
    mask
}

fn accumulate_pattern_mask(state: &RuntimeConstructState, mask: &mut [u8]) {
    if let Some(pattern) = &state.match_trace.matched_leaf_pattern {
        apply_disjoint_pattern_mask(pattern, state.absolute_offset, mask);
    }
    // "Replaces current" wrapper constructors (e.g. an x86 legacy prefix
    // byte) that this state discarded on the way here -- see
    // `replaced_wrapper_patterns`'s doc comment. Without this, any
    // prefix-byte-consuming instruction's mask would be missing exactly
    // that byte's pattern bits.
    for (wrapper_offset, pattern) in &state.replaced_wrapper_patterns {
        apply_disjoint_pattern_mask(pattern, *wrapper_offset, mask);
    }
    for handle in &state.handles {
        if let Some(sub) = &handle.subtable_state {
            accumulate_pattern_mask(sub, mask);
        }
    }
}

fn apply_disjoint_pattern_mask(
    pattern: &CompiledDisjointPattern,
    base_offset: usize,
    mask: &mut [u8],
) {
    match pattern {
        CompiledDisjointPattern::Instruction(block) => {
            apply_pattern_block_mask(block, base_offset, mask)
        }
        // FID's instruction mask is instruction-bytes only -- a context
        // register pattern (e.g. an addressing-mode bit set by a prior
        // instruction) never lives in these bytes at all.
        CompiledDisjointPattern::Context(_) => {}
        CompiledDisjointPattern::Combine { instruction, .. } => {
            apply_pattern_block_mask(instruction, base_offset, mask)
        }
        CompiledDisjointPattern::Or(patterns) => {
            // A constructor's own pattern statement can itself be an OR of
            // several alternatives (`pattern: cond1 | cond2`); only one
            // alternative is actually why *this* decode matched, but that
            // information doesn't survive into `matched_leaf_pattern`.
            // Intersect (AND) every alternative's mask instead of unioning:
            // a bit only safely counts as "this constructor's opcode
            // identity" if *every* alternative pattern also constrains it,
            // otherwise this errs toward under-masking (a missed hash match)
            // rather than over-masking (a wrong one).
            let Some((first, rest)) = patterns.split_first() else {
                return;
            };
            let mut branch_mask = vec![0u8; mask.len()];
            apply_disjoint_pattern_mask(first, base_offset, &mut branch_mask);
            for alt in rest {
                let mut alt_mask = vec![0u8; mask.len()];
                apply_disjoint_pattern_mask(alt, base_offset, &mut alt_mask);
                for (b, a) in branch_mask.iter_mut().zip(alt_mask.iter()) {
                    *b &= a;
                }
            }
            for (m, b) in mask.iter_mut().zip(branch_mask.iter()) {
                *m |= b;
            }
        }
    }
}

fn apply_pattern_block_mask(block: &CompiledPatternBlock, base_offset: usize, mask: &mut [u8]) {
    if block.nonzero_size <= 0 {
        return;
    }
    let Ok(block_offset) = usize::try_from(block.offset.max(0)) else {
        return;
    };
    for (word_index, &mask_word) in block.mask_words.iter().enumerate() {
        let word_byte_offset = block_offset + word_index * 4;
        for (byte_index, &byte_mask) in mask_word.to_be_bytes().iter().enumerate() {
            let Some(abs) = base_offset
                .checked_add(word_byte_offset)
                .and_then(|v| v.checked_add(byte_index))
            else {
                continue;
            };
            if let Some(slot) = mask.get_mut(abs) {
                *slot |= byte_mask;
            }
        }
    }
}

pub(crate) fn decode_instruction_with_context(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
    context_override: Option<PackedContextOverride>,
) -> Result<DecodedInstruction> {
    let (instruction, _, _, _) = decode_instruction_and_lift_with_context_override(
        compiled,
        bytes,
        address,
        context_override,
    )?;
    Ok(instruction)
}

pub(crate) fn decode_instruction(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Result<DecodedInstruction> {
    decode_instruction_with_context(compiled, bytes, address, None)
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
