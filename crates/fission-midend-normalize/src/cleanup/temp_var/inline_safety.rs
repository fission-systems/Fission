use super::super::condexe;
use super::super::utils::{
    collect_defined_labels, count_var_uses_in_stmt, expr_contains_var, expr_has_side_effects,
    expr_mentions_var, is_low_cost_flag_intrinsic,
};
use super::binding_liveness::stmt_defines_var;
use super::binding_recovery::collect_all_body_names_expr;
use crate::HashSet;
use crate::prelude::*;

pub(super) fn count_uses_until_redef(stmts: &[PreHirStmt], def_idx: usize, name: &str) -> usize {
    let mut total = 0usize;
    for stmt in stmts.iter().skip(def_idx + 1) {
        if stmt_redefines_temp(stmt, name) {
            break;
        }
        total = total.saturating_add(count_var_uses_in_stmt(stmt, name));
    }
    total
}

pub(super) fn expr_is_pure_copy_rhs(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Var(_) | PreHirExpr::Const(_, _) => true,
        PreHirExpr::Cast { expr, .. } => {
            matches!(expr.as_ref(), PreHirExpr::Var(_) | PreHirExpr::Const(_, _))
        }
        _ => false,
    }
}

pub(super) fn find_inline_forward_target(
    stmts: &[PreHirStmt],
    def_idx: usize,
    name: &str,
    stable_materialization: bool,
) -> Option<usize> {
    let mut scan_idx = def_idx + 1;
    while scan_idx < stmts.len() {
        let stmt = &stmts[scan_idx];
        let uses = count_var_uses_in_stmt(stmt, name);
        let redefines = stmt_redefines_temp(stmt, name);
        if redefines {
            return None;
        }
        if uses > 0 && stmt_allows_inline_target(stmt) {
            if if_arm_use_sees_changed_inputs(stmt, name, &stmts[def_idx]) {
                return None;
            }
            return Some(scan_idx);
        }
        if uses == 0 {
            if stmt_redefines_expr_dependency(stmt, &stmts[def_idx]) {
                return None;
            }
            if stmt_blocks_linear_inline_scan(stmt) {
                return None;
            }
            if stable_materialization && stmt_blocks_stable_inline_scan(stmt) {
                return None;
            }
            scan_idx += 1;
            continue;
        }
        if !stmt_allows_forward_scan(stmt) {
            return None;
        }
        return None;
    }
    None
}

/// Whether a use of `name` inside an arm of the `if` `stmt` runs after its
/// definition's inputs may have changed.
///
/// The scan that reaches the `if` checks only the statements between the
/// definition and the `if`. A use in the condition is evaluated next, but a
/// use in an arm runs after whatever precedes it there: `t = rax; if (rax) {
/// rax = 0; f(t); }` passes the old `rax`, and substituting `rax` passes 0.
pub(super) fn if_arm_use_sees_changed_inputs(
    stmt: &PreHirStmt,
    name: &str,
    defining_stmt: &PreHirStmt,
) -> bool {
    let PreHirStmt::If {
        cond,
        then_body,
        else_body,
    } = stmt
    else {
        return false;
    };
    let PreHirStmt::Assign { rhs, .. } = defining_stmt else {
        return false;
    };
    let mut inputs = HashSet::default();
    collect_all_body_names_expr(rhs, &mut inputs);
    let check = InputWriteOrder {
        name,
        inputs,
        reads_memory: condexe::expr_reads_memory(rhs) || expr_has_side_effects(rhs),
    };
    // A label in an arm can be a loop's head: a `goto` back to it runs the
    // use again after the writes that follow it.
    if [then_body, else_body].into_iter().any(|arm| {
        arm.iter()
            .any(|arm_stmt| count_var_uses_in_stmt(arm_stmt, name) > 0)
            && !collect_defined_labels(arm).is_empty()
    }) {
        return true;
    }
    let written = check.expr_writes(cond);
    check.stmts(then_body, written).is_none() || check.stmts(else_body, written).is_none()
}

/// Walks statements in execution order, asking whether a use of `name` runs
/// after a write to one of the inputs of its definition -- a variable it
/// reads, or memory when it reads memory.
struct InputWriteOrder<'a> {
    name: &'a str,
    inputs: HashSet<String>,
    reads_memory: bool,
}

