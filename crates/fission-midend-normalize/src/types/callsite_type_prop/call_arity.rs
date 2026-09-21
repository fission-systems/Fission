//! Call-site arity and void-receiver cleanup.
//!
//! This module owns structural call cleanup after signature/type propagation:
//! exact known-API arity, void receiver removal, and self-call arity pruning.

use super::*;

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
