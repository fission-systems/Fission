use fission_midend_core::ir::{
    ArgForwardingRelation, CallEdgeKind, CallTargetProvenance, CallTargetRef,
    ProcedureCallShape, ProcedureControlEffect, ProcedureMemoryEffect, ProcedureReturnShape,
    ProcedureStackEffect, ProcedureSummary, SummarySoundness, WrapperClass,
    WrapperContractionProof, parse_call_target_address,
};
use fission_midend_dir::{DirExpr, DirFunction, DirLValue, DirStmt};

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

pub fn summarize_wrapper_hir_function(func: &DirFunction) -> Option<ProcedureSummary> {
    let (target, wrapper_class, forwarded_param_indices, confidence) = match func.body.as_slice() {
        [DirStmt::Return(Some(DirExpr::Call { target, args, .. }))] => {
            let forwarded = args
                .iter()
                .map(|arg| match arg {
                    DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
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
            DirStmt::Assign {
                lhs: DirLValue::Var(temp),
                rhs: DirExpr::Call { target, args, .. },
            },
            DirStmt::Return(Some(DirExpr::Var(ret))),
        ] if temp == ret => {
            let forwarded = args
                .iter()
                .map(|arg| match arg {
                    DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
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

pub fn summary_soundness_for_wrapper(summary: &ProcedureSummary) -> SummarySoundness {
    if summary.wrapper_contraction.is_some() {
        SummarySoundness::Optimistic
    } else {
        SummarySoundness::Pessimistic
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_midend_core::ir::NirType;
    use fission_midend_dir::{DirBinding, DirExpr, DirFunction, DirStmt};
        use fission_core::CallingConvention;

    fn empty_binding(name: &str) -> DirBinding {
        DirBinding {
            name: name.to_string(),
            ty: NirType::Unknown,
            surface_type_name: None,
            origin: None,
            initializer: None,
        }
    }

    #[test]
    fn summarize_wrapper_hir_function_detects_tail_forwarder() {
        let func = DirFunction {
            name: "wrapper".to_string(),
            int_param_offsets: Vec::new(),
            params: vec![empty_binding("param_1")],
            locals: vec![],
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: vec![DirStmt::Return(Some(DirExpr::Call {
                target: "sub_140010000".to_string(),
                args: vec![DirExpr::Var("param_1".to_string())],
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

}
