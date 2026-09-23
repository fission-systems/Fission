//! Call-site arity and void-receiver cleanup.
//!
//! This module owns structural call cleanup after signature/type propagation:
//! exact known-API arity, void receiver removal, and self-call arity pruning.

use super::*;

/// Remove only a dead call-result store, not the call itself.
///
/// API calls may share a machine-register binding even when consecutive
/// results have unrelated C types. If a later call overwrites that carrier
/// before its value is read, keeping the first call's receiver unnecessarily
/// merges the two result types into one local declaration. `TempPreserved`
/// protects materializations needed while building the function; by this
/// normalization pass those consumers have run, so explicit liveness is the
/// authority for whether a temporary call result remains observable.
pub(super) fn drop_unused_call_receivers(func: &mut PreHirFunction) -> usize {
    let address_taken = crate::analysis::defuse::collect_address_taken_locals(&func.body);
    let mut protected = HashSet::default();
    protected.extend(func.params.iter().map(|binding| binding.name.clone()));
    protected.extend(
        func.locals
            .iter()
            .filter(|binding| {
                let non_addressable_local =
                    binding.origin.is_some_and(NirBindingOrigin::is_temp_like)
                        || (binding.origin.is_none() && !address_taken.contains(&binding.name));
                !non_addressable_local || matches!(binding.ty, NirType::Aggregate { .. })
            })
            .map(|binding| binding.name.clone()),
    );
    drop_unused_call_receivers_in_stmts(&mut func.body, &HashSet::default(), &protected)
}

fn drop_unused_call_receivers_in_stmts(
    stmts: &mut Vec<PreHirStmt>,
    live_after: &HashSet<String>,
    protected: &HashSet<String>,
) -> usize {
    let mut live = live_after.clone();
    let mut dropped = 0;
    for index in (0..stmts.len()).rev() {
        match &mut stmts[index] {
            PreHirStmt::Block(body) => {
                let body_live_after = live_out_for_stmt_list(body, &live);
                dropped += drop_unused_call_receivers_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    &body_live_after,
                    protected,
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                let then_live_after = live_out_for_stmt_list(then_body, &live);
                let else_live_after = live_out_for_stmt_list(else_body, &live);
                dropped += drop_unused_call_receivers_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    &then_live_after,
                    protected,
                );
                dropped += drop_unused_call_receivers_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    &else_live_after,
                    protected,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    let case_live_after = live_out_for_stmt_list(&case.body, &live);
                    dropped += drop_unused_call_receivers_in_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        &case_live_after,
                        protected,
                    );
                }
                let default_live_after = live_out_for_stmt_list(default, &live);
                dropped += drop_unused_call_receivers_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    &default_live_after,
                    protected,
                );
            }
            // A loop body can feed a later iteration. Its local live-out needs
            // a loop fixed point, so this deliberately leaves loop receivers
            // alone until that proof is available.
            PreHirStmt::While { .. } | PreHirStmt::DoWhile { .. } | PreHirStmt::For { .. } => {}
            _ => {}
        }

        let receiver = match &stmts[index] {
            PreHirStmt::Assign {
                lhs: PreHirLValue::Var(name),
                rhs,
            } if !protected.contains(name)
                && !live.contains(name)
                && expr_is_known_api_call_result(rhs) =>
            {
                Some(rhs.clone())
            }
            _ => None,
        };
        if let Some(call) = receiver {
            stmts[index] = PreHirStmt::Expr(call);
            dropped += 1;
        }
        live = crate::analysis::liveness::LivenessTransfer::for_stmt(&stmts[index])
            .live_in_from(&live);
    }
    dropped
}

fn live_out_for_stmt_list(stmts: &[PreHirStmt], live_after: &HashSet<String>) -> HashSet<String> {
    if crate::analysis::liveness::LivenessTransfer::for_stmts(stmts).may_fall_through() {
        live_after.clone()
    } else {
        HashSet::default()
    }
}

