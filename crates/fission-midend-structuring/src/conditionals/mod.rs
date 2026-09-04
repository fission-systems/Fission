//! Conditional structuring free functions (`try_lower_if*`, short-circuit).
//!
//! Entry points take [`crate::host::StructuringHost`]. Production host is
//! `PreviewBuilder` in `fission-pcode`.

mod if_else;
mod plain_if;
mod short_circuit;

pub use if_else::{
    ComplexArmPlan, VirtualExitIfElsePlan, count_explicit_gotos,
    lower_virtual_exit_if_else_committed, plan_virtual_exit_if_else, try_lower_if_else,
    try_lower_return_chain_arm, try_reduce_if_else_with_follow,
};
pub use plain_if::try_lower_if;
pub use short_circuit::{
    try_lower_short_circuit_and, try_lower_short_circuit_and_else, try_lower_short_circuit_if,
    try_lower_short_circuit_or,
};

use crate::host::StructuringHost;
use crate::linear_types::{LinearExit, structuring_diag_enabled};
use fission_midend_core::ir::MlilPreviewError;
use fission_midend_prehir::{PreHirExpr, PreHirLValue, PreHirStmt};

/// Simple assign/expr statements that may sit in a while-loop's condition
/// prefix -- i.e. anything `try_lower_while` knows how to carry into either
/// the clean `while(cond)` shape (via `try_fold_cond_prefix`'s own,
/// separate single-use/no-earlier-read safety checks) or the always-safe
/// `while(1) { prefix; if (!cond) break; body }` guard, which just
/// re-executes `prefix` verbatim once per iteration -- exactly what the
/// raw block already did, side effects included. This used to also reject
/// any prefix containing a side-effecting call (e.g. `while (some_func(x)
/// != 0)`'s condition-computing call), but a call re-executed once per
/// loop iteration by either shape is exactly the semantics a real
/// `while(cond)` loop has -- rejecting it here just meant the loop got no
/// shape at all (all 9 collapse rules fail, backward continue-edge and
/// trailing code silently dropped) instead of a merely less-optimized one.
/// Only the STATEMENT SHAPE matters here; anything not a plain
/// assign-to-var or bare expression (nested control flow, etc.) still
/// isn't safe to carry and stays rejected.
pub fn is_trivial_structuring_stmt(stmt: &PreHirStmt) -> bool {
    matches!(
        stmt,
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(_),
            ..
        } | PreHirStmt::Expr(_)
    )
}

/// Fold a short-circuit chain block's own statements back into its condition.
///
/// A chain block must contribute nothing of its own: `b` in `a && b` runs only
/// when `a` held, so hoisting its computation above the `if` would run it
/// unconditionally. A block whose statements are only pure `t = <expr>;`
/// definitions feeding that same block's condition *does* contribute nothing --
/// substituting them back into the condition reproduces the short circuit's own
/// evaluation order exactly, and the names disappear with the block.
///
/// Returns `None` unless every statement qualifies: a plain `Var = rhs` with no
/// call in `rhs` (a call is a side effect, and evaluating it inside a condition
/// moves when it happens), and a target read at most once in what has been
/// folded so far, so nothing is duplicated.
pub fn fold_prefix_into_cond(prefix: &[PreHirStmt], cond: &PreHirExpr) -> Option<PreHirExpr> {
    let mut folded = cond.clone();
    for stmt in prefix.iter().rev() {
        let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs,
        } = stmt
        else {
            return None;
        };
        if expr_contains_call(rhs) {
            return None;
        }
        if count_var_reads(&folded, name) != 1 {
            return None;
        }
        folded = substitute_var(&folded, name, rhs);
    }
    Some(folded)
}

fn expr_contains_call(e: &PreHirExpr) -> bool {
    match e {
        PreHirExpr::Call { .. } => true,
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(..) => false,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => expr_contains_call(expr),
        PreHirExpr::Binary { lhs, rhs, .. } => expr_contains_call(lhs) || expr_contains_call(rhs),
        PreHirExpr::Index { base, index, .. } => {
            expr_contains_call(base) || expr_contains_call(index)
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_call(cond)
                || expr_contains_call(then_expr)
                || expr_contains_call(else_expr)
        }
    }
}

