use crate::nir::types::{
    ArgForwardingRelation, CallEdgeKind, CallTargetProvenance, CallTargetRef, HirExpr, HirFunction,
    HirLValue, HirStmt, ProcedureCallShape, ProcedureControlEffect, ProcedureMemoryEffect,
    ProcedureReturnShape, ProcedureStackEffect, ProcedureSummary, SummarySoundness, WrapperClass,
    WrapperContractionProof, parse_call_target_address,
};
use crate::pcode::{PcodeFunction, PcodeOp, PcodeOpcode};

fn summary_seed(target: &str) -> CallTargetRef {
    if let Some(address) = parse_call_target_address(target) {
        return CallTargetRef {
            address: Some(address),
            symbol: target.to_string(),
            provenance: CallTargetProvenance::Direct,
            edge_kind: CallEdgeKind::Direct,
            confidence: 160,
        };
    }
    CallTargetRef {
        address: None,
        symbol: target.to_string(),
        provenance: CallTargetProvenance::Reference,
        edge_kind: CallEdgeKind::Reference,
        confidence: 128,
    }
}

pub(crate) fn summarize_wrapper_hir_function(func: &HirFunction) -> Option<ProcedureSummary> {
    let (target, wrapper_class, forwarded_param_indices, confidence) = match func.body.as_slice() {
        [HirStmt::Return(Some(HirExpr::Call { target, args, .. }))] => {
            let forwarded = args
                .iter()
                .map(|arg| match arg {
                    HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
                        func.params.iter().position(|param| param.name == *name)
                    }
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?;
            (
                summary_seed(target),
                WrapperClass::TailForwarder,
                forwarded,
                96,
            )
        }
        [
            HirStmt::Assign {
                lhs: HirLValue::Var(temp),
                rhs: HirExpr::Call { target, args, .. },
            },
            HirStmt::Return(Some(HirExpr::Var(ret))),
        ] if temp == ret => {
            let forwarded = args
                .iter()
                .map(|arg| match arg {
                    HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
                        func.params.iter().position(|param| param.name == *name)
                    }
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?;
            (
                summary_seed(target),
                WrapperClass::PureAdapter,
                forwarded,
                88,
            )
        }
        _ => return None,
    };

    Some(ProcedureSummary {
        control_effect: ProcedureControlEffect::Returns,
        memory_effect: ProcedureMemoryEffect::Unknown,
        stack_effect: ProcedureStackEffect::Unknown,
        return_shape: ProcedureReturnShape::ForwardedCallResult(target.clone()),
        arg_forwarding: ArgForwardingRelation {
            forwarded_param_indices: forwarded_param_indices.clone(),
        },
        call_shape: ProcedureCallShape::SingleTailWrapper,
        wrapper_contraction: Some(WrapperContractionProof {
            wrapper_class,
            target,
            arg_forwarding: ArgForwardingRelation {
                forwarded_param_indices,
            },
            confidence,
        }),
    })
}

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
    let op = block.ops.first()?;
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

pub(crate) fn summary_soundness_for_wrapper(summary: &ProcedureSummary) -> SummarySoundness {
    if summary.wrapper_contraction.is_some() {
        SummarySoundness::Optimistic
    } else {
        SummarySoundness::Pessimistic
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nir::support::CallingConvention;
    use crate::nir::types::{HirExpr, HirFunction, HirStmt, NirBinding, NirType};
    use crate::pcode::{PcodeBasicBlock, PcodeOp, Varnode};

    fn empty_binding(name: &str) -> NirBinding {
        NirBinding {
            name: name.to_string(),
            ty: NirType::Unknown,
            surface_type_name: None,
            origin: None,
            initializer: None,
        }
    }

    #[test]
    fn summarize_wrapper_hir_function_detects_tail_forwarder() {
        let func = HirFunction {
            name: "wrapper".to_string(),
            params: vec![empty_binding("param_1")],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![HirStmt::Return(Some(HirExpr::Call {
                target: "sub_140010000".to_string(),
                args: vec![HirExpr::Var("param_1".to_string())],
                ty: NirType::Unknown,
            }))],
            calling_convention: CallingConvention::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        };
        let summary = summarize_wrapper_hir_function(&func).expect("summary");
        assert_eq!(summary.call_shape, ProcedureCallShape::SingleTailWrapper);
        assert_eq!(
            summary
                .wrapper_contraction
                .as_ref()
                .map(|proof| proof.wrapper_class),
            Some(WrapperClass::TailForwarder)
        );
        assert_eq!(summary.arg_forwarding.forwarded_param_indices, vec![0]);
    }

    #[test]
    fn summarize_direct_tail_wrapper_from_pcode_detects_branch_wrapper() {
        let pcode = PcodeFunction {
            blocks: vec![PcodeBasicBlock {
                index: 0,
                start_address: 0x140002d40,
                successors: vec![],
                ops: vec![PcodeOp {
                    seq_num: 0,
                    opcode: PcodeOpcode::Branch,
                    address: 0x140002d40,
                    output: None,
                    inputs: vec![Varnode {
                        space_id: 0,
                        offset: 0,
                        size: 8,
                        is_constant: true,
                        constant_val: 0x140001430_i64,
                    }],
                    asm_mnemonic: Some("jmp".to_string()),
                }],
            }],
        };
        let summary = summarize_direct_tail_wrapper_from_pcode(
            &pcode,
            0x140002d40,
            |target| format!("sub_{target:x}"),
            |_| false,
        )
        .expect("summary");
        assert_eq!(summary.control_effect, ProcedureControlEffect::TailJumps);
        assert_eq!(
            summary
                .wrapper_contraction
                .as_ref()
                .map(|proof| proof.wrapper_class),
            Some(WrapperClass::TailForwarder)
        );
        assert_eq!(
            summary
                .wrapper_contraction
                .as_ref()
                .and_then(|proof| proof.target.address),
            Some(0x140001430)
        );
    }

    #[test]
    fn summarize_direct_tail_wrapper_from_ops_ignores_trace_copy() {
        let ops = vec![
            PcodeOp {
                seq_num: 0,
                opcode: PcodeOpcode::Copy,
                address: 0x140002d40,
                output: Some(Varnode {
                    space_id: 1,
                    offset: 0x7000_0000_0000_0000,
                    size: 8,
                    is_constant: false,
                    constant_val: 0,
                }),
                inputs: vec![Varnode {
                    space_id: 0,
                    offset: 0,
                    size: 8,
                    is_constant: true,
                    constant_val: 0,
                }],
                asm_mnemonic: Some("INSN_RAW".to_string()),
            },
            PcodeOp {
                seq_num: 1,
                opcode: PcodeOpcode::Branch,
                address: 0x140002d40,
                output: None,
                inputs: vec![Varnode {
                    space_id: 0,
                    offset: 0,
                    size: 8,
                    is_constant: true,
                    constant_val: 0x140001430_i64,
                }],
                asm_mnemonic: Some("jmp".to_string()),
            },
        ];
        let summary = summarize_direct_tail_wrapper_from_ops(
            &ops,
            0x140002d40,
            |target| format!("sub_{target:x}"),
            |_| false,
        )
        .expect("summary");
        assert_eq!(summary.control_effect, ProcedureControlEffect::TailJumps);
        assert_eq!(
            summary
                .wrapper_contraction
                .as_ref()
                .map(|proof| proof.wrapper_class),
            Some(WrapperClass::TailForwarder)
        );
    }
}