fn expr_is_known_api_call_result(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Call { target, .. } => api_signature_via_import_aliases(target).is_some(),
        PreHirExpr::Cast { expr, .. } => expr_is_known_api_call_result(expr),
        _ => false,
    }
}

#[cfg(test)]
mod unused_receiver_tests {
    use super::*;

    fn temp(name: &str) -> PreHirBinding {
        PreHirBinding {
            name: name.to_string(),
            ty: NirType::Int {
                bits: 64,
                signed: false,
            },
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn side_effecting_call() -> PreHirExpr {
        PreHirExpr::Call {
            target: "setvbuf".to_string(),
            args: Vec::new(),
            ty: NirType::Int {
                bits: 32,
                signed: true,
            },
        }
    }

    fn assign_call(name: &str) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name.to_string()),
            rhs: side_effecting_call(),
        }
    }

    #[test]
    fn dead_temporary_call_receiver_becomes_a_call_statement() {
        let mut func = PreHirFunction {
            locals: vec![temp("eax")],
            body: vec![assign_call("eax")],
            ..Default::default()
        };

        assert_eq!(drop_unused_call_receivers(&mut func), 1);
        assert!(matches!(
            func.body.as_slice(),
            [PreHirStmt::Expr(PreHirExpr::Call { target, .. })] if target == "setvbuf"
        ));
    }

    #[test]
    fn call_receiver_stays_when_its_value_is_read_afterward() {
        let mut func = PreHirFunction {
            locals: vec![temp("eax")],
            body: vec![
                assign_call("eax"),
                PreHirStmt::Return(Some(PreHirExpr::Var("eax".to_string()))),
            ],
            ..Default::default()
        };

        assert_eq!(drop_unused_call_receivers(&mut func), 0);
        assert!(matches!(func.body[0], PreHirStmt::Assign { .. }));
    }

    #[test]
    fn branch_receiver_observed_after_join_stays_assigned() {
        let mut func = PreHirFunction {
            locals: vec![temp("eax")],
            body: vec![
                PreHirStmt::If {
                    cond: PreHirExpr::Const(1, NirType::Bool),
                    then_body: vec![assign_call("eax")].into(),
                    else_body: Vec::new().into(),
                },
                PreHirStmt::Return(Some(PreHirExpr::Var("eax".to_string()))),
            ],
            ..Default::default()
        };

        assert_eq!(drop_unused_call_receivers(&mut func), 0);
        let PreHirStmt::If { then_body, .. } = &func.body[0] else {
            panic!("expected if statement");
        };
        assert!(matches!(then_body.as_slice(), [PreHirStmt::Assign { .. }]));
    }

    #[test]
    fn stack_backed_call_receiver_is_not_removed_as_dead() {
        let mut slot = temp("slot");
        slot.origin = Some(NirBindingOrigin::StackOffset(-8));
        let mut func = PreHirFunction {
            locals: vec![slot],
            body: vec![assign_call("slot")],
            ..Default::default()
        };

        assert_eq!(drop_unused_call_receivers(&mut func), 0);
        assert!(matches!(func.body[0], PreHirStmt::Assign { .. }));
    }

    #[test]
    fn dead_preserved_temporary_call_receiver_becomes_a_call_statement() {
        let mut receiver = temp("uVar4");
        receiver.origin = Some(NirBindingOrigin::TempPreserved);
        let mut func = PreHirFunction {
            locals: vec![receiver],
            body: vec![assign_call("uVar4")],
            ..Default::default()
        };

        assert_eq!(drop_unused_call_receivers(&mut func), 1);
        assert!(matches!(
            func.body.as_slice(),
            [PreHirStmt::Expr(PreHirExpr::Call { target, .. })] if target == "setvbuf"
        ));
    }

    #[test]
    fn dead_unclassified_local_call_receiver_becomes_a_call_statement() {
        let mut local = temp("eax");
        local.origin = None;
        let mut func = PreHirFunction {
            locals: vec![local],
            body: vec![assign_call("eax")],
            ..Default::default()
        };

        assert_eq!(drop_unused_call_receivers(&mut func), 1);
        assert!(matches!(
            func.body.as_slice(),
            [PreHirStmt::Expr(PreHirExpr::Call { target, .. })] if target == "setvbuf"
        ));
    }

    #[test]
    fn address_taken_unclassified_local_call_receiver_is_preserved() {
        let mut local = temp("local");
        local.origin = None;
        let mut func = PreHirFunction {
            locals: vec![local],
            body: vec![
                PreHirStmt::Expr(PreHirExpr::Call {
                    target: "escape".to_string(),
                    args: vec![PreHirExpr::AddressOfLocal("local".to_string())],
                    ty: NirType::Unknown,
                }),
                assign_call("local"),
            ],
            ..Default::default()
        };

        assert_eq!(drop_unused_call_receivers(&mut func), 0);
        assert!(matches!(func.body[1], PreHirStmt::Assign { .. }));
    }

    #[test]
    fn unknown_indirect_call_receiver_is_not_removed_from_machine_state() {
        let mut receiver = temp("eax");
        receiver.origin = Some(NirBindingOrigin::TempPreserved);
        let mut func = PreHirFunction {
            locals: vec![receiver],
            body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("eax".to_string()),
                rhs: PreHirExpr::Call {
                    target: "__fission_callind_opaque".to_string(),
                    args: vec![PreHirExpr::Const(
                        3,
                        NirType::Int {
                            bits: 32,
                            signed: false,
                        },
                    )],
                    ty: NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                },
            }],
            ..Default::default()
        };

        assert_eq!(drop_unused_call_receivers(&mut func), 0);
        assert!(matches!(func.body[0], PreHirStmt::Assign { .. }));
    }

    #[test]
    fn returning_branches_do_not_keep_dead_call_result_live_from_unreachable_tail() {
        let mut eax = temp("eax");
        eax.origin = None;
        let mut func = PreHirFunction {
            locals: vec![eax, temp("fp")],
            body: vec![PreHirStmt::Block(
                vec![
                    PreHirStmt::If {
                        cond: PreHirExpr::Var("fp".to_string()),
                        then_body: vec![PreHirStmt::Return(Some(PreHirExpr::Const(
                            0,
                            NirType::Ptr(Box::new(NirType::Unknown)),
                        )))]
                        .into(),
                        else_body: vec![
                            assign_call("eax"),
                            PreHirStmt::Return(Some(PreHirExpr::Var("fp".to_string()))),
                        ]
                        .into(),
                    },
                    PreHirStmt::Return(Some(PreHirExpr::Var("eax".to_string()))),
                ]
                .into(),
            )],
            ..Default::default()
        };

        assert_eq!(drop_unused_call_receivers(&mut func), 1);
        let PreHirStmt::Block(body) = &func.body[0] else {
            panic!("expected outer block");
        };
        let PreHirStmt::If { else_body, .. } = &body[0] else {
            panic!("expected branch");
        };
        assert!(matches!(
            else_body.as_slice(),
            [PreHirStmt::Expr(PreHirExpr::Call { target, .. }), PreHirStmt::Return(Some(_))]
                if target == "setvbuf"
        ));
    }
}