impl InputWriteOrder<'_> {
    fn writes(&self, stmt: &PreHirStmt) -> bool {
        self.inputs
            .iter()
            .any(|input| stmt_defines_var(stmt, input))
            || (self.reads_memory && stmt_may_write_memory(stmt))
    }

    fn expr_writes(&self, expr: &PreHirExpr) -> bool {
        self.reads_memory && expr_has_side_effects(expr)
    }

    /// `None` when a use sees a write; otherwise whether an input has been
    /// written by the end of `stmts`, given `written` on entry.
    fn stmts(&self, stmts: &[PreHirStmt], mut written: bool) -> Option<bool> {
        for stmt in stmts {
            written = self.stmt(stmt, written)?;
        }
        Some(written)
    }

    fn stmt(&self, stmt: &PreHirStmt, written: bool) -> Option<bool> {
        if count_var_uses_in_stmt(stmt, self.name) == 0 {
            return Some(written || self.writes(stmt));
        }
        if written {
            return None;
        }
        match stmt {
            // A statement reads its operands before its own write lands.
            PreHirStmt::Assign { .. }
            | PreHirStmt::Expr(_)
            | PreHirStmt::Return(_)
            | PreHirStmt::VaStart { .. } => Some(self.writes(stmt)),
            PreHirStmt::Block(body) => self.stmts(body, false),
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let written = self.expr_writes(cond);
                let then_written = self.stmts(then_body, written)?;
                let else_written = self.stmts(else_body, written)?;
                Some(then_written || else_written)
            }
            // A later iteration runs the use after this one's writes, and a
            // case can fall through into the next.
            PreHirStmt::While { .. }
            | PreHirStmt::DoWhile { .. }
            | PreHirStmt::For { .. }
            | PreHirStmt::Switch { .. } => (!self.writes(stmt)).then_some(false),
            PreHirStmt::Label(_)
            | PreHirStmt::Goto(_)
            | PreHirStmt::Break
            | PreHirStmt::Continue => Some(false),
        }
    }
}

/// Whether `stmt`, or anything nested in it, stores to memory or makes a call.
fn stmt_may_write_memory(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            !matches!(lhs, PreHirLValue::Var(_)) || expr_has_side_effects(rhs)
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => expr_has_side_effects(expr),
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => body.iter().any(stmt_may_write_memory),
        PreHirStmt::For {
            init, update, body, ..
        } => {
            init.as_deref().is_some_and(stmt_may_write_memory)
                || update.as_deref().is_some_and(stmt_may_write_memory)
                || body.iter().any(stmt_may_write_memory)
        }
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_has_side_effects(cond)
                || then_body.iter().any(stmt_may_write_memory)
                || else_body.iter().any(stmt_may_write_memory)
        }
        PreHirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_has_side_effects(expr)
                || cases
                    .iter()
                    .any(|case| case.body.iter().any(stmt_may_write_memory))
                || default.iter().any(stmt_may_write_memory)
        }
        PreHirStmt::VaStart { .. } => true,
        PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(None)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
    }
}

fn stmt_redefines_expr_dependency(stmt: &PreHirStmt, defining_stmt: &PreHirStmt) -> bool {
    let PreHirStmt::Assign {
        lhs: PreHirLValue::Var(defined_name),
        ..
    } = stmt
    else {
        return false;
    };
    let PreHirStmt::Assign { rhs, .. } = defining_stmt else {
        return false;
    };
    expr_mentions_var(rhs, defined_name)
}

fn stmt_blocks_linear_inline_scan(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            !matches!(lhs, PreHirLValue::Var(_)) || expr_has_side_effects(rhs)
        }
        PreHirStmt::Expr(expr) => expr_has_side_effects(expr),
        PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Block(_)
        | PreHirStmt::Switch { .. }
        | PreHirStmt::If { .. }
        | PreHirStmt::While { .. }
        | PreHirStmt::DoWhile { .. }
        | PreHirStmt::For { .. }
        | PreHirStmt::Break
        | PreHirStmt::Continue => true,
    }
}

fn stmt_blocks_stable_inline_scan(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            !matches!(lhs, PreHirLValue::Var(_)) || expr_has_side_effects(rhs)
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => expr_has_side_effects(expr),
        PreHirStmt::Label(_) => false,
        PreHirStmt::Return(None)
        | PreHirStmt::VaStart { .. }
        | PreHirStmt::Block(_)
        | PreHirStmt::Switch { .. }
        | PreHirStmt::If { .. }
        | PreHirStmt::While { .. }
        | PreHirStmt::DoWhile { .. }
        | PreHirStmt::For { .. }
        | PreHirStmt::Goto(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => true,
    }
}

fn stmt_allows_forward_scan(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(_),
            rhs,
        } => !expr_has_side_effects(rhs),
        PreHirStmt::Return(Some(expr)) => !expr_has_side_effects(expr),
        PreHirStmt::If { cond, .. } => !expr_has_side_effects(cond),
        PreHirStmt::Expr(expr) => !expr_has_side_effects(expr),
        _ => false,
    }
}

