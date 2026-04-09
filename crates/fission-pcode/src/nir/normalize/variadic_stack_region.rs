//! Conservative detection of Windows x64 stack tail arguments (home / spill region).
//!
//! Does not rewrite IR yet; increments [`crate::nir::types::NirBuildStats::variadic_stack_region_fold_count`]
//! when call sites plausibly pass stack slots beyond the four register parameters — a lattice hook for
//! future `va_list`-style folding (kept separate from Win API name-based typing).

use crate::nir::support::CallingConvention;
use crate::nir::types::{HirExpr, HirFunction, HirStmt};

use super::wave_stats::add_variadic_stack_region_folds;

fn expr_uses_stack_slot_name(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Var(name) => name.starts_with("stack_"),
        HirExpr::Load { ptr, .. } => expr_uses_stack_slot_name(ptr),
        HirExpr::PtrOffset { base, .. } => expr_uses_stack_slot_name(base),
        HirExpr::Cast { expr, .. } => expr_uses_stack_slot_name(expr),
        HirExpr::Unary { expr, .. } => expr_uses_stack_slot_name(expr),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_uses_stack_slot_name(lhs) || expr_uses_stack_slot_name(rhs)
        }
        HirExpr::Call { args, .. } => args.iter().any(expr_uses_stack_slot_name),
        HirExpr::Index { base, index, .. } => {
            expr_uses_stack_slot_name(base) || expr_uses_stack_slot_name(index)
        }
        HirExpr::AggregateCopy { src, .. } => expr_uses_stack_slot_name(src),
        HirExpr::Const(_, _) => false,
    }
}

fn scan_calls_for_stack_tail(body: &[HirStmt], folds: &mut usize) {
    for stmt in body {
        match stmt {
            HirStmt::Assign { rhs, .. } => scan_expr_calls(rhs, folds),
            HirStmt::Expr(e) => scan_expr_calls(e, folds),
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. }
            | HirStmt::For { body: stmts, .. } => scan_calls_for_stack_tail(stmts, folds),
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                scan_expr_calls(expr, folds);
                for c in cases {
                    scan_calls_for_stack_tail(&c.body, folds);
                }
                scan_calls_for_stack_tail(default, folds);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                scan_expr_calls(cond, folds);
                scan_calls_for_stack_tail(then_body, folds);
                scan_calls_for_stack_tail(else_body, folds);
            }
            HirStmt::Return(Some(e)) => scan_expr_calls(e, folds),
            _ => {}
        }
    }
}

fn scan_expr_calls(expr: &HirExpr, folds: &mut usize) {
    match expr {
        HirExpr::Call { args, .. } => {
            if args.len() > 4 {
                let tail = &args[4..];
                // Stack tail: either surfaced `stack_*` names or memory loads (home space / outgoing args).
                if tail.iter().any(|a| {
                    expr_uses_stack_slot_name(a) || matches!(a, HirExpr::Load { .. })
                }) {
                    *folds += 1;
                }
            }
            for a in args {
                scan_expr_calls(a, folds);
            }
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => scan_expr_calls(expr, folds),
        HirExpr::Binary { lhs, rhs, .. } => {
            scan_expr_calls(lhs, folds);
            scan_expr_calls(rhs, folds);
        }
        HirExpr::PtrOffset { base, .. } => scan_expr_calls(base, folds),
        HirExpr::Index { base, index, .. } => {
            scan_expr_calls(base, folds);
            scan_expr_calls(index, folds);
        }
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
    }
}

pub(super) fn apply_variadic_stack_region_pass(func: &mut HirFunction) -> bool {
    if !func.is_64bit || func.calling_convention != CallingConvention::WindowsX64 {
        return false;
    }
    let mut folds = 0usize;
    scan_calls_for_stack_tail(&func.body, &mut folds);
    if folds == 0 {
        return false;
    }
    add_variadic_stack_region_folds(folds);
    true
}