fn exact_arity_for_target(
    target: &str,
    summaries: &indexmap::IndexMap<String, CallSummary>,
) -> Option<usize> {
    let resolved_target = resolve_call_target_symbol(target, summaries);
    if is_known_variadic_runtime_symbol(target) || is_known_variadic_runtime_symbol(resolved_target)
    {
        return None;
    }
    if resolved_target != target {
        return api_signature_via_import_aliases(resolved_target)
            .map(|sig| sig.params.len())
            .or_else(|| {
                summaries
                    .get(resolved_target)
                    .and_then(|summary| summary.prototype.locked_exact_arity)
            })
            .or_else(|| api_signature_via_import_aliases(target).map(|sig| sig.params.len()));
    }
    summaries
        .get(target)
        .and_then(|summary| summary.prototype.locked_exact_arity)
        .or_else(|| api_signature_via_import_aliases(resolved_target).map(|sig| sig.params.len()))
        .or_else(|| api_signature_via_import_aliases(target).map(|sig| sig.params.len()))
}

/// Whether this target leaves nothing for a receiver to read.
///
/// Two independent sources, either of which is decisive:
///
/// 1. The API type library's signature says the return type is `void`.
///    `resolve_return_ty` maps `void` to `None`, the same answer it gives for
///    a type it could not resolve, so the two are indistinguishable there --
///    this asks the signature string directly.
/// 2. Ghidra's no-return lists name it. A function that never returns cannot
///    have left a result behind either, and that covers the ones missing from
///    the signature library (`__stack_chk_fail`, `setutent`). Asked across
///    every executable format rather than the binary's own, which this pass
///    does not carry: the names on those lists are libc/OS primitives whose
///    no-return-ness does not vary by container.
fn api_target_returns_void(target: &str) -> bool {
    if api_signature_via_import_aliases(target)
        .is_some_and(|sig| matches!(sig.return_type.trim(), "void" | "VOID"))
    {
        return true;
    }
    let index = fission_core::core::ghidra_no_return::ghidra_no_return_index();
    [
        fission_core::core::ghidra_no_return::GHIDRA_FORMAT_ELF,
        fission_core::core::ghidra_no_return::GHIDRA_FORMAT_PE,
        fission_core::core::ghidra_no_return::GHIDRA_FORMAT_MACHO,
    ]
    .iter()
    .any(|format| index.is_no_return(format, None, None, target))
}

