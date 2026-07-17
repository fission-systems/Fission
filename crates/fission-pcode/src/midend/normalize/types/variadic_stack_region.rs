//! Windows x64 home-slot / variadic-region recovery.
//!
//! This pass promotes builder-classified `HomeSlot` bindings into a semantic
//! `VaStart` marker when a call's tail arguments prove that a `va_list` cursor
//! points into the saved register/home region. The rewrite is intentionally
//! conservative: it only fires when the call already exposes stack-tail
//! arguments and the final argument is provably derived from a recovered home
//! slot.

use crate::midend::ir::{HirExpr, HirFunction, HirStmt, NirBindingOrigin};
use fission_core::CallingConvention;
use std::collections::{BTreeSet, HashMap};

use super::super::wave_stats::{
    add_abi_slot_recoveries, add_home_slot_promotions, add_va_start_recoveries,
    add_variadic_stack_region_folds,
};

fn home_slot_map(func: &HirFunction) -> HashMap<String, i64> {
    func.locals
        .iter()
        .filter_map(|binding| match binding.origin {
            Some(NirBindingOrigin::HomeSlot(offset)) => Some((binding.name.clone(), offset)),
            _ => None,
        })
        .collect()
}

fn expr_uses_home_slot(expr: &HirExpr, home_slots: &HashMap<String, i64>) -> bool {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => home_slots.contains_key(name),
        HirExpr::Load { ptr, .. } => expr_uses_home_slot(ptr, home_slots),
        HirExpr::PtrOffset { base, .. } | HirExpr::FieldAccess { base, .. } => {
            expr_uses_home_slot(base, home_slots)
        }
        HirExpr::Cast { expr, .. } => expr_uses_home_slot(expr, home_slots),
        HirExpr::Unary { expr, .. } => expr_uses_home_slot(expr, home_slots),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_uses_home_slot(lhs, home_slots) || expr_uses_home_slot(rhs, home_slots)
        }
        HirExpr::Call { args, .. } => args.iter().any(|arg| expr_uses_home_slot(arg, home_slots)),
        HirExpr::Index { base, index, .. } => {
            expr_uses_home_slot(base, home_slots) || expr_uses_home_slot(index, home_slots)
        }
        HirExpr::AggregateCopy { src, .. } => expr_uses_home_slot(src, home_slots),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_uses_home_slot(cond, home_slots)
                || expr_uses_home_slot(then_expr, home_slots)
                || expr_uses_home_slot(else_expr, home_slots)
        }
        HirExpr::Const(_, _) => false,
    }
}

fn call_tail_uses_home_slot(args: &[HirExpr], home_slots: &HashMap<String, i64>) -> bool {
    args.len() > 4
        && args[4..]
            .iter()
            .any(|arg| expr_uses_home_slot(arg, home_slots) || matches!(arg, HirExpr::Load { .. }))
}

fn call_last_arg_is_va_region(args: &[HirExpr], home_slots: &HashMap<String, i64>) -> bool {
    let Some(last) = args.last() else {
        return false;
    };
    expr_uses_home_slot(last, home_slots)
}

fn recover_in_stmt(
    stmt: &mut HirStmt,
    home_slots: &HashMap<String, i64>,
    last_named_param: Option<&str>,
    folds: &mut usize,
    va_starts: &mut usize,
) -> bool {
    match stmt {
        HirStmt::Expr(HirExpr::Call { args, .. }) => {
            if call_tail_uses_home_slot(args, home_slots) {
                *folds += 1;
                if last_named_param.is_some() && call_last_arg_is_va_region(args, home_slots) {
                    return true;
                }
            }
            false
        }
        HirStmt::VaStart { va_list, .. } => expr_uses_home_slot(va_list, home_slots),
        HirStmt::Assign { rhs, .. } => {
            if let HirExpr::Call { args, .. } = rhs
                && call_tail_uses_home_slot(args, home_slots)
            {
                *folds += 1;
            }
            false
        }
        HirStmt::Block(stmts)
        | HirStmt::While { body: stmts, .. }
        | HirStmt::DoWhile { body: stmts, .. }
        | HirStmt::For { body: stmts, .. } => {
            recover_in_stmts(stmts, home_slots, last_named_param, folds, va_starts)
        }
        HirStmt::Switch { cases, default, .. } => {
            let mut changed = false;
            for case in cases {
                changed |= recover_in_stmts(
                    &mut case.body,
                    home_slots,
                    last_named_param,
                    folds,
                    va_starts,
                );
            }
            changed |= recover_in_stmts(default, home_slots, last_named_param, folds, va_starts);
            changed
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            recover_in_stmts(then_body, home_slots, last_named_param, folds, va_starts)
                | recover_in_stmts(else_body, home_slots, last_named_param, folds, va_starts)
        }
        HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Return(_)
        | HirStmt::Break
        | HirStmt::Continue => false,
        _ => false,
    }
}

fn recover_in_stmts(
    stmts: &mut Vec<HirStmt>,
    home_slots: &HashMap<String, i64>,
    last_named_param: Option<&str>,
    folds: &mut usize,
    va_starts: &mut usize,
) -> bool {
    let mut changed = false;
    let mut idx = 0usize;
    while idx < stmts.len() {
        if recover_in_stmt(
            &mut stmts[idx],
            home_slots,
            last_named_param,
            folds,
            va_starts,
        ) && let HirStmt::Expr(HirExpr::Call { args, .. }) = &stmts[idx]
            && let Some(last_param) = last_named_param
        {
            let marker = HirStmt::VaStart {
                va_list: args.last().cloned().unwrap_or(HirExpr::Var("va".into())),
                last_named_param: last_param.to_string(),
            };
            let already_present = idx > 0
                && matches!(
                    &stmts[idx - 1],
                    HirStmt::VaStart {
                        last_named_param,
                        ..
                    } if last_named_param == last_param
                );
            if !already_present {
                stmts.insert(idx, marker);
                *va_starts += 1;
                changed = true;
                idx += 1;
            }
        }
        idx += 1;
    }
    changed
}

pub(crate) fn apply_variadic_stack_region_pass(func: &mut HirFunction) -> bool {
    if !func.is_64bit || func.calling_convention != CallingConvention::WindowsX64 {
        return false;
    }

    let home_slots = home_slot_map(func);
    if home_slots.is_empty() {
        return false;
    }

    let unique_offsets = home_slots.values().copied().collect::<BTreeSet<_>>();
    add_abi_slot_recoveries(unique_offsets.len());
    add_home_slot_promotions(home_slots.len());

    let last_named_param = func.params.last().map(|param| param.name.as_str());
    let mut folds = 0usize;
    let mut va_starts = 0usize;
    let changed = recover_in_stmts(
        &mut func.body,
        &home_slots,
        last_named_param,
        &mut folds,
        &mut va_starts,
    );
    add_variadic_stack_region_folds(folds);
    add_va_start_recoveries(va_starts);
    changed
}
