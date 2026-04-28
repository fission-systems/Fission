//! Transitional compiled-table executor for the common SLEIGH runtime spine.
//! The canonical owner mapping remains `DecisionNode -> RuntimeInstructionContext ->
//! RuntimeConstructState/RuntimeParserWalker -> RuntimeTemplateEvaluator -> RuntimePcodeEmitter`.

use anyhow::{anyhow, bail, Result};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};

use crate::compiler::{
    CompiledConstTpl, CompiledConstructTplKind, CompiledDecisionProbe, CompiledDisjointPattern,
    CompiledExecutableConstructor, CompiledFixedRegister, CompiledFrontend, CompiledHandleSelector,
    CompiledHandleTemplate, CompiledHandleTpl, CompiledOpTpl, CompiledOpTplOpcode,
    CompiledOperandDecodeStep, CompiledOperandSpec, CompiledPatternBlock,
    CompiledPatternExpression, CompiledPatternMatcher, CompiledSpaceRef, CompiledSpaceTpl,
    CompiledTemplateSource, CompiledTokenFieldRef, CompiledVarnodeTpl,
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

        match emit_pcode_for_state(compiled, address, &decoded) {
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

pub(crate) fn decode_instruction(
    compiled: &CompiledFrontend,
    native: Option<&Arc<NativeBackend>>,
    bytes: &[u8],
    address: u64,
) -> Result<DecodedInstruction> {
    let mut ctx = CompiledInstructionContext::parse(bytes, address)?;
    ctx.context_register = compiled.default_context;
    ctx.context_known_mask = compiled.default_context_known_mask;

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
                return decoded_instruction_from_state(address, bytes, decoded);
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
        return decoded_instruction_from_state(address, bytes, decoded);
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
    address: u64,
    bytes: &[u8],
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
    Ok(DecodedInstruction {
        address,
        bytes: bytes.get(..length).unwrap_or(bytes).to_vec(),
        length,
        mnemonic,
        operands_text,
        flow_kind,
        direct_target,
        references,
    })
}