fn stmt_allows_inline_target(stmt: &PreHirStmt) -> bool {
    matches!(
        stmt,
        PreHirStmt::Assign { .. }
            | PreHirStmt::Expr(_)
            | PreHirStmt::Return(_)
            | PreHirStmt::If { .. }
    )
}

fn stmt_redefines_temp(stmt: &PreHirStmt, name: &str) -> bool {
    matches!(
        stmt,
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(lhs_name),
            ..
        } if lhs_name == name
    )
}

pub(super) fn stmt_uses_var_in_predicate_position(stmt: &PreHirStmt, name: &str) -> bool {
    match stmt {
        PreHirStmt::If { cond, .. } => expr_contains_var(cond, name),
        PreHirStmt::While { cond, .. } | PreHirStmt::DoWhile { cond, .. } => {
            expr_contains_var(cond, name)
        }
        PreHirStmt::For {
            init, cond, update, ..
        } => {
            init.as_deref()
                .is_some_and(|stmt| stmt_uses_var_in_predicate_position(stmt, name))
                || cond
                    .as_ref()
                    .is_some_and(|expr| expr_contains_var(expr, name))
                || update
                    .as_deref()
                    .is_some_and(|stmt| stmt_uses_var_in_predicate_position(stmt, name))
        }
        PreHirStmt::Switch { expr, .. } => expr_contains_var(expr, name),
        PreHirStmt::Block(stmts) => stmts
            .iter()
            .any(|inner| stmt_uses_var_in_predicate_position(inner, name)),
        _ => false,
    }
}

pub(super) fn expr_is_low_cost_inline_candidate(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => true,
        PreHirExpr::Call { target, args, .. } if is_low_cost_flag_intrinsic(target) => {
            args.iter().all(expr_is_low_cost_inline_candidate)
        }
        PreHirExpr::Cast { expr, .. } | PreHirExpr::Unary { expr, .. } => {
            expr_is_low_cost_inline_candidate(expr)
        }
        PreHirExpr::Binary { op, lhs, rhs, .. } => {
            matches!(
                op,
                PreHirBinaryOp::Eq
                    | PreHirBinaryOp::Ne
                    | PreHirBinaryOp::Lt
                    | PreHirBinaryOp::Le
                    | PreHirBinaryOp::SLt
                    | PreHirBinaryOp::SLe
                    | PreHirBinaryOp::And
                    | PreHirBinaryOp::Or
                    | PreHirBinaryOp::Xor
                    | PreHirBinaryOp::Add
                    | PreHirBinaryOp::Sub
                    | PreHirBinaryOp::Shl
                    | PreHirBinaryOp::Shr
                    | PreHirBinaryOp::Sar
                    | PreHirBinaryOp::Mod
            ) && expr_is_low_cost_inline_candidate(lhs)
                && expr_is_low_cost_inline_candidate(rhs)
        }
        _ => false,
    }
}

pub(super) fn expr_prefers_stable_materialization(expr: &PreHirExpr) -> bool {
    match expr {
        PreHirExpr::Var(_)
        | PreHirExpr::AddressOfGlobal(_)
        | PreHirExpr::AddressOfLocal(_)
        | PreHirExpr::Const(_, _) => false,
        PreHirExpr::Cast { expr, .. } => expr_prefers_stable_materialization(expr),
        PreHirExpr::Call { target, args, .. } if is_low_cost_flag_intrinsic(target) => {
            args.iter().any(expr_prefers_stable_materialization)
        }
        PreHirExpr::Unary { .. }
        | PreHirExpr::Load { .. }
        | PreHirExpr::PtrOffset { .. }
        | PreHirExpr::Index { .. }
        | PreHirExpr::Select { .. }
        | PreHirExpr::AggregateCopy { .. }
        | PreHirExpr::FieldAccess { .. }
        | PreHirExpr::Call { .. } => true,
        PreHirExpr::Binary { op, .. } => matches!(
            op,
            PreHirBinaryOp::Add
                | PreHirBinaryOp::Sub
                | PreHirBinaryOp::Mul
                | PreHirBinaryOp::Div
                | PreHirBinaryOp::Mod
                | PreHirBinaryOp::And
                | PreHirBinaryOp::Or
                | PreHirBinaryOp::Xor
                | PreHirBinaryOp::Shl
                | PreHirBinaryOp::Shr
                | PreHirBinaryOp::Sar
                | PreHirBinaryOp::Eq
                | PreHirBinaryOp::Ne
                | PreHirBinaryOp::Lt
                | PreHirBinaryOp::Le
                | PreHirBinaryOp::SLt
                | PreHirBinaryOp::SLe
        ),
    }
}