/// Drop the receiver from a call whose target is known to return nothing.
///
/// A call clobbers the ABI's result register, so a later read of it that
/// liveness cannot rule out makes the call site materialize a receiver --
/// giving `rax = free(ptr);` and `rax = (uchar *)(__stack_chk_fail());`.
/// Neither is valid C (gcc: "void value not ignored as it ought to be"), and
/// neither is true: the callee left nothing there to read.
///
/// Only the assignment goes; the call itself stays as an expression
/// statement, and the receiver stays declared, so a later use of it reads an
/// uninitialized local -- which is exactly as defined as reading the
/// register the callee never wrote.
pub(super) fn drop_void_call_receivers(
    stmts: &mut Vec<PreHirStmt>,
    summaries: &indexmap::IndexMap<String, fission_midend_core::CallSummary>,
) -> usize {
    let mut dropped = 0usize;
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Assign { rhs, .. } => {
                if let PreHirExpr::Call { target, .. } = rhs
                    && (api_target_returns_void(target)
                        || summaries
                            .get(target)
                            .is_some_and(|summary| summary.prototype.returns_void))
                {
                    *stmt = PreHirStmt::Expr(rhs.clone());
                    dropped += 1;
                }
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                dropped += drop_void_call_receivers(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    summaries,
                );
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                dropped += drop_void_call_receivers(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    summaries,
                );
                dropped += drop_void_call_receivers(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    summaries,
                );
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    dropped += drop_void_call_receivers(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        summaries,
                    );
                }
                dropped += drop_void_call_receivers(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    summaries,
                );
            }
            _ => {}
        }
    }
    dropped
}

pub(super) fn prune_known_api_call_args_stmts(
    stmts: &mut [PreHirStmt],
    summaries: &indexmap::IndexMap<String, CallSummary>,
) -> usize {
    let mut pruned = 0usize;
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { rhs, .. }
            | PreHirStmt::Expr(rhs)
            | PreHirStmt::Return(Some(rhs)) => {
                pruned += prune_known_api_call_args_expr(rhs, summaries);
            }
            PreHirStmt::VaStart { va_list, .. } => {
                pruned += prune_known_api_call_args_expr(va_list, summaries);
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                pruned += prune_known_api_call_args_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    summaries,
                );
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                pruned += prune_known_api_call_args_expr(expr, summaries);
                for case in cases {
                    pruned += prune_known_api_call_args_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        summaries,
                    );
                }
                pruned += prune_known_api_call_args_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    summaries,
                );
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                pruned += prune_known_api_call_args_expr(cond, summaries);
                pruned += prune_known_api_call_args_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    summaries,
                );
                pruned += prune_known_api_call_args_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    summaries,
                );
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
    pruned
}