fn count_var_reads(e: &PreHirExpr, name: &str) -> usize {
    match e {
        PreHirExpr::Var(n) => usize::from(n == name),
        PreHirExpr::AddressOfGlobal(_) | PreHirExpr::AddressOfLocal(_) | PreHirExpr::Const(..) => 0,
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. } => count_var_reads(expr, name),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            count_var_reads(lhs, name) + count_var_reads(rhs, name)
        }
        PreHirExpr::Index { base, index, .. } => {
            count_var_reads(base, name) + count_var_reads(index, name)
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_var_reads(cond, name)
                + count_var_reads(then_expr, name)
                + count_var_reads(else_expr, name)
        }
        PreHirExpr::Call { args, .. } => args.iter().map(|a| count_var_reads(a, name)).sum(),
    }
}

fn substitute_var(e: &PreHirExpr, name: &str, value: &PreHirExpr) -> PreHirExpr {
    let sub = |x: &PreHirExpr| Box::new(substitute_var(x, name, value));
    match e {
        PreHirExpr::Var(n) if n == name => value.clone(),
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(..) => e.clone(),
        PreHirExpr::Cast { expr, ty } => PreHirExpr::Cast {
            expr: sub(expr),
            ty: ty.clone(),
        },
        PreHirExpr::Unary { op, expr, ty } => PreHirExpr::Unary {
            op: *op,
            expr: sub(expr),
            ty: ty.clone(),
        },
        PreHirExpr::Load { ptr, ty } => PreHirExpr::Load {
            ptr: sub(ptr),
            ty: ty.clone(),
        },
        PreHirExpr::Binary { op, lhs, rhs, ty } => PreHirExpr::Binary {
            op: *op,
            lhs: sub(lhs),
            rhs: sub(rhs),
            ty: ty.clone(),
        },
        other => other.clone(),
    }
}

fn forward_join_idx_from_address(
    host: &impl StructuringHost,
    origin_idx: usize,
    address: u64,
) -> Option<usize> {
    host.find_block_index_by_address(address)
        .filter(|join_idx| *join_idx > origin_idx)
}

fn is_forward_exit_from(origin_idx: usize, exit: LinearExit) -> bool {
    match exit {
        LinearExit::Join(join_idx) => join_idx > origin_idx,
        LinearExit::Return | LinearExit::End => true,
    }
}

fn shared_forward_linear_exit(
    host: &mut impl StructuringHost,
    origin_idx: usize,
    lhs_idx: usize,
    rhs_idx: usize,
) -> Result<Option<LinearExit>, MlilPreviewError> {
    let Some(exit) = host.shared_linear_exit(lhs_idx, rhs_idx)? else {
        return Ok(None);
    };
    if is_forward_exit_from(origin_idx, exit) {
        Ok(Some(exit))
    } else {
        Ok(None)
    }
}

fn log_try_lower_if_reject_diag(diag: bool, idx: usize, block_addr: u64, reason: &str) {
    if diag {
        eprintln!(
            "[DIAG] try_lower_if {}: idx={} block=0x{:x}",
            reason, idx, block_addr
        );
    }
}

fn log_short_circuit_cache(
    host: &impl StructuringHost,
    diag: bool,
    kind: &str,
    start_idx: usize,
    exit: LinearExit,
) {
    if diag {
        eprintln!(
            "[DIAG] try_lower_short_circuit {} {}: start_idx={} exit={:?}",
            kind,
            if host.has_linear_body_cache(start_idx, exit) {
                "cache_hit"
            } else {
                "cache_miss"
            },
            start_idx,
            exit
        );
    }
}

#[allow(dead_code)]
fn _use_structuring_diag() {
    let _ = structuring_diag_enabled();
}
