//! Transitional compiled-table executor for the common SLEIGH runtime spine.
//! The canonical owner mapping remains `DecisionNode -> RuntimeInstructionContext ->
//! RuntimeConstructState/RuntimeParserWalker -> RuntimeTemplateEvaluator -> RuntimePcodeEmitter`.

use anyhow::{anyhow, bail, Result};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use crate::compiler::{
    CompiledConstTpl, CompiledConstructTplKind, CompiledDecisionProbe,
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

mod audit {
    include!("audit.rs");
}

mod bind_decode {
    use super::*;
    include!("bind_decode.rs");
}

mod template_eval {
    use super::*;
    include!("template_eval.rs");
}

pub use template_eval::{FlowEmitOptions, RuntimeFlowOverride};
pub use audit::{audit_sla_template_features, SlaTemplateFeatureAudit};
pub(crate) use bind_decode::{
    apply_context_commits, decode_instruction_length, try_bind_runtime_state_at,
};

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
