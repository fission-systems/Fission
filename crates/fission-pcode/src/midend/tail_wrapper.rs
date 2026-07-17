//! P-code adapters for direct tail-wrapper summary (normalize owner lives in midend-normalize).

use crate::midend::ir::{
    ArgForwardingRelation, CallEdgeKind, CallTargetProvenance, CallTargetRef, ProcedureCallShape,
    ProcedureControlEffect, ProcedureMemoryEffect, ProcedureReturnShape, ProcedureStackEffect,
    ProcedureSummary, WrapperClass, WrapperContractionProof,
};
use crate::pcode::{PcodeFunction, PcodeOp, PcodeOpcode};

pub fn summarize_direct_tail_wrapper_from_pcode(
    pcode: &PcodeFunction,
    entry_address: u64,
    target_symbol: impl Fn(u64) -> String,
    classify_import: impl Fn(u64) -> bool,
) -> Option<ProcedureSummary> {
    let block = pcode.blocks.first()?;
    if pcode.blocks.len() != 1 || block.ops.len() != 1 {
        return None;
    }
    summarize_direct_tail_wrapper_from_ops(
        &block.ops,
        entry_address,
        target_symbol,
        classify_import,
    )
}

pub fn summarize_direct_tail_wrapper_from_ops(
    ops: &[PcodeOp],
    entry_address: u64,
    target_symbol: impl Fn(u64) -> String,
    classify_import: impl Fn(u64) -> bool,
) -> Option<ProcedureSummary> {
    let semantic_ops = ops
        .iter()
        .filter(|op| op.asm_mnemonic.as_deref() != Some("INSN_RAW"))
        .collect::<Vec<_>>();
    if semantic_ops.len() != 1 {
        return None;
    }
    let op = semantic_ops.first()?;
    if op.opcode != PcodeOpcode::Branch || op.inputs.len() != 1 {
        return None;
    }
    let target = op.inputs.first()?;
    if !target.is_constant {
        return None;
    }
    let target_addr = target.constant_val as u64;
    if target_addr == 0 || target_addr == entry_address {
        return None;
    }
    let wrapper_class = if classify_import(target_addr) {
        WrapperClass::ImportThunk
    } else {
        WrapperClass::TailForwarder
    };
    let call_target = CallTargetRef {
        address: Some(target_addr),
        symbol: target_symbol(target_addr),
        provenance: if wrapper_class == WrapperClass::ImportThunk {
            CallTargetProvenance::Import
        } else {
            CallTargetProvenance::Direct
        },
        edge_kind: if wrapper_class == WrapperClass::ImportThunk {
            CallEdgeKind::Import
        } else {
            CallEdgeKind::Direct
        },
        confidence: 224,
    };
    Some(ProcedureSummary {
        control_effect: ProcedureControlEffect::TailJumps,
        memory_effect: ProcedureMemoryEffect::Pure,
        stack_effect: ProcedureStackEffect::Neutral,
        return_shape: ProcedureReturnShape::ForwardedTailTarget(call_target.clone()),
        arg_forwarding: ArgForwardingRelation {
            forwarded_param_indices: Vec::new(),
        },
        call_shape: ProcedureCallShape::SingleTailWrapper,
        wrapper_contraction: Some(WrapperContractionProof {
            wrapper_class,
            target: call_target,
            arg_forwarding: ArgForwardingRelation {
                forwarded_param_indices: Vec::new(),
            },
            confidence: 224,
        }),
    })
}
