//! Purity and memory-clobber queries the presentation folds use to decide
//! whether an expression may be moved, duplicated or dropped.

use super::*;

pub(super) fn is_presentation_pure_intrinsic(target: &str) -> bool {
    matches!(
        target,
        "__popcount"
            | "__popcount64"
            | "__lzcnt"
            | "__carry"
            | "__scarry"
            | "__sborrow"
            | "__parity"
    )
}

pub(super) fn expr_is_presentation_pure(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Var(_)
        | HirExpr::AddressOfGlobal(_)
        | HirExpr::AddressOfLocal(_)
        | HirExpr::Const(_, _) => true,
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => expr_is_presentation_pure(expr),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_is_presentation_pure(lhs) && expr_is_presentation_pure(rhs)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_is_presentation_pure(cond)
                && expr_is_presentation_pure(then_expr)
                && expr_is_presentation_pure(else_expr)
        }
        // Pure flag/parity intrinsics are safe to inline or drop when unused.
        HirExpr::Call { target, args, .. } if is_presentation_pure_intrinsic(target) => {
            args.iter().all(expr_is_presentation_pure)
        }
        // Loads, real calls, aggregate copies, and field/index may alias memory.
        HirExpr::Call { .. }
        | HirExpr::Load { .. }
        | HirExpr::AggregateCopy { .. }
        | HirExpr::PtrOffset { .. }
        | HirExpr::Index { .. }
        | HirExpr::FieldAccess { .. } => false,
    }
}

/// Like [`expr_is_presentation_pure`], but also admits reads of memory.
///
/// A load is not pure -- a store between its definition and its use changes
/// what it reads -- but it is *movable* across a span that contains no store
/// and no call. Refusing it outright is why most temporaries stay
/// unfolded: on a measured 97-function binary, 38% of body lines are
/// temp-variable plumbing, and the seed of nearly every chain is a memory
/// read. Callers must pair this with [`memory_clobbered_between`].
pub(super) fn expr_is_movable_read(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. } => expr_is_movable_read(inner),
        HirExpr::Index { base, index, .. } => {
            expr_is_movable_read(base) && expr_is_movable_read(index)
        }
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => expr_is_movable_read(expr),
        HirExpr::Binary { lhs, rhs, .. } => expr_is_movable_read(lhs) && expr_is_movable_read(rhs),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_is_movable_read(cond)
                && expr_is_movable_read(then_expr)
                && expr_is_movable_read(else_expr)
        }
        // A real call may write anything; an aggregate copy is a store.
        HirExpr::Call { .. } | HirExpr::AggregateCopy { .. } => expr_is_presentation_pure(expr),
        _ => expr_is_presentation_pure(expr),
    }
}

/// Whether anything in `stmts[from..to]` could write memory a load might read.
pub(super) fn memory_clobbered_between(stmts: &[HirStmt], from: usize, to: usize) -> bool {
    stmts[from..to.min(stmts.len())]
        .iter()
        .any(stmt_clobbers_memory)
}

pub(super) fn stmt_clobbers_memory(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            !matches!(lhs, HirLValue::Var(_)) || !expr_is_movable_read(rhs)
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => !expr_is_movable_read(expr),
        HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue
        | HirStmt::Label(_)
        | HirStmt::Goto(_) => false,
        // Anything with a body is a barrier `find_single_use_target` already
        // refuses to scan past; treat it as clobbering so this stays sound if
        // that ever changes.
        _ => true,
    }
}