fn prune_known_api_call_args_expr(
    expr: &mut PreHirExpr,
    summaries: &indexmap::IndexMap<String, CallSummary>,
) -> usize {
    let mut pruned = 0usize;
    match expr {
        PreHirExpr::Call { target, args, .. } => {
            for arg in args.iter_mut() {
                pruned += prune_known_api_call_args_expr(arg, summaries);
            }
            if let Some(exact_arity) = exact_arity_for_target(target, summaries)
                && args.len() > exact_arity
            {
                let removed = args.len() - exact_arity;
                args.truncate(exact_arity);
                pruned += removed;
            }
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            pruned += prune_known_api_call_args_expr(lhs, summaries);
            pruned += prune_known_api_call_args_expr(rhs, summaries);
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => {
            pruned += prune_known_api_call_args_expr(expr, summaries);
        }
        PreHirExpr::Index { base, index, .. } => {
            pruned += prune_known_api_call_args_expr(base, summaries);
            pruned += prune_known_api_call_args_expr(index, summaries);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            pruned += prune_known_api_call_args_expr(cond, summaries);
            pruned += prune_known_api_call_args_expr(then_expr, summaries);
            pruned += prune_known_api_call_args_expr(else_expr, summaries);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
    pruned
}

pub(super) fn prune_self_call_args_stmts(
    stmts: &mut [PreHirStmt],
    func_name: &str,
    arity: usize,
) -> usize {
    let mut pruned = 0usize;
    for stmt in stmts {
        match stmt {
            PreHirStmt::Assign { rhs, .. }
            | PreHirStmt::Expr(rhs)
            | PreHirStmt::Return(Some(rhs)) => {
                pruned += prune_self_call_args_expr(rhs, func_name, arity);
            }
            PreHirStmt::VaStart { va_list, .. } => {
                pruned += prune_self_call_args_expr(va_list, func_name, arity);
            }
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                pruned += prune_self_call_args_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    func_name,
                    arity,
                );
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                pruned += prune_self_call_args_expr(expr, func_name, arity);
                for case in cases {
                    pruned += prune_self_call_args_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        func_name,
                        arity,
                    );
                }
                pruned += prune_self_call_args_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    func_name,
                    arity,
                );
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                pruned += prune_self_call_args_expr(cond, func_name, arity);
                pruned += prune_self_call_args_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                    func_name,
                    arity,
                );
                pruned += prune_self_call_args_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                    func_name,
                    arity,
                );
            }
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue => {}
        }
    }
    pruned
}

fn prune_self_call_args_expr(expr: &mut PreHirExpr, func_name: &str, arity: usize) -> usize {
    let mut pruned = 0usize;
    match expr {
        PreHirExpr::Call { target, args, .. } => {
            for arg in args.iter_mut() {
                pruned += prune_self_call_args_expr(arg, func_name, arity);
            }
            if target == func_name && args.len() > arity {
                let removed = args.len() - arity;
                args.truncate(arity);
                pruned += removed;
            }
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            pruned += prune_self_call_args_expr(lhs, func_name, arity);
            pruned += prune_self_call_args_expr(rhs, func_name, arity);
        }
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => {
            pruned += prune_self_call_args_expr(expr, func_name, arity);
        }
        PreHirExpr::Index { base, index, .. } => {
            pruned += prune_self_call_args_expr(base, func_name, arity);
            pruned += prune_self_call_args_expr(index, func_name, arity);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            pruned += prune_self_call_args_expr(cond, func_name, arity);
            pruned += prune_self_call_args_expr(then_expr, func_name, arity);
            pruned += prune_self_call_args_expr(else_expr, func_name, arity);
        }
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => {}
    }
    pruned
}
