//! HIR presentation pass — readability-only tree polish before HIR print.
//!
//! Contract: [`docs/adr/0011-hir-presentation-contract.md`] and `render/AGENTS.md`.
//!
//! - Clone-only: NIR print uses the pre-presentation tree.
//! - Preserve evaluation count/order for calls and loads (no double-eval inlines).
//! - Structural invariants only — no function/address/binary special cases.
//! - Semantic recovery stays in normalize/structuring.
//! - Post-pass structural firewall ([`invariants`]): on violation, restore pre-polish tree.

mod alias_propagation;
mod cleanup;
mod condition_normalization;
mod goto_recovery;
mod invariants;
mod naming;
mod seed_folding;
mod switch_recovery;

use super::{
    HirBinaryOp, HirExpr, HirFunction, HirLValue, HirStmt, HirUnaryOp, NirBinding,
    NirBindingOrigin, NirType,
};
use alias_propagation::propagate_pure_var_aliases;
use cleanup::{
    drop_unused_presentation_locals, eliminate_pure_dead_assigns, simplify_presentation_casts,
};
use condition_normalization::{
    body_is_effectively_empty, canonicalize_conditions_in_expr,
    canonicalize_presentation_conditions, expr_result_type, fold_redundant_select_return_join,
    single_return_expr, single_var_assign,
};
use fission_midend_core::ir::HirSwitchCase;
use invariants::check_hir_presentation_invariants;
use seed_folding::{fold_seed_transform, fold_self_update_after_seed};
use std::collections::{HashMap, HashSet};

/// Apply HIR-facing presentation polish in place.
///
/// On structural invariant failure (use-without-def, call/load inflation, empty
/// if shells), restores the pre-polish tree so broken presentation never ships.
pub(crate) fn apply_hir_presentation(func: &mut HirFunction) {
    apply_hir_presentation_with_globals(func, &HashSet::new());
}

/// As `apply_hir_presentation`, told which names denote file-scope globals.
///
/// Presentation's dead-assignment pass reasons about values: a name nothing
/// reads back holds nothing worth printing. That is true of a local and false
/// of a global -- writing one is the observable effect, whether or not this
/// function reads it again -- so without the set it silently deleted stores
/// like `max_flush_loops = rax;` from the emitted C.
pub(crate) fn apply_hir_presentation_with_globals(
    func: &mut HirFunction,
    globals: &HashSet<String>,
) {
    let before = func.clone();
    apply_hir_presentation_passes(func, globals);
    if let Err(_violations) = check_hir_presentation_invariants(&before, func) {
        // Prefer mechanical NIR-shaped tree over observationally broken HIR polish.
        *func = before;
        #[cfg(debug_assertions)]
        {
            // Surface in debug builds without aborting release decomp paths.
            eprintln!(
                "hir presentation invariants failed; restored pre-polish tree: {_violations:?}"
            );
        }
    }
}

fn apply_hir_presentation_passes(func: &mut HirFunction, globals: &HashSet<String>) {
    for _ in 0..16 {
        let mut changed = false;
        changed |= flatten_redundant_blocks(&mut func.body);
        changed |= propagate_pure_var_aliases(func);
        changed |= fold_self_update_after_seed(&mut func.body);
        changed |= fold_seed_transform(&mut func.body);
        // Shared `goto L; ... L: return e` → direct returns (enables if-else recovery).
        changed |= goto_recovery::expand_goto_shared_returns(&mut func.body);
        changed |= collapse_trivial_assign_returns(&mut func.body);
        // O0-style `if (c) goto L; body; L:` → structured if/else for readability.
        let mut global_goto_refs = HashMap::new();
        goto_recovery::collect_goto_ref_counts(&func.body, &mut global_goto_refs);
        changed |= goto_recovery::recover_if_else_from_gotos(&mut func.body, &global_goto_refs);
        // O0 while: `goto Lcond; Lbody: …; Lcond: if (c) goto Lbody;`
        changed |= goto_recovery::recover_while_from_gotos(&mut func.body);
        // Structuring often emits `while (1) { if (!c) break; body }` → `while (c)`.
        changed |= fold_while_true_break_guard(&mut func.body);
        // `i = 0; while (i < n) { …; i = i + 1; }` → `for (i = 0; i < n; i = i + 1)`.
        changed |= fold_seed_while_to_for(&mut func.body);
        // `if (x==c1) {..} else if (x==c2) {..} else ..` → `switch(x) {..}`
        // (GCC/Clang "switch lowering": a small or sparse-case switch
        // compiled to an if-chain instead of a jump table). Runs before the
        // pure-value/return-to-select folds below, which want the same
        // 2-arm `if`/`else` shape a 3+-arm chain also matches one level at
        // a time -- letting them go first would eat the chain into nested
        // ternaries before this pass ever sees it as a chain.
        changed |= switch_recovery::recover_switch_from_lowered_if_chain(&mut func.body);
        // `if (c) { x = a; } else { x = b; }` → `x = c ? a : b` (pure values only).
        changed |= fold_if_else_pure_same_var_assign(&mut func.body);
        // `if (c) { return a; } else { return b; }` → `return c ? a : b` (pure values).
        changed |= fold_if_else_pure_returns_to_select(&mut func.body);
        // `return x==10 ? 2 : x>10 ? DFLT : ...` → `switch (x) {..}` -- the
        // *other* real "switch lowering" shape (a binary-search-style
        // decision tree, already collapsed into one nested Select by the
        // fold just above), companion to the linear if-chain recovery
        // earlier in this loop. Must run after that fold settles the tree
        // shape, not before.
        changed |= switch_recovery::recover_switch_from_select_decision_tree(&mut func.body);
        // `if (c) { return a; } return b;` → `return c ? a : b` (pure values).
        changed |= fold_if_return_fallthrough_return(&mut func.body);
        // `x = seed; if (c) { x = a; }` → `x = c ? a : seed` (pure; empty else).
        changed |= fold_seed_if_overwrite_assign(&mut func.body);
        // Drop empty `else {}` arms after other folds.
        changed |= strip_empty_else_arms(&mut func.body);
        // `if (c) {} else { body }` → `if (!c) { body }`.
        changed |= fold_empty_then_invert_else(&mut func.body);
        // Prefer `x op k` over `k op x` in comparisons (and peel `!(eq/ne)` etc.).
        changed |= canonicalize_presentation_conditions(func);
        // `x = c ? a : b; return ~c ? x : a` (null-check join) → `return x` / fold.
        changed |= fold_redundant_select_return_join(&mut func.body);
        let mut goto_targets = HashSet::new();
        goto_recovery::collect_goto_targets(&func.body, &mut goto_targets);
        changed |=
            goto_recovery::prune_unreachable_after_total_return(&mut func.body, &goto_targets);
        changed |= inline_single_use_pure_assigns(func);
        changed |= eliminate_pure_dead_assigns(func, globals);
        changed |= goto_recovery::remove_unreferenced_labels(&mut func.body);
        if !changed {
            break;
        }
    }
    // Once, after the structural fixed-point settles: loop-counter detection
    // needs the final While/For/DoWhile shape, and running once avoids
    // transient intermediate shapes producing conflicting guesses.
    naming::apply_semantic_naming(func);
    simplify_presentation_casts(func);
    drop_unused_presentation_locals(func);
}

// ── Pure expression helpers ──────────────────────────────────────────────────

fn is_presentation_pure_intrinsic(target: &str) -> bool {
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

fn expr_is_presentation_pure(expr: &HirExpr) -> bool {
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
fn expr_is_movable_read(expr: &HirExpr) -> bool {
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
fn memory_clobbered_between(stmts: &[HirStmt], from: usize, to: usize) -> bool {
    stmts[from..to.min(stmts.len())]
        .iter()
        .any(stmt_clobbers_memory)
}

fn stmt_clobbers_memory(stmt: &HirStmt) -> bool {
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

fn expr_mentions_var(expr: &HirExpr, name: &str) -> bool {
    match expr {
        HirExpr::Var(n) => n == name,
        HirExpr::AddressOfGlobal(_) | HirExpr::AddressOfLocal(_) | HirExpr::Const(_, _) => false,
        HirExpr::Unary { expr, .. } | HirExpr::Cast { expr, .. } => expr_mentions_var(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_mentions_var(lhs, name) || expr_mentions_var(rhs, name)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_mentions_var(cond, name)
                || expr_mentions_var(then_expr, name)
                || expr_mentions_var(else_expr, name)
        }
        HirExpr::Call { args, .. } => args.iter().any(|a| expr_mentions_var(a, name)),
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. }
        | HirExpr::AggregateCopy { src: ptr, .. } => expr_mentions_var(ptr, name),
        HirExpr::Index { base, index, .. } => {
            expr_mentions_var(base, name) || expr_mentions_var(index, name)
        }
    }
}

fn replace_var_in_expr(expr: &mut HirExpr, name: &str, replacement: &HirExpr) {
    match expr {
        HirExpr::Var(var) if var == name => *expr = replacement.clone(),
        HirExpr::Var(_)
        | HirExpr::AddressOfGlobal(_)
        | HirExpr::AddressOfLocal(_)
        | HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
            replace_var_in_expr(expr, name, replacement)
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            replace_var_in_expr(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            replace_var_in_expr(cond, name, replacement);
            replace_var_in_expr(then_expr, name, replacement);
            replace_var_in_expr(else_expr, name, replacement);
        }
        HirExpr::Call { args, .. } => {
            for a in args {
                replace_var_in_expr(a, name, replacement);
            }
        }
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. }
        | HirExpr::AggregateCopy { src: ptr, .. } => replace_var_in_expr(ptr, name, replacement),
        HirExpr::Index { base, index, .. } => {
            replace_var_in_expr(base, name, replacement);
            replace_var_in_expr(index, name, replacement);
        }
    }
}

fn replace_var_in_lvalue(lhs: &mut HirLValue, name: &str, replacement: &HirExpr) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => replace_var_in_expr(ptr, name, replacement),
        HirLValue::Index { base, index, .. } => {
            replace_var_in_expr(base, name, replacement);
            replace_var_in_expr(index, name, replacement);
        }
        HirLValue::FieldAccess { base, .. } => replace_var_in_expr(base, name, replacement),
    }
}

fn replace_var_in_stmt(stmt: &mut HirStmt, name: &str, replacement: &HirExpr) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            replace_var_in_lvalue(lhs, name, replacement);
            replace_var_in_expr(rhs, name, replacement);
        }
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) | HirStmt::VaStart { va_list: e, .. } => {
            replace_var_in_expr(e, name, replacement)
        }
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
        HirStmt::Block(body) => {
            for s in body {
                replace_var_in_stmt(s, name, replacement);
            }
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            replace_var_in_expr(cond, name, replacement);
            for s in body {
                replace_var_in_stmt(s, name, replacement);
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            replace_var_in_expr(cond, name, replacement);
            for s in then_body {
                replace_var_in_stmt(s, name, replacement);
            }
            for s in else_body {
                replace_var_in_stmt(s, name, replacement);
            }
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                replace_var_in_stmt(i, name, replacement);
            }
            if let Some(c) = cond {
                replace_var_in_expr(c, name, replacement);
            }
            if let Some(u) = update {
                replace_var_in_stmt(u, name, replacement);
            }
            for s in body {
                replace_var_in_stmt(s, name, replacement);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            replace_var_in_expr(expr, name, replacement);
            for case in cases {
                for s in &mut case.body {
                    replace_var_in_stmt(s, name, replacement);
                }
            }
            for s in default {
                replace_var_in_stmt(s, name, replacement);
            }
        }
    }
}

// ── Counts ───────────────────────────────────────────────────────────────────

fn count_defs_in_stmts(stmts: &[HirStmt], out: &mut HashMap<String, usize>) {
    for s in stmts {
        count_defs_in_stmt(s, out);
    }
}

fn count_defs_in_stmt(stmt: &HirStmt, out: &mut HashMap<String, usize>) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            ..
        } => {
            *out.entry(name.clone()).or_default() += 1;
        }
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            count_defs_in_stmts(body, out)
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            count_defs_in_stmts(then_body, out);
            count_defs_in_stmts(else_body, out);
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                count_defs_in_stmt(i, out);
            }
            if let Some(u) = update {
                count_defs_in_stmt(u, out);
            }
            count_defs_in_stmts(body, out);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                count_defs_in_stmts(&case.body, out);
            }
            count_defs_in_stmts(default, out);
        }
        _ => {}
    }
}

fn count_uses_in_stmts(stmts: &[HirStmt], name: &str) -> usize {
    stmts.iter().map(|s| count_uses_in_stmt(s, name)).sum()
}

fn count_uses_in_stmt(stmt: &HirStmt, name: &str) -> usize {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            count_uses_in_lvalue(lhs, name) + count_uses_in_expr(rhs, name)
        }
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) | HirStmt::VaStart { va_list: e, .. } => {
            count_uses_in_expr(e, name)
        }
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => 0,
        HirStmt::Block(body) => count_uses_in_stmts(body, name),
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            count_uses_in_expr(cond, name) + count_uses_in_stmts(body, name)
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_uses_in_expr(cond, name)
                + count_uses_in_stmts(then_body, name)
                + count_uses_in_stmts(else_body, name)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_ref().map_or(0, |s| count_uses_in_stmt(s, name))
                + cond.as_ref().map_or(0, |e| count_uses_in_expr(e, name))
                + update.as_ref().map_or(0, |s| count_uses_in_stmt(s, name))
                + count_uses_in_stmts(body, name)
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_uses_in_expr(expr, name)
                + cases
                    .iter()
                    .map(|c| count_uses_in_stmts(&c.body, name))
                    .sum::<usize>()
                + count_uses_in_stmts(default, name)
        }
    }
}

fn count_uses_in_lvalue(lhs: &HirLValue, name: &str) -> usize {
    match lhs {
        HirLValue::Var(_) => 0,
        HirLValue::Deref { ptr, .. } => count_uses_in_expr(ptr, name),
        HirLValue::Index { base, index, .. } => {
            count_uses_in_expr(base, name) + count_uses_in_expr(index, name)
        }
        HirLValue::FieldAccess { base, .. } => count_uses_in_expr(base, name),
    }
}

fn count_uses_in_expr(expr: &HirExpr, name: &str) -> usize {
    match expr {
        HirExpr::Var(n) | HirExpr::AddressOfLocal(n) => usize::from(n == name),
        HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => 0,
        HirExpr::Unary { expr, .. } | HirExpr::Cast { expr, .. } => count_uses_in_expr(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => {
            count_uses_in_expr(lhs, name) + count_uses_in_expr(rhs, name)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_uses_in_expr(cond, name)
                + count_uses_in_expr(then_expr, name)
                + count_uses_in_expr(else_expr, name)
        }
        HirExpr::Call { args, .. } => args.iter().map(|a| count_uses_in_expr(a, name)).sum(),
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. }
        | HirExpr::AggregateCopy { src: ptr, .. } => count_uses_in_expr(ptr, name),
        HirExpr::Index { base, index, .. } => {
            count_uses_in_expr(base, name) + count_uses_in_expr(index, name)
        }
    }
}

// ── Passes ───────────────────────────────────────────────────────────────────

fn flatten_redundant_blocks(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut out = Vec::with_capacity(stmts.len());
    for stmt in std::mem::take(stmts) {
        match stmt {
            HirStmt::Block(mut body) => {
                changed |= flatten_redundant_blocks(&mut body);
                // Unwrap brace-only wrappers into the parent sequence for HIR.
                out.extend(body);
                changed = true;
            }
            mut other => {
                match &mut other {
                    HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                        changed |= flatten_redundant_blocks(body);
                    }
                    HirStmt::If {
                        then_body,
                        else_body,
                        ..
                    } => {
                        changed |= flatten_redundant_blocks(then_body);
                        changed |= flatten_redundant_blocks(else_body);
                    }
                    HirStmt::For { body, .. } => {
                        changed |= flatten_redundant_blocks(body);
                    }
                    HirStmt::Switch { cases, default, .. } => {
                        for case in cases {
                            changed |= flatten_redundant_blocks(&mut case.body);
                        }
                        changed |= flatten_redundant_blocks(default);
                    }
                    _ => {}
                }
                out.push(other);
            }
        }
    }
    *stmts = out;
    changed
}

/// Occurrences of `name` in `expr`.
fn count_var_mentions(expr: &HirExpr, name: &str) -> usize {
    match expr {
        HirExpr::Var(n) => usize::from(n == name),
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::FieldAccess { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => count_var_mentions(expr, name),
        HirExpr::Binary { lhs, rhs, .. } => {
            count_var_mentions(lhs, name) + count_var_mentions(rhs, name)
        }
        HirExpr::Index { base, index, .. } => {
            count_var_mentions(base, name) + count_var_mentions(index, name)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            count_var_mentions(cond, name)
                + count_var_mentions(then_expr, name)
                + count_var_mentions(else_expr, name)
        }
        HirExpr::Call { args, .. } => args.iter().map(|a| count_var_mentions(a, name)).sum(),
        _ => 0,
    }
}

/// Replace every `Var(name)` in `expr` with `value`.
fn substitute_var(expr: &mut HirExpr, name: &str, value: &HirExpr) {
    match expr {
        HirExpr::Var(n) if n == name => *expr = value.clone(),
        HirExpr::Var(_) => {}
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::FieldAccess { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. } => substitute_var(inner, name, value),
        HirExpr::Binary { lhs, rhs, .. } => {
            substitute_var(lhs, name, value);
            substitute_var(rhs, name, value);
        }
        HirExpr::Index { base, index, .. } => {
            substitute_var(base, name, value);
            substitute_var(index, name, value);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            substitute_var(cond, name, value);
            substitute_var(then_expr, name, value);
            substitute_var(else_expr, name, value);
        }
        HirExpr::Call { args, .. } => {
            for a in args {
                substitute_var(a, name, value);
            }
        }
        _ => {}
    }
}

/// Collapse `x = rhs; return x` → `return rhs` (labels between are skipped).
/// Allows call/select RHS: single evaluation is preserved.
fn collapse_trivial_assign_returns(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        let HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } = &stmts[i]
        else {
            i += 1;
            continue;
        };
        let name = name.clone();
        let rhs = rhs.clone();
        // Skip pure labels between assign and return.
        let mut j = i + 1;
        while j < stmts.len() && matches!(&stmts[j], HirStmt::Label(_)) {
            j += 1;
        }
        if j < stmts.len() {
            if let HirStmt::Return(Some(HirExpr::Var(ret))) = &stmts[j] {
                if ret == &name {
                    stmts[j] = HirStmt::Return(Some(rhs));
                    stmts.remove(i);
                    changed = true;
                    continue;
                }
            }
        }
        i += 1;
    }
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                changed |= collapse_trivial_assign_returns(body);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= collapse_trivial_assign_returns(then_body);
                changed |= collapse_trivial_assign_returns(else_body);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init_stmt) = init {
                    if let HirStmt::Block(b) = init_stmt.as_mut() {
                        changed |= collapse_trivial_assign_returns(b);
                    }
                }
                if let Some(upd) = update {
                    if let HirStmt::Block(b) = upd.as_mut() {
                        changed |= collapse_trivial_assign_returns(b);
                    }
                }
                changed |= collapse_trivial_assign_returns(body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= collapse_trivial_assign_returns(&mut case.body);
                }
                changed |= collapse_trivial_assign_returns(default);
            }
            _ => {}
        }
    }
    changed
}

/// Inline single-def, single-use pure assigns into their use site (temps + noise).
fn inline_single_use_pure_assigns(func: &mut HirFunction) -> bool {
    let formal: HashSet<&str> = func.params.iter().map(|b| b.name.as_str()).collect();
    let mut def_counts = HashMap::new();
    count_defs_in_stmts(&func.body, &mut def_counts);
    inline_single_use_in_stmts(&mut func.body, &formal, &def_counts)
}

fn inline_single_use_in_stmts(
    stmts: &mut Vec<HirStmt>,
    formal: &HashSet<&str>,
    def_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        let candidate = match &stmts[i] {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs,
            } if !formal.contains(name.as_str())
                && def_counts.get(name.as_str()).copied().unwrap_or(0) == 1
                && (expr_is_presentation_pure(rhs) || expr_is_movable_read(rhs))
                && !expr_mentions_var(rhs, name) =>
            {
                Some((name.clone(), rhs.clone()))
            }
            _ => None,
        };
        if let Some((name, rhs)) = candidate {
            // Total uses in the remaining statements (and nested).
            let uses_after: usize = stmts[i + 1..]
                .iter()
                .map(|s| count_uses_in_stmt(s, &name))
                .sum();
            // Also count uses only in this linear tail for adjacent inline.
            if uses_after == 1 {
                // Find the unique use site in the linear suffix; only inline when
                // it is an adjacent pure consumer (return / assign) without a
                // redefinition of a dependency in between. For presentation we
                // allow a short pure-assign gap only when the use is still 1.
                if let Some(target) = find_single_use_target(stmts, i + 1, &name) {
                    // `rbx = rax; …; rax = g(); rax += rbx` must not become
                    // `rax += rax` — refuse inline if any free var of the RHS
                    // is redefined before the use.
                    // A load may be moved to its use only across a span that
                    // writes no memory; a pure expression needs no such check.
                    if !expr_is_presentation_pure(&rhs)
                        && memory_clobbered_between(stmts, i + 1, target)
                    {
                        i += 1;
                        continue;
                    }
                    if pure_expr_free_var_redefined_before(stmts, i + 1, target, &rhs) {
                        i += 1;
                        continue;
                    }
                    replace_var_in_stmt(&mut stmts[target], &name, &rhs);
                    stmts.remove(i);
                    changed = true;
                    continue;
                }
            }
        }
        i += 1;
    }

    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                changed |= inline_single_use_in_stmts(body, formal, def_counts);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= inline_single_use_in_stmts(then_body, formal, def_counts);
                changed |= inline_single_use_in_stmts(else_body, formal, def_counts);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init_stmt) = init {
                    if let HirStmt::Block(b) = init_stmt.as_mut() {
                        changed |= inline_single_use_in_stmts(b, formal, def_counts);
                    }
                }
                if let Some(upd) = update {
                    if let HirStmt::Block(b) = upd.as_mut() {
                        changed |= inline_single_use_in_stmts(b, formal, def_counts);
                    }
                }
                changed |= inline_single_use_in_stmts(body, formal, def_counts);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= inline_single_use_in_stmts(&mut case.body, formal, def_counts);
                }
                changed |= inline_single_use_in_stmts(default, formal, def_counts);
            }
            _ => {}
        }
    }
    changed
}

/// True if any free variable of a pure expression is assigned on `[start, end)`.
fn pure_expr_free_var_redefined_before(
    stmts: &[HirStmt],
    start: usize,
    end: usize,
    expr: &HirExpr,
) -> bool {
    let mut free = HashSet::new();
    collect_free_vars_in_expr(expr, &mut free);
    if free.is_empty() {
        return false;
    }
    for stmt in stmts.iter().take(end).skip(start) {
        if let HirStmt::Assign {
            lhs: HirLValue::Var(n),
            ..
        } = stmt
        {
            if free.contains(n.as_str()) {
                return true;
            }
        }
    }
    false
}

fn collect_free_vars_in_expr(expr: &HirExpr, out: &mut HashSet<String>) {
    match expr {
        HirExpr::Var(n) | HirExpr::AddressOfLocal(n) => {
            out.insert(n.clone());
        }
        HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
        HirExpr::Unary { expr, .. } | HirExpr::Cast { expr, .. } => {
            collect_free_vars_in_expr(expr, out);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_free_vars_in_expr(lhs, out);
            collect_free_vars_in_expr(rhs, out);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_free_vars_in_expr(cond, out);
            collect_free_vars_in_expr(then_expr, out);
            collect_free_vars_in_expr(else_expr, out);
        }
        HirExpr::Call { args, .. } => {
            for a in args {
                collect_free_vars_in_expr(a, out);
            }
        }
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. }
        | HirExpr::AggregateCopy { src: ptr, .. } => collect_free_vars_in_expr(ptr, out),
        HirExpr::Index { base, index, .. } => {
            collect_free_vars_in_expr(base, out);
            collect_free_vars_in_expr(index, out);
        }
    }
}

fn find_single_use_target(stmts: &[HirStmt], start: usize, name: &str) -> Option<usize> {
    let mut found = None;
    for (idx, stmt) in stmts.iter().enumerate().skip(start) {
        let uses = count_uses_in_stmt(stmt, name);
        if uses > 0 {
            if found.is_some() {
                return None;
            }
            // Assign / return / expr, or predicate-only use in if/while.
            let ok = match stmt {
                HirStmt::Assign { .. } | HirStmt::Return(_) | HirStmt::Expr(_) => uses == 1,
                HirStmt::If {
                    cond,
                    then_body,
                    else_body,
                } => {
                    count_uses_in_expr(cond, name) == uses
                        && count_uses_in_stmts(then_body, name) == 0
                        && count_uses_in_stmts(else_body, name) == 0
                }
                HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
                    count_uses_in_expr(cond, name) == uses && count_uses_in_stmts(body, name) == 0
                }
                _ => false,
            };
            if !ok {
                return None;
            }
            found = Some(idx);
        }
        // Stop scanning past control-flow barriers without a use.
        if uses == 0
            && matches!(
                stmt,
                HirStmt::If { .. }
                    | HirStmt::While { .. }
                    | HirStmt::DoWhile { .. }
                    | HirStmt::For { .. }
                    | HirStmt::Switch { .. }
                    | HirStmt::Goto(_)
                    | HirStmt::Label(_)
                    | HirStmt::Break
                    | HirStmt::Continue
            )
        {
            return None;
        }
    }
    found
}

/// Pure assign of a single var: `name = pure_expr`.
fn pure_var_assign(stmt: &HirStmt) -> Option<(&str, &HirExpr)> {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } if expr_is_presentation_pure(rhs) => Some((name.as_str(), rhs)),
        _ => None,
    }
}

/// `i = init; while (cond) { body…; i = update; }` → `for (i = init; cond; i = update)`.
///
/// Presentation-only: init/update must be presentation-pure. Body may contain
/// calls (loop body is not reordered). Rejects multi-def induction or missing tail.
fn fold_seed_while_to_for(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= fold_seed_while_to_for(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= fold_seed_while_to_for(then_body);
                changed |= fold_seed_while_to_for(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= fold_seed_while_to_for(&mut case.body);
                }
                changed |= fold_seed_while_to_for(default);
            }
            _ => {}
        }
    }

    let mut i = 0;
    while i + 1 < stmts.len() {
        let Some((ind_name, _)) = pure_var_assign(&stmts[i]) else {
            i += 1;
            continue;
        };
        let ind_name = ind_name.to_string();
        let HirStmt::While { cond, body } = &stmts[i + 1] else {
            i += 1;
            continue;
        };
        if body.is_empty() {
            i += 1;
            continue;
        }
        let Some((upd_name, _)) = pure_var_assign(body.last().unwrap()) else {
            i += 1;
            continue;
        };
        if upd_name != ind_name {
            i += 1;
            continue;
        }
        // Cond should mention induction (avoid folding unrelated seed+while).
        if count_uses_in_expr(cond, &ind_name) == 0 {
            i += 1;
            continue;
        }
        // Body (excluding trailing update) must not re-assign induction (for clarity).
        let body_core = &body[..body.len() - 1];
        if body_core.iter().any(|s| assigns_var_name(s, &ind_name)) {
            i += 1;
            continue;
        }

        let init_stmt = stmts[i].clone();
        let HirStmt::While { cond, body } = stmts[i + 1].clone() else {
            unreachable!();
        };
        let update_stmt = body.last().cloned().unwrap();
        let for_body: Vec<HirStmt> = body[..body.len() - 1].to_vec();
        stmts[i] = HirStmt::For {
            init: Some(Box::new(init_stmt)),
            cond: Some(cond),
            update: Some(Box::new(update_stmt)),
            body: for_body,
        };
        stmts.remove(i + 1);
        changed = true;
        // Re-scan from same index (new For may nest folds later via recursion).
        i += 1;
    }
    changed
}

fn assigns_var_name(stmt: &HirStmt, name: &str) -> bool {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(n),
            ..
        } => n == name,
        HirStmt::Block(b) | HirStmt::While { body: b, .. } | HirStmt::DoWhile { body: b, .. } => {
            b.iter().any(|s| assigns_var_name(s, name))
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            init.as_ref().is_some_and(|s| assigns_var_name(s, name))
                || update.as_ref().is_some_and(|s| assigns_var_name(s, name))
                || body.iter().any(|s| assigns_var_name(s, name))
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            then_body.iter().any(|s| assigns_var_name(s, name))
                || else_body.iter().any(|s| assigns_var_name(s, name))
        }
        HirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|c| c.body.iter().any(|s| assigns_var_name(s, name)))
                || default.iter().any(|s| assigns_var_name(s, name))
        }
        _ => false,
    }
}

/// `while (1) { if (!cond) break; body… }` → `while (cond) { body… }`.
fn fold_while_true_break_guard(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= fold_while_true_break_guard(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= fold_while_true_break_guard(then_body);
                changed |= fold_while_true_break_guard(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= fold_while_true_break_guard(&mut case.body);
                }
                changed |= fold_while_true_break_guard(default);
            }
            _ => {}
        }
    }

    for stmt in stmts.iter_mut() {
        let HirStmt::While { cond, body } = stmt else {
            continue;
        };
        if !expr_is_constant_true(cond) {
            continue;
        }
        if body.is_empty() {
            continue;
        }
        // Leading: if (<guard>) break;
        let guard_cond = match &body[0] {
            HirStmt::If {
                cond: g,
                then_body,
                else_body,
            } if else_body.is_empty() && matches!(then_body.as_slice(), [HirStmt::Break]) => {
                g.clone()
            }
            _ => continue,
        };

        // while (1) { if (!c) break; … } → while (c)
        // while (1) { if (c) break; … }  → while (!c)
        let new_cond = match peel_not(&guard_cond) {
            Some(inner) => inner,
            None => goto_recovery::invert_cond(guard_cond),
        };
        *cond = new_cond;
        body.remove(0);
        changed = true;
    }
    changed
}

fn expr_is_constant_true(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Const(v, _) => *v != 0,
        HirExpr::Cast { expr, .. } => expr_is_constant_true(expr),
        _ => false,
    }
}

fn peel_not(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } => Some(expr.as_ref().clone()),
        // `x == 0` / `x == false` → peel to truthiness of x as break-on-zero guard.
        HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if matches!(rhs.as_ref(), HirExpr::Const(0, _)) => Some(lhs.as_ref().clone()),
        HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } if matches!(lhs.as_ref(), HirExpr::Const(0, _)) => Some(rhs.as_ref().clone()),
        _ => None,
    }
}

/// `if (c) { x = a; } else { x = b; }` → `x = c ? a : b` when c/a/b are presentation-pure.
fn fold_if_else_pure_same_var_assign(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= fold_if_else_pure_same_var_assign(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= fold_if_else_pure_same_var_assign(then_body);
                changed |= fold_if_else_pure_same_var_assign(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= fold_if_else_pure_same_var_assign(&mut case.body);
                }
                changed |= fold_if_else_pure_same_var_assign(default);
            }
            _ => {}
        }
    }

    for stmt in stmts.iter_mut() {
        let HirStmt::If {
            cond,
            then_body,
            else_body,
        } = stmt
        else {
            continue;
        };
        if else_body.is_empty() || !expr_is_presentation_pure(cond) {
            continue;
        }
        let Some((then_name, then_rhs)) = single_var_assign(then_body) else {
            continue;
        };
        let Some((else_name, else_rhs)) = single_var_assign(else_body) else {
            continue;
        };
        if then_name != else_name {
            continue;
        }
        if !expr_is_presentation_pure(then_rhs) || !expr_is_presentation_pure(else_rhs) {
            continue;
        }
        // Avoid self-referential select: `x = c ? x : e` would be wrong if x is live-in.
        if expr_mentions_var(cond, then_name)
            || expr_mentions_var(then_rhs, then_name)
            || expr_mentions_var(else_rhs, then_name)
        {
            continue;
        }
        let ty = expr_result_type(then_rhs);
        *stmt = HirStmt::Assign {
            lhs: HirLValue::Var(then_name.to_string()),
            rhs: HirExpr::Select {
                cond: Box::new(cond.clone()),
                then_expr: Box::new(then_rhs.clone()),
                else_expr: Box::new(else_rhs.clone()),
                ty,
            },
        };
        changed = true;
    }
    changed
}

/// `if (c) { return a; } else { return b; }` → `return c ? a : b` (pure operands).
fn fold_if_else_pure_returns_to_select(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= fold_if_else_pure_returns_to_select(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= fold_if_else_pure_returns_to_select(then_body);
                changed |= fold_if_else_pure_returns_to_select(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= fold_if_else_pure_returns_to_select(&mut case.body);
                }
                changed |= fold_if_else_pure_returns_to_select(default);
            }
            _ => {}
        }
    }

    for stmt in stmts.iter_mut() {
        let HirStmt::If {
            cond,
            then_body,
            else_body,
        } = stmt
        else {
            continue;
        };
        if else_body.is_empty() || !expr_is_presentation_pure(cond) {
            continue;
        }
        let Some(then_ret) = single_return_expr(then_body) else {
            continue;
        };
        let Some(else_ret) = single_return_expr(else_body) else {
            continue;
        };
        if !expr_is_presentation_pure(then_ret) || !expr_is_presentation_pure(else_ret) {
            continue;
        }
        let ty = expr_result_type(then_ret);
        *stmt = HirStmt::Return(Some(HirExpr::Select {
            cond: Box::new(cond.clone()),
            then_expr: Box::new(then_ret.clone()),
            else_expr: Box::new(else_ret.clone()),
            ty,
        }));
        changed = true;
    }
    changed
}

/// `if (c) { return a; } return b;` → `return c ? a : b` when operands are pure.
fn fold_if_return_fallthrough_return(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= fold_if_return_fallthrough_return(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= fold_if_return_fallthrough_return(then_body);
                changed |= fold_if_return_fallthrough_return(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= fold_if_return_fallthrough_return(&mut case.body);
                }
                changed |= fold_if_return_fallthrough_return(default);
            }
            _ => {}
        }
    }

    let mut i = 0;
    while i < stmts.len() {
        let HirStmt::If {
            cond,
            then_body,
            else_body,
        } = &stmts[i]
        else {
            i += 1;
            continue;
        };
        if !else_body.is_empty() && !body_is_effectively_empty(else_body) {
            i += 1;
            continue;
        }
        if !expr_is_presentation_pure(cond) {
            i += 1;
            continue;
        }
        let Some(then_ret) = single_return_expr(then_body) else {
            i += 1;
            continue;
        };
        if !expr_is_presentation_pure(then_ret) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < stmts.len() && matches!(&stmts[j], HirStmt::Label(_)) {
            j += 1;
        }
        let Some(HirStmt::Return(Some(else_ret))) = stmts.get(j) else {
            i += 1;
            continue;
        };
        if !expr_is_presentation_pure(else_ret) {
            i += 1;
            continue;
        }
        let ty = expr_result_type(then_ret);
        let select = HirExpr::Select {
            cond: Box::new(cond.clone()),
            then_expr: Box::new(then_ret.clone()),
            else_expr: Box::new(else_ret.clone()),
            ty,
        };
        // Drop labels between if and fallthrough return, then replace both.
        stmts.drain(i..=j);
        stmts.insert(i, HirStmt::Return(Some(select)));
        changed = true;
        // Stay at i to allow chained folds on the new return.
        i += 1;
    }
    changed
}

/// `x = seed; if (c) { x = a; }` → `x = c ? a : seed` (presentation-pure, empty else).
/// Cond/then may mention `x`; they are rewritten with `seed` substituted.
fn fold_seed_if_overwrite_assign(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= fold_seed_if_overwrite_assign(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= fold_seed_if_overwrite_assign(then_body);
                changed |= fold_seed_if_overwrite_assign(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= fold_seed_if_overwrite_assign(&mut case.body);
                }
                changed |= fold_seed_if_overwrite_assign(default);
            }
            _ => {}
        }
    }

    let mut i = 0;
    while i + 1 < stmts.len() {
        let HirStmt::Assign {
            lhs: HirLValue::Var(seed_name),
            rhs: seed_rhs,
        } = &stmts[i]
        else {
            i += 1;
            continue;
        };
        if !expr_is_presentation_pure(seed_rhs) || expr_mentions_var(seed_rhs, seed_name) {
            i += 1;
            continue;
        }
        let seed_name = seed_name.clone();
        let seed_rhs = seed_rhs.clone();

        let mut j = i + 1;
        while j < stmts.len() && matches!(&stmts[j], HirStmt::Label(_)) {
            j += 1;
        }
        let Some(HirStmt::If {
            cond,
            then_body,
            else_body,
        }) = stmts.get(j)
        else {
            i += 1;
            continue;
        };
        if !else_body.is_empty() && !body_is_effectively_empty(else_body) {
            i += 1;
            continue;
        }
        let Some((then_name, then_rhs)) = single_var_assign(then_body) else {
            i += 1;
            continue;
        };
        if then_name != seed_name {
            i += 1;
            continue;
        }
        if !expr_is_presentation_pure(cond) || !expr_is_presentation_pure(then_rhs) {
            i += 1;
            continue;
        }

        let mut cond = cond.clone();
        let mut then_rhs = then_rhs.clone();
        replace_var_in_expr(&mut cond, &seed_name, &seed_rhs);
        replace_var_in_expr(&mut then_rhs, &seed_name, &seed_rhs);
        // After substitution, reject residual self-reference (shouldn't happen for pure seeds).
        if expr_mentions_var(&cond, &seed_name)
            || expr_mentions_var(&then_rhs, &seed_name)
            || expr_mentions_var(&seed_rhs, &seed_name)
        {
            i += 1;
            continue;
        }
        if !expr_is_presentation_pure(&cond) || !expr_is_presentation_pure(&then_rhs) {
            i += 1;
            continue;
        }

        let ty = expr_result_type(&then_rhs);
        let select = HirExpr::Select {
            cond: Box::new(cond),
            then_expr: Box::new(then_rhs),
            else_expr: Box::new(seed_rhs),
            ty,
        };
        stmts.drain(i..=j);
        stmts.insert(
            i,
            HirStmt::Assign {
                lhs: HirLValue::Var(seed_name),
                rhs: select,
            },
        );
        changed = true;
        i += 1;
    }
    changed
}

/// Drop empty `else {}` arms (including nested).
fn strip_empty_else_arms(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= strip_empty_else_arms(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= strip_empty_else_arms(then_body);
                changed |= strip_empty_else_arms(else_body);
                if !else_body.is_empty() && body_is_effectively_empty(else_body) {
                    else_body.clear();
                    changed = true;
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= strip_empty_else_arms(&mut case.body);
                }
                changed |= strip_empty_else_arms(default);
            }
            _ => {}
        }
    }
    changed
}

/// `if (c) {} else { body }` → `if (!c) { body }` (labels-only then counts as empty).
fn fold_empty_then_invert_else(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= fold_empty_then_invert_else(b);
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                changed |= fold_empty_then_invert_else(then_body);
                changed |= fold_empty_then_invert_else(else_body);
                if body_is_effectively_empty(then_body)
                    && !else_body.is_empty()
                    && !body_is_effectively_empty(else_body)
                {
                    *cond = goto_recovery::invert_cond(std::mem::replace(
                        cond,
                        HirExpr::Const(0, NirType::Bool),
                    ));
                    std::mem::swap(then_body, else_body);
                    else_body.clear();
                    changed = true;
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= fold_empty_then_invert_else(&mut case.body);
                }
                changed |= fold_empty_then_invert_else(default);
            }
            _ => {}
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midend::MlilPreviewOptions;
    use crate::render::render_layered_pseudocode;

    fn int_ty(bits: u32, signed: bool) -> NirType {
        NirType::Int { bits, signed }
    }

    fn local(name: &str) -> NirBinding {
        NirBinding {
            name: name.into(),
            ty: int_ty(32, true),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::Temp),
            initializer: None,
        }
    }

    fn param(name: &str) -> NirBinding {
        NirBinding {
            name: name.into(),
            ty: int_ty(32, true),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        }
    }

    #[test]
    fn hir_presentation_keeps_a_write_to_a_stack_slot_nothing_reads_by_name() {
        // A stack slot's store is observable through a pointer into the
        // frame, so an unread *name* is not evidence the write is dead --
        // `main` builds a six-byte string across two slots and passes the
        // address of the first.
        //
        // The binding arrives with a `Temp` origin on purpose: that is what a
        // slot looks like here once a name-based liveness pass has pruned it
        // and `rescue_undeclared_bindings` has put it back, which is exactly
        // how the second half of that string was lost. The name is all that is
        // left to recognise it by.
        let mut func = HirFunction {
            name: "f".into(),
            params: vec![],
            locals: vec![local("local_2"), local("x")],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("local_2".into()),
                    rhs: HirExpr::Const(111, int_ty(32, true)),
                },
                HirStmt::Return(Some(HirExpr::Var("x".into()))),
            ],
            ..Default::default()
        };
        apply_hir_presentation_with_globals(&mut func, &HashSet::new());
        assert!(
            func.body.iter().any(|stmt| matches!(
                stmt,
                HirStmt::Assign {
                    lhs: HirLValue::Var(name),
                    ..
                } if name == "local_2"
            )),
            "the write to a stack slot must survive:\n{:?}",
            func.body
        );
    }

    #[test]
    fn hir_presentation_keeps_a_write_to_a_global_nothing_reads() {
        // A store to a global is the observable effect, whether or not this
        // function reads it back -- the dead-assignment pass reasons about
        // values and would otherwise delete it from the emitted C.
        let mut func = HirFunction {
            name: "f".into(),
            params: vec![],
            locals: vec![local("x")],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("max_flush_loops".into()),
                    rhs: HirExpr::Var("x".into()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("dead_local".into()),
                    rhs: HirExpr::Var("x".into()),
                },
                HirStmt::Return(Some(HirExpr::Var("x".into()))),
            ],
            ..Default::default()
        };
        let globals: HashSet<String> = ["max_flush_loops".to_string()].into_iter().collect();
        apply_hir_presentation_with_globals(&mut func, &globals);
        let assigns: Vec<&str> = func
            .body
            .iter()
            .filter_map(|stmt| match stmt {
                HirStmt::Assign {
                    lhs: HirLValue::Var(name),
                    ..
                } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert!(assigns.contains(&"max_flush_loops"), "{assigns:?}");
        assert!(!assigns.contains(&"dead_local"), "{assigns:?}");
    }

    #[test]
    fn hir_presentation_drops_unused_home_local() {
        let mut func = HirFunction {
            name: "f".into(),
            params: vec![],
            locals: vec![
                NirBinding {
                    name: "home_0".into(),
                    ty: int_ty(64, false),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
                local("x"),
            ],
            return_type: int_ty(32, true),
            body: vec![HirStmt::Return(Some(HirExpr::Var("x".into())))],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        assert!(func.locals.iter().all(|b| b.name != "home_0"));
        assert!(func.locals.iter().any(|b| b.name == "x"));
    }

    /// Regression test for the bin_000.elf finding: a variable referenced
    /// only inside a do-while loop's condition (`while (r15 != rax)`, never
    /// elsewhere in the body) must keep its declaration. `collect_used_names_stmt`
    /// used to match `While`/`DoWhile` as `{ body, .. }`, silently discarding
    /// the `cond` field, so `drop_unused_presentation_locals` pruned the
    /// declaration while the renderer still printed the condition
    /// referencing it -- producing HIR that referenced an undeclared
    /// variable.
    #[test]
    fn hir_presentation_keeps_local_referenced_only_in_dowhile_condition() {
        let mut func = HirFunction {
            name: "f".into(),
            params: vec![],
            locals: vec![local("guard"), local("cursor")],
            return_type: int_ty(32, true),
            body: vec![HirStmt::DoWhile {
                body: vec![HirStmt::Assign {
                    lhs: HirLValue::Var("cursor".into()),
                    rhs: HirExpr::Binary {
                        op: fission_midend_core::ir::HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("cursor".into())),
                        rhs: Box::new(HirExpr::Const(1, int_ty(32, true))),
                        ty: int_ty(32, true),
                    },
                }],
                cond: HirExpr::Binary {
                    op: fission_midend_core::ir::HirBinaryOp::Ne,
                    lhs: Box::new(HirExpr::Var("guard".into())),
                    rhs: Box::new(HirExpr::Var("cursor".into())),
                    ty: int_ty(32, true),
                },
            }],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        assert!(
            func.locals.iter().any(|b| b.name == "guard"),
            "local used only in a do-while condition must keep its declaration"
        );
    }

    #[test]
    fn hir_presentation_folds_add_ints_param_home_and_temp() {
        // Mirrors gcc -O0 add_ints HIR/NIR shape:
        //   param_10 = param_1;
        //   param_18 = param_2;
        //   uVar6 = param_18;
        //   uVar6 = uVar6 + param_10;
        //   return uVar6;
        let mut func = HirFunction {
            name: "add_ints".into(),
            params: vec![param("param_1"), param("param_2")],
            locals: vec![local("param_10"), local("param_18"), local("uVar6")],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("param_10".into()),
                    rhs: HirExpr::Var("param_1".into()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("param_18".into()),
                    rhs: HirExpr::Var("param_2".into()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("uVar6".into()),
                    rhs: HirExpr::Var("param_18".into()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("uVar6".into()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("uVar6".into())),
                        rhs: Box::new(HirExpr::Var("param_10".into())),
                        ty: int_ty(32, true),
                    },
                },
                HirStmt::Return(Some(HirExpr::Var("uVar6".into()))),
            ],
            ..Default::default()
        };

        let options = MlilPreviewOptions::default();
        let layered = render_layered_pseudocode(&func, &options);

        assert!(
            layered.nir.contains("param_10") || layered.nir.contains("uVar6"),
            "NIR should keep mechanical aliases:\n{}",
            layered.nir
        );
        assert!(
            !layered.hir.contains("param_10"),
            "HIR should fold param home aliases:\n{}",
            layered.hir
        );
        assert!(
            !layered.hir.contains("param_18"),
            "HIR should fold param home aliases:\n{}",
            layered.hir
        );
        assert!(
            !layered.hir.contains("uVar6"),
            "HIR should fold return temp:\n{}",
            layered.hir
        );
        assert!(
            layered.hir.contains("param_1")
                && layered.hir.contains("param_2")
                && layered.hir.contains('+'),
            "HIR should return param_1 + param_2 style:\n{}",
            layered.hir
        );

        apply_hir_presentation(&mut func);
        assert!(
            func.locals.is_empty(),
            "no leftover locals: {:?}",
            func.locals
        );
        assert_eq!(func.body.len(), 1);
        match &func.body[0] {
            HirStmt::Return(Some(HirExpr::Binary {
                op: HirBinaryOp::Add,
                lhs,
                rhs,
                ..
            })) => {
                let names = match (lhs.as_ref(), rhs.as_ref()) {
                    (HirExpr::Var(a), HirExpr::Var(b)) => (a.as_str(), b.as_str()),
                    other => panic!("expected var+var, got {other:?}"),
                };
                assert!(
                    (names.0 == "param_1" && names.1 == "param_2")
                        || (names.0 == "param_2" && names.1 == "param_1"),
                    "unexpected add operands: {names:?}"
                );
            }
            other => panic!("expected return add, got {other:?}"),
        }
    }

    #[test]
    fn hir_presentation_keeps_mutated_param_home() {
        // param_10 = param_1; param_10 >>= 1;  — multi-def, do not alias-fold.
        let mut func = HirFunction {
            name: "count_bits_like".into(),
            params: vec![param("param_1")],
            locals: vec![local("param_10")],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("param_10".into()),
                    rhs: HirExpr::Var("param_1".into()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("param_10".into()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Shr,
                        lhs: Box::new(HirExpr::Var("param_10".into())),
                        rhs: Box::new(HirExpr::Const(1, int_ty(32, false))),
                        ty: int_ty(32, true),
                    },
                },
                HirStmt::Return(Some(HirExpr::Var("param_10".into()))),
            ],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        // Seed copy may fold into self-update: param_10 = param_1 >> 1; return param_10
        // or further collapse to return param_1 >> 1. Multi-def path must not
        // rewrite the mutated home as a pure alias of param_1 across the shift.
        let code = crate::midend::print_hir_function(&func);
        assert!(
            code.contains(">>") || code.contains("param_10"),
            "shift should remain: {code}"
        );
        // Must not claim `return param_1` alone.
        assert!(
            !matches!(
                func.body.as_slice(),
                [HirStmt::Return(Some(HirExpr::Var(n)))] if n == "param_1"
            ),
            "must not drop the shift: {code}"
        );
    }

    #[test]
    fn hir_presentation_keeps_saved_call_result_across_redef() {
        // rax = f(); rbx = rax; rax = g(); rax += rbx  must not become rax += rax.
        let mut func = HirFunction {
            name: "dual_call".into(),
            params: vec![param("param_1")],
            locals: vec![local("rax"), local("rbx")],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("rax".into()),
                    rhs: HirExpr::Call {
                        target: "f".into(),
                        args: vec![HirExpr::Var("param_1".into())],
                        ty: int_ty(32, true),
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("rbx".into()),
                    rhs: HirExpr::Var("rax".into()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("rax".into()),
                    rhs: HirExpr::Call {
                        target: "g".into(),
                        args: vec![HirExpr::Var("param_1".into())],
                        ty: int_ty(32, true),
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("rax".into()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("rax".into())),
                        rhs: Box::new(HirExpr::Var("rbx".into())),
                        ty: int_ty(32, true),
                    },
                },
                HirStmt::Return(Some(HirExpr::Var("rax".into()))),
            ],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        assert!(
            !code.contains("rax += rax") && !code.contains("rax = rax + rax"),
            "must not fold saved call result into reassigned surface:\n{code}"
        );
        assert!(
            code.contains("rbx") || code.contains("f("),
            "first call result must remain distinguishable:\n{code}"
        );
    }

    fn le(lhs: &str, rhs: &str) -> HirExpr {
        HirExpr::Binary {
            op: HirBinaryOp::Le,
            lhs: Box::new(HirExpr::Var(lhs.into())),
            rhs: Box::new(HirExpr::Var(rhs.into())),
            ty: NirType::Bool,
        }
    }

    #[test]
    fn hir_presentation_recovers_clamp_goto_diamond() {
        // O0 clamp shape: param homes + if-goto join return.
        let func = HirFunction {
            name: "clamp".into(),
            params: vec![param("param_1"), param("param_2"), param("param_3")],
            locals: vec![
                local("param_10"),
                local("param_18"),
                local("param_20"),
                local("rax"),
            ],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("param_10".into()),
                    rhs: HirExpr::Var("param_1".into()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("param_18".into()),
                    rhs: HirExpr::Var("param_2".into()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("param_20".into()),
                    rhs: HirExpr::Var("param_3".into()),
                },
                HirStmt::If {
                    cond: le("param_18", "param_10"),
                    then_body: vec![HirStmt::Goto("L1".into())],
                    else_body: vec![],
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("rax".into()),
                    rhs: HirExpr::Var("param_18".into()),
                },
                HirStmt::Goto("Lret".into()),
                HirStmt::Label("L1".into()),
                HirStmt::If {
                    cond: le("param_10", "param_20"),
                    then_body: vec![HirStmt::Goto("L2".into())],
                    else_body: vec![],
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("rax".into()),
                    rhs: HirExpr::Var("param_20".into()),
                },
                HirStmt::Goto("Lret".into()),
                HirStmt::Label("L2".into()),
                HirStmt::Assign {
                    lhs: HirLValue::Var("rax".into()),
                    rhs: HirExpr::Var("param_10".into()),
                },
                HirStmt::Label("Lret".into()),
                HirStmt::Return(Some(HirExpr::Var("rax".into()))),
            ],
            ..Default::default()
        };

        let options = MlilPreviewOptions::default();
        let layered = render_layered_pseudocode(&func, &options);
        assert!(
            layered.nir.contains("goto") || layered.nir.contains("param_10"),
            "NIR keeps mechanical control:\n{}",
            layered.nir
        );
        assert!(
            !layered.hir.contains("goto"),
            "HIR should eliminate gotos:\n{}",
            layered.hir
        );
        assert!(
            !layered.hir.contains("param_10")
                && !layered.hir.contains("param_18")
                && !layered.hir.contains("param_20"),
            "HIR should fold param homes:\n{}",
            layered.hir
        );
        assert!(
            layered.hir.contains("param_1")
                && layered.hir.contains("param_2")
                && layered.hir.contains("param_3"),
            "HIR should mention formals:\n{}",
            layered.hir
        );
        // Pure diamond may stay as if/else or fold further into nested select/ternary.
        assert!(
            layered.hir.contains("if") || layered.hir.contains('?'),
            "HIR should be structured if/else or pure select:\n{}",
            layered.hir
        );
        assert!(
            layered.hir.contains("return"),
            "HIR should return a value:\n{}",
            layered.hir
        );
    }

    #[test]
    fn hir_presentation_keeps_goto_target_after_unconditional_return() {
        // A forward `goto` jumps past an unconditional `return` into code
        // laid out later in the body (e.g. a rarely-taken branch a compiler
        // placed after the common-path return) -- `prune_unreachable_after_
        // total_return` must not treat that trailing labeled block as dead
        // just because it textually follows a `return`, or the goto is left
        // dangling and the labeled code silently disappears from HIR.
        let func = HirFunction {
            name: "f".into(),
            params: vec![param("param_1")],
            locals: vec![local("x"), local("y")],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::If {
                    cond: le("param_1", "param_1"),
                    then_body: vec![HirStmt::Goto("L1".into())],
                    else_body: vec![],
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("x".into()),
                    rhs: HirExpr::Const(1, int_ty(32, true)),
                },
                HirStmt::Return(Some(HirExpr::Var("x".into()))),
                HirStmt::Label("L1".into()),
                HirStmt::Assign {
                    lhs: HirLValue::Var("y".into()),
                    rhs: HirExpr::Const(2, int_ty(32, true)),
                },
                HirStmt::Return(Some(HirExpr::Var("y".into()))),
            ],
            ..Default::default()
        };

        let options = MlilPreviewOptions::default();
        let layered = render_layered_pseudocode(&func, &options);
        if layered.hir.contains("goto") {
            assert!(
                layered.hir.contains("L1:"),
                "HIR keeps a `goto L1` only if `L1:` is still defined somewhere:\n{}",
                layered.hir
            );
        }
        assert!(
            layered.hir.contains('2'),
            "HIR must not drop the labeled tail (y = 2) reachable only via goto:\n{}",
            layered.hir
        );
    }

    #[test]
    fn hir_presentation_keeps_nested_goto_target_after_unconditional_return() {
        let func = HirFunction {
            name: "f".into(),
            params: vec![param("param_1")],
            locals: vec![local("x")],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::If {
                    cond: le("param_1", "param_1"),
                    then_body: vec![HirStmt::Goto("L1".into())],
                    else_body: vec![],
                },
                HirStmt::Return(Some(HirExpr::Const(1, int_ty(32, true)))),
                HirStmt::Block(vec![
                    HirStmt::Label("L1".into()),
                    HirStmt::Assign {
                        lhs: HirLValue::Var("x".into()),
                        rhs: HirExpr::Const(2, int_ty(32, true)),
                    },
                    HirStmt::Return(Some(HirExpr::Var("x".into()))),
                ]),
            ],
            ..Default::default()
        };

        let options = MlilPreviewOptions::default();
        let layered = render_layered_pseudocode(&func, &options);
        if layered.hir.contains("goto") {
            assert!(layered.hir.contains("L1:"), "{0}", layered.hir);
        }
        assert!(layered.hir.contains('2'), "{0}", layered.hir);
    }

    #[test]
    fn hir_presentation_keeps_cross_scope_target_during_local_if_recovery() {
        let cond = || HirStmt::If {
            cond: le("param_1", "param_1"),
            then_body: vec![HirStmt::Goto("L1".into())],
            else_body: vec![],
        };
        let func = HirFunction {
            name: "f".into(),
            params: vec![param("param_1")],
            locals: vec![local("x")],
            return_type: int_ty(32, true),
            body: vec![
                cond(),
                HirStmt::Block(vec![
                    cond(),
                    HirStmt::Return(Some(HirExpr::Const(1, int_ty(32, true)))),
                    HirStmt::Label("L1".into()),
                    HirStmt::Assign {
                        lhs: HirLValue::Var("x".into()),
                        rhs: HirExpr::Const(2, int_ty(32, true)),
                    },
                    HirStmt::Return(Some(HirExpr::Var("x".into()))),
                ]),
            ],
            ..Default::default()
        };

        let options = MlilPreviewOptions::default();
        let layered = render_layered_pseudocode(&func, &options);
        if layered.hir.contains("goto") {
            assert!(layered.hir.contains("L1:"), "{0}", layered.hir);
        }
        assert!(layered.hir.contains('2'), "{0}", layered.hir);
    }

    #[test]
    fn hir_presentation_recovers_count_bits_while_loop() {
        // goto Lcond; Lbody: …; Lcond: if (x) goto Lbody;
        let func = HirFunction {
            name: "count_bits".into(),
            params: vec![NirBinding {
                name: "param_1".into(),
                ty: int_ty(32, false),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            locals: vec![
                NirBinding {
                    name: "param_10".into(),
                    ty: int_ty(32, false),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
                local("local_4"),
                local("uVar7"),
            ],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("param_10".into()),
                    rhs: HirExpr::Var("param_1".into()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("local_4".into()),
                    rhs: HirExpr::Const(0, int_ty(32, true)),
                },
                HirStmt::Goto("Lcond".into()),
                HirStmt::Label("Lbody".into()),
                HirStmt::Assign {
                    lhs: HirLValue::Var("uVar7".into()),
                    rhs: HirExpr::Var("param_10".into()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("uVar7".into()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::And,
                        lhs: Box::new(HirExpr::Var("uVar7".into())),
                        rhs: Box::new(HirExpr::Const(1, int_ty(32, true))),
                        ty: int_ty(32, true),
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("local_4".into()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("local_4".into())),
                        rhs: Box::new(HirExpr::Var("uVar7".into())),
                        ty: int_ty(32, true),
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("param_10".into()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Shr,
                        lhs: Box::new(HirExpr::Var("param_10".into())),
                        rhs: Box::new(HirExpr::Const(1, int_ty(32, false))),
                        ty: int_ty(32, false),
                    },
                },
                HirStmt::Label("Lcond".into()),
                HirStmt::If {
                    cond: HirExpr::Var("param_10".into()),
                    then_body: vec![HirStmt::Goto("Lbody".into())],
                    else_body: vec![],
                },
                HirStmt::Return(Some(HirExpr::Var("local_4".into()))),
            ],
            ..Default::default()
        };

        let layered = render_layered_pseudocode(&func, &MlilPreviewOptions::default());
        assert!(
            layered.nir.contains("goto") || layered.nir.contains("Lcond"),
            "NIR keeps goto loop:\n{}",
            layered.nir
        );
        assert!(
            !layered.hir.contains("goto"),
            "HIR should drop gotos:\n{}",
            layered.hir
        );
        assert!(
            layered.hir.contains("while"),
            "HIR should recover while:\n{}",
            layered.hir
        );
        assert!(
            !layered.hir.contains("uVar7"),
            "HIR should fold bit temp:\n{}",
            layered.hir
        );
        assert!(
            !layered.hir.contains("while (1)") && !layered.hir.contains("while(1)"),
            "HIR should not leave while(1):\n{}",
            layered.hir
        );
    }

    #[test]
    fn hir_presentation_folds_while_true_break_guard() {
        // Real count_bits shape after structuring: while(1){ if(!x) break; body }
        let mut func = HirFunction {
            name: "count_bits".into(),
            params: vec![NirBinding {
                name: "param_1".into(),
                ty: int_ty(32, false),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            locals: vec![
                NirBinding {
                    name: "param_10".into(),
                    ty: int_ty(32, false),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
                local("local_4"),
            ],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("param_10".into()),
                    rhs: HirExpr::Var("param_1".into()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("local_4".into()),
                    rhs: HirExpr::Const(0, int_ty(32, true)),
                },
                HirStmt::While {
                    cond: HirExpr::Const(1, int_ty(32, false)),
                    body: vec![
                        HirStmt::If {
                            cond: HirExpr::Unary {
                                op: HirUnaryOp::Not,
                                expr: Box::new(HirExpr::Var("param_10".into())),
                                ty: NirType::Bool,
                            },
                            then_body: vec![HirStmt::Break],
                            else_body: vec![],
                        },
                        HirStmt::Assign {
                            lhs: HirLValue::Var("local_4".into()),
                            rhs: HirExpr::Binary {
                                op: HirBinaryOp::Add,
                                lhs: Box::new(HirExpr::Var("local_4".into())),
                                rhs: Box::new(HirExpr::Binary {
                                    op: HirBinaryOp::And,
                                    lhs: Box::new(HirExpr::Var("param_10".into())),
                                    rhs: Box::new(HirExpr::Const(1, int_ty(32, true))),
                                    ty: int_ty(32, true),
                                }),
                                ty: int_ty(32, true),
                            },
                        },
                        HirStmt::Assign {
                            lhs: HirLValue::Var("param_10".into()),
                            rhs: HirExpr::Binary {
                                op: HirBinaryOp::Shr,
                                lhs: Box::new(HirExpr::Var("param_10".into())),
                                rhs: Box::new(HirExpr::Const(1, int_ty(32, false))),
                                ty: int_ty(32, false),
                            },
                        },
                    ],
                },
                HirStmt::Return(Some(HirExpr::Var("local_4".into()))),
            ],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        assert!(
            !code.contains("while (1)") && !code.contains("break"),
            "should fold while(1)/break:\n{code}"
        );
        assert!(
            code.contains("while") && code.contains("param_10"),
            "should keep while(param_10)-style loop:\n{code}"
        );
    }

    #[test]
    fn hir_presentation_folds_nullcheck_select_return_join() {
        // apply_binop: rax = !p ? 0 : call(p); return !(p==0) ? rax : 0;
        let mut func = HirFunction {
            name: "apply_binop".into(),
            params: vec![
                NirBinding {
                    name: "param_1".into(),
                    ty: int_ty(64, false),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::ParamIndex(0)),
                    initializer: None,
                },
                NirBinding {
                    name: "param_2".into(),
                    ty: int_ty(32, false),
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::ParamIndex(1)),
                    initializer: None,
                },
            ],
            locals: vec![NirBinding {
                name: "rax".into(),
                ty: int_ty(64, false),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::Temp),
                initializer: None,
            }],
            return_type: int_ty(64, false),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("rax".into()),
                    rhs: HirExpr::Select {
                        cond: Box::new(HirExpr::Unary {
                            op: HirUnaryOp::Not,
                            expr: Box::new(HirExpr::Var("param_1".into())),
                            ty: NirType::Bool,
                        }),
                        then_expr: Box::new(HirExpr::Const(0, int_ty(64, false))),
                        else_expr: Box::new(HirExpr::Call {
                            target: "fn".into(),
                            args: vec![HirExpr::Var("param_2".into())],
                            ty: int_ty(64, false),
                        }),
                        ty: int_ty(64, false),
                    },
                },
                HirStmt::Return(Some(HirExpr::Select {
                    cond: Box::new(HirExpr::Unary {
                        op: HirUnaryOp::Not,
                        expr: Box::new(HirExpr::Binary {
                            op: HirBinaryOp::Eq,
                            lhs: Box::new(HirExpr::Var("param_1".into())),
                            rhs: Box::new(HirExpr::Const(0, int_ty(64, false))),
                            ty: NirType::Bool,
                        }),
                        ty: NirType::Bool,
                    }),
                    then_expr: Box::new(HirExpr::Var("rax".into())),
                    else_expr: Box::new(HirExpr::Const(0, int_ty(64, false))),
                    ty: int_ty(64, false),
                })),
            ],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        assert!(!code.contains("rax"), "rax join temp should fold:\n{code}");
        assert!(
            code.matches("param_1").count() <= 2,
            "should not double null-check param_1 excessively:\n{code}"
        );
        assert!(
            code.contains("return") && code.contains("fn"),
            "should keep call in return:\n{code}"
        );
    }

    #[test]
    fn hir_presentation_drops_dead_flag_and_popcount_noise() {
        let mut func = HirFunction {
            name: "flag_noise".into(),
            params: vec![param("param_1")],
            locals: vec![local("zf"), local("xVar2"), local("uVar1"), local("result")],
            return_type: int_ty(32, true),
            body: vec![
                // Dead parity chain (never read).
                HirStmt::Assign {
                    lhs: HirLValue::Var("uVar1".into()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::And,
                        lhs: Box::new(HirExpr::Var("param_1".into())),
                        rhs: Box::new(HirExpr::Const(255, int_ty(32, false))),
                        ty: int_ty(32, false),
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("xVar2".into()),
                    rhs: HirExpr::Call {
                        target: "__popcount".into(),
                        args: vec![HirExpr::Var("uVar1".into())],
                        ty: int_ty(8, false),
                    },
                },
                // Live flag used once in return — should inline.
                HirStmt::Assign {
                    lhs: HirLValue::Var("zf".into()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Eq,
                        lhs: Box::new(HirExpr::Var("param_1".into())),
                        rhs: Box::new(HirExpr::Const(0, int_ty(32, true))),
                        ty: NirType::Bool,
                    },
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("result".into()),
                    rhs: HirExpr::Select {
                        cond: Box::new(HirExpr::Unary {
                            op: HirUnaryOp::Not,
                            expr: Box::new(HirExpr::Var("zf".into())),
                            ty: NirType::Bool,
                        }),
                        then_expr: Box::new(HirExpr::Const(1, int_ty(32, true))),
                        else_expr: Box::new(HirExpr::Const(0, int_ty(32, true))),
                        ty: int_ty(32, true),
                    },
                },
                HirStmt::Return(Some(HirExpr::Var("result".into()))),
            ],
            ..Default::default()
        };

        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        assert!(
            !code.contains("__popcount") && !code.contains("uVar1") && !code.contains("xVar2"),
            "dead popcount chain should drop:\n{code}"
        );
        assert!(
            !code.contains("zf"),
            "single-use flag should inline away:\n{code}"
        );
        assert!(
            code.contains("param_1") && code.contains("return"),
            "result should remain meaningful:\n{code}"
        );
    }

    #[test]
    fn hir_presentation_peels_identity_casts() {
        let mut func = HirFunction {
            name: "casts".into(),
            params: vec![param("param_1")],
            locals: vec![],
            return_type: int_ty(32, true),
            body: vec![HirStmt::Return(Some(HirExpr::Cast {
                ty: int_ty(32, true),
                expr: Box::new(HirExpr::Cast {
                    ty: int_ty(32, true),
                    expr: Box::new(HirExpr::Var("param_1".into())),
                }),
            }))],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        match &func.body[0] {
            HirStmt::Return(Some(HirExpr::Var(n))) if n == "param_1" => {}
            other => panic!("expected return param_1 after cast peel, got {other:?}"),
        }
    }

    #[test]
    fn hir_presentation_recovers_signum_goto_diamond() {
        let func = HirFunction {
            name: "signum".into(),
            params: vec![param("param_1")],
            locals: vec![local("param_10"), local("iVar4"), local("xVar9")],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("param_10".into()),
                    rhs: HirExpr::Var("param_1".into()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("iVar4".into()),
                    rhs: HirExpr::Var("param_10".into()),
                },
                HirStmt::If {
                    cond: HirExpr::Binary {
                        op: HirBinaryOp::LogicalOr,
                        lhs: Box::new(HirExpr::Binary {
                            op: HirBinaryOp::Eq,
                            lhs: Box::new(HirExpr::Var("iVar4".into())),
                            rhs: Box::new(HirExpr::Const(0, int_ty(32, true))),
                            ty: NirType::Bool,
                        }),
                        rhs: Box::new(HirExpr::Binary {
                            op: HirBinaryOp::SLt,
                            lhs: Box::new(HirExpr::Var("iVar4".into())),
                            rhs: Box::new(HirExpr::Const(0, int_ty(32, true))),
                            ty: NirType::Bool,
                        }),
                        ty: NirType::Bool,
                    },
                    then_body: vec![HirStmt::Goto("La".into())],
                    else_body: vec![],
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("xVar9".into()),
                    rhs: HirExpr::Const(1, int_ty(32, true)),
                },
                HirStmt::Goto("Lret".into()),
                HirStmt::Label("La".into()),
                HirStmt::If {
                    cond: HirExpr::Binary {
                        op: HirBinaryOp::SLe,
                        lhs: Box::new(HirExpr::Const(0, int_ty(32, true))),
                        rhs: Box::new(HirExpr::Var("param_10".into())),
                        ty: NirType::Bool,
                    },
                    then_body: vec![HirStmt::Goto("Lb".into())],
                    else_body: vec![],
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("xVar9".into()),
                    rhs: HirExpr::Const(-1, int_ty(32, true)),
                },
                HirStmt::Goto("Lret".into()),
                HirStmt::Label("Lb".into()),
                HirStmt::Assign {
                    lhs: HirLValue::Var("xVar9".into()),
                    rhs: HirExpr::Const(0, int_ty(32, true)),
                },
                HirStmt::Label("Lret".into()),
                HirStmt::Return(Some(HirExpr::Var("xVar9".into()))),
            ],
            ..Default::default()
        };

        let layered = render_layered_pseudocode(&func, &MlilPreviewOptions::default());
        assert!(
            !layered.hir.contains("goto"),
            "HIR should eliminate gotos:\n{}",
            layered.hir
        );
        assert!(
            !layered.hir.contains("param_10") && !layered.hir.contains("iVar4"),
            "HIR should fold aliases/temps:\n{}",
            layered.hir
        );
        // Pure diamond may stay as if/else or fold further into nested select/ternary.
        assert!(
            layered.hir.contains("param_1")
                && (layered.hir.contains("if") || layered.hir.contains('?')),
            "HIR should be structured if/else or pure select on param_1:\n{}",
            layered.hir
        );
        assert!(
            layered.hir.contains("return"),
            "HIR should return a value:\n{}",
            layered.hir
        );
    }

    fn count_calls_in_stmts(stmts: &[HirStmt]) -> usize {
        stmts.iter().map(count_calls_in_stmt).sum()
    }

    fn count_calls_in_stmt(stmt: &HirStmt) -> usize {
        match stmt {
            HirStmt::Assign { rhs, .. } => count_calls_in_expr(rhs),
            HirStmt::Expr(e) | HirStmt::Return(Some(e)) => count_calls_in_expr(e),
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. } => count_calls_in_stmts(b),
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                count_calls_in_expr(cond)
                    + count_calls_in_stmts(then_body)
                    + count_calls_in_stmts(else_body)
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                init.as_ref().map_or(0, |s| count_calls_in_stmt(s))
                    + cond.as_ref().map_or(0, |e| count_calls_in_expr(e))
                    + update.as_ref().map_or(0, |s| count_calls_in_stmt(s))
                    + count_calls_in_stmts(body)
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                count_calls_in_expr(expr)
                    + cases
                        .iter()
                        .map(|c| count_calls_in_stmts(&c.body))
                        .sum::<usize>()
                    + count_calls_in_stmts(default)
            }
            _ => 0,
        }
    }

    fn count_calls_in_expr(expr: &HirExpr) -> usize {
        match expr {
            HirExpr::Call { args, .. } => 1 + args.iter().map(count_calls_in_expr).sum::<usize>(),
            HirExpr::Unary { expr, .. } | HirExpr::Cast { expr, .. } => count_calls_in_expr(expr),
            HirExpr::Binary { lhs, rhs, .. } => count_calls_in_expr(lhs) + count_calls_in_expr(rhs),
            HirExpr::Select {
                cond,
                then_expr,
                else_expr,
                ..
            } => {
                count_calls_in_expr(cond)
                    + count_calls_in_expr(then_expr)
                    + count_calls_in_expr(else_expr)
            }
            HirExpr::Load { ptr, .. }
            | HirExpr::PtrOffset { base: ptr, .. }
            | HirExpr::FieldAccess { base: ptr, .. }
            | HirExpr::AggregateCopy { src: ptr, .. } => count_calls_in_expr(ptr),
            HirExpr::Index { base, index, .. } => {
                count_calls_in_expr(base) + count_calls_in_expr(index)
            }
            _ => 0,
        }
    }

    /// ADR 0011: multi-use call result must not be inlined into multiple call sites.
    #[test]
    fn hir_presentation_does_not_duplicate_multi_use_call() {
        let mut func = HirFunction {
            name: "multi_use_call".into(),
            params: vec![param("param_1")],
            locals: vec![local("x"), local("y")],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("x".into()),
                    rhs: HirExpr::Call {
                        target: "side_effect".into(),
                        args: vec![HirExpr::Var("param_1".into())],
                        ty: int_ty(32, true),
                    },
                },
                // Second use of x — must keep materialization (one call only).
                HirStmt::Assign {
                    lhs: HirLValue::Var("y".into()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("x".into())),
                        rhs: Box::new(HirExpr::Const(1, int_ty(32, true))),
                        ty: int_ty(32, true),
                    },
                },
                HirStmt::Return(Some(HirExpr::Binary {
                    op: HirBinaryOp::Add,
                    lhs: Box::new(HirExpr::Var("x".into())),
                    rhs: Box::new(HirExpr::Var("y".into())),
                    ty: int_ty(32, true),
                })),
            ],
            ..Default::default()
        };
        let before = count_calls_in_stmts(&func.body);
        assert_eq!(before, 1, "fixture must start with one call");
        apply_hir_presentation(&mut func);
        let after = count_calls_in_stmts(&func.body);
        assert_eq!(
            after, 1,
            "presentation must not re-execute call; body={:?}",
            func.body
        );
        let code = crate::midend::print_hir_function(&func);
        assert_eq!(
            code.matches("side_effect").count(),
            1,
            "printed HIR must mention call once:\n{code}"
        );
    }

    /// ADR 0011: single-eval collapse of call into return is OK (still one call).
    #[test]
    fn hir_presentation_collapses_single_use_call_return_without_duplicating() {
        let mut func = HirFunction {
            name: "once_call".into(),
            params: vec![param("param_1")],
            locals: vec![local("x")],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("x".into()),
                    rhs: HirExpr::Call {
                        target: "once".into(),
                        args: vec![HirExpr::Var("param_1".into())],
                        ty: int_ty(32, true),
                    },
                },
                HirStmt::Return(Some(HirExpr::Var("x".into()))),
            ],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        assert_eq!(count_calls_in_stmts(&func.body), 1);
        let code = crate::midend::print_hir_function(&func);
        assert!(
            code.contains("return") && code.matches("once(").count() == 1,
            "single-use call may fold into return once:\n{code}"
        );
    }

    #[test]
    fn hir_presentation_folds_if_else_pure_assign_to_select() {
        let mut func = HirFunction {
            name: "clamp_like".into(),
            params: vec![param("param_1")],
            locals: vec![local("x")],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::If {
                    cond: le("param_1", "0"),
                    then_body: vec![HirStmt::Assign {
                        lhs: HirLValue::Var("x".into()),
                        rhs: HirExpr::Const(0, int_ty(32, true)),
                    }],
                    else_body: vec![HirStmt::Assign {
                        lhs: HirLValue::Var("x".into()),
                        rhs: HirExpr::Var("param_1".into()),
                    }],
                },
                HirStmt::Return(Some(HirExpr::Var("x".into()))),
            ],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        // Prefer select form (and optional return collapse): no residual if/else.
        assert!(
            !code.contains("if (") && (code.contains('?') || code.contains("return")),
            "expected pure if/else assign fold into select:\n{code}"
        );
        assert!(
            code.contains("return") && (code.contains("param_1") || code.contains('0')),
            "must keep both branch values:\n{code}"
        );
    }

    #[test]
    fn hir_presentation_folds_if_else_pure_returns_to_select() {
        let mut func = HirFunction {
            name: "minmax_ret".into(),
            params: vec![param("param_1")],
            locals: vec![],
            return_type: int_ty(32, true),
            body: vec![HirStmt::If {
                cond: le("param_1", "0"),
                then_body: vec![HirStmt::Return(Some(HirExpr::Const(0, int_ty(32, true))))],
                else_body: vec![HirStmt::Return(Some(HirExpr::Var("param_1".into())))],
            }],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        assert!(
            !code.contains("if (") && code.contains('?') && code.contains("return"),
            "expected if/else pure returns → ternary return:\n{code}"
        );
    }

    #[test]
    fn hir_presentation_does_not_fold_effectful_if_else_assign() {
        let mut func = HirFunction {
            name: "side_effect_branch".into(),
            params: vec![param("param_1")],
            locals: vec![local("x")],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::If {
                    cond: le("param_1", "0"),
                    then_body: vec![HirStmt::Assign {
                        lhs: HirLValue::Var("x".into()),
                        rhs: HirExpr::Call {
                            target: "side".into(),
                            args: vec![],
                            ty: int_ty(32, true),
                        },
                    }],
                    else_body: vec![HirStmt::Assign {
                        lhs: HirLValue::Var("x".into()),
                        rhs: HirExpr::Var("param_1".into()),
                    }],
                },
                HirStmt::Return(Some(HirExpr::Var("x".into()))),
            ],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        // Must keep a single evaluation of `side()` — do not select-fold call arms.
        assert_eq!(count_calls_in_stmts(&func.body), 1);
        let code = crate::midend::print_hir_function(&func);
        assert!(
            code.contains("if (") || code.contains("side("),
            "effectful branch must not become multi-eval select:\n{code}"
        );
    }

    #[test]
    fn hir_presentation_strips_empty_else() {
        // No prior seed for `y` so seed+overwrite does not fold this into select.
        let mut func = HirFunction {
            name: "maybe_set".into(),
            params: vec![param("param_1")],
            locals: vec![local("y")],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::If {
                    cond: le("param_1", "0"),
                    then_body: vec![HirStmt::Assign {
                        lhs: HirLValue::Var("y".into()),
                        rhs: HirExpr::Const(0, int_ty(32, true)),
                    }],
                    else_body: vec![HirStmt::Block(vec![])],
                },
                HirStmt::Return(Some(HirExpr::Var("y".into()))),
            ],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        assert!(
            !code.contains("else"),
            "empty else arm should be stripped:\n{code}"
        );
        assert!(
            code.contains("if ("),
            "residual if should remain after empty-else strip:\n{code}"
        );
    }

    #[test]
    fn hir_presentation_folds_if_return_fallthrough_return() {
        let mut func = HirFunction {
            name: "early_ret".into(),
            params: vec![param("param_1")],
            locals: vec![],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::If {
                    cond: le("param_1", "0"),
                    then_body: vec![HirStmt::Return(Some(HirExpr::Const(0, int_ty(32, true))))],
                    else_body: vec![],
                },
                HirStmt::Return(Some(HirExpr::Var("param_1".into()))),
            ],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        assert!(
            !code.contains("if (") && code.contains('?') && code.contains("return"),
            "expected if-return + fallthrough return → ternary:\n{code}"
        );
    }

    /// Regression: pure assigns inside if/else used after the if must not be
    /// deleted by nested dead-elim (whole-function use counts required).
    #[test]
    fn hir_presentation_keeps_branch_defs_used_after_if() {
        let mut func = HirFunction {
            name: "branch_join".into(),
            params: vec![param("param_1")],
            locals: vec![local("x")],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::If {
                    cond: HirExpr::Binary {
                        op: HirBinaryOp::SGt,
                        lhs: Box::new(HirExpr::Var("param_1".into())),
                        rhs: Box::new(HirExpr::Const(0, int_ty(32, true))),
                        ty: NirType::Bool,
                    },
                    then_body: vec![HirStmt::Assign {
                        lhs: HirLValue::Var("x".into()),
                        rhs: HirExpr::Const(1, int_ty(32, true)),
                    }],
                    else_body: vec![HirStmt::Assign {
                        lhs: HirLValue::Var("x".into()),
                        rhs: HirExpr::Const(-1, int_ty(32, true)),
                    }],
                },
                HirStmt::Return(Some(HirExpr::Var("x".into()))),
            ],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        // Must return a concrete value, not an undefined local.
        assert!(
            code.contains("return")
                && (code.contains('1') || code.contains('-') || code.contains('?')),
            "branch join value must survive presentation:\n{code}"
        );
        assert!(
            !code.contains("return x") && !code.contains("return xVar"),
            "must not return an undefined join temp:\n{code}"
        );
    }

    /// signum-like: positive branch + else select must keep a defined return.
    #[test]
    fn hir_presentation_signum_like_keeps_defined_return() {
        let mut func = HirFunction {
            name: "signum_like".into(),
            params: vec![param("param_1")],
            locals: vec![local("xVar9"), local("sf")],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::If {
                    // NIR-ish: `0 < param_1` (const-left; canonicalize may commute).
                    cond: HirExpr::Binary {
                        op: HirBinaryOp::SLt,
                        lhs: Box::new(HirExpr::Const(0, int_ty(32, true))),
                        rhs: Box::new(HirExpr::Var("param_1".into())),
                        ty: NirType::Bool,
                    },
                    then_body: vec![HirStmt::Assign {
                        lhs: HirLValue::Var("xVar9".into()),
                        rhs: HirExpr::Const(1, int_ty(32, true)),
                    }],
                    else_body: vec![
                        HirStmt::Assign {
                            lhs: HirLValue::Var("sf".into()),
                            rhs: HirExpr::Binary {
                                op: HirBinaryOp::SLt,
                                lhs: Box::new(HirExpr::Var("param_1".into())),
                                rhs: Box::new(HirExpr::Const(0, int_ty(32, true))),
                                ty: NirType::Bool,
                            },
                        },
                        HirStmt::Assign {
                            lhs: HirLValue::Var("xVar9".into()),
                            rhs: HirExpr::Select {
                                cond: Box::new(HirExpr::Unary {
                                    op: HirUnaryOp::Not,
                                    expr: Box::new(HirExpr::Var("sf".into())),
                                    ty: NirType::Bool,
                                }),
                                then_expr: Box::new(HirExpr::Const(0, int_ty(32, true))),
                                else_expr: Box::new(HirExpr::Const(-1, int_ty(32, true))),
                                ty: int_ty(32, true),
                            },
                        },
                    ],
                },
                HirStmt::Return(Some(HirExpr::Var("xVar9".into()))),
            ],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        assert!(
            code.contains("return")
                && (code.contains('1') || code.contains('0') || code.contains('-')),
            "signum-like must keep defined return values:\n{code}"
        );
        assert!(
            !code.contains("return xVar9") && !code.contains("return xVar"),
            "must not return undefined xVar after presentation:\n{code}"
        );
    }

    #[test]
    fn hir_presentation_inverts_empty_then_else() {
        let mut func = HirFunction {
            name: "empty_then".into(),
            params: vec![param("param_1")],
            locals: vec![local("x")],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("x".into()),
                    rhs: HirExpr::Const(1, int_ty(32, true)),
                },
                HirStmt::If {
                    cond: le("param_1", "0"),
                    then_body: vec![],
                    else_body: vec![HirStmt::Assign {
                        lhs: HirLValue::Var("x".into()),
                        rhs: HirExpr::Const(0, int_ty(32, true)),
                    }],
                },
                HirStmt::Return(Some(HirExpr::Var("x".into()))),
            ],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        let else_count = code.matches("else").count();
        assert_eq!(
            else_count, 0,
            "empty then should invert to single if:\n{code}"
        );
        assert!(
            code.contains("if (") || code.contains('?'),
            "must retain a branch form:\n{code}"
        );
    }

    #[test]
    fn hir_presentation_canonicalizes_const_left_comparison() {
        let mut func = HirFunction {
            name: "cmp_left".into(),
            params: vec![param("param_1")],
            locals: vec![],
            return_type: int_ty(32, true),
            body: vec![HirStmt::If {
                cond: HirExpr::Binary {
                    op: HirBinaryOp::SLe,
                    lhs: Box::new(HirExpr::Const(0, int_ty(32, true))),
                    rhs: Box::new(HirExpr::Var("param_1".into())),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Return(Some(HirExpr::Const(1, int_ty(32, true))))],
                else_body: vec![HirStmt::Return(Some(HirExpr::Const(0, int_ty(32, true))))],
            }],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        // After const-left commute: `param_1 >= 0` (not `0 <= param_1`).
        let const_left = code.contains("0 <=") || code.contains("0 < ") || code.contains("(0 <=");
        assert!(
            !const_left,
            "const-left comparison should commute to var-left:\n{code}"
        );
        assert!(
            code.contains("param_1") && (code.contains(">=") || code.contains('?')),
            "expected var-left comparison or folded select:\n{code}"
        );
    }

    #[test]
    fn hir_presentation_peels_not_eq_zero() {
        let mut func = HirFunction {
            name: "not_eq0".into(),
            params: vec![param("param_1")],
            locals: vec![],
            return_type: int_ty(32, true),
            body: vec![HirStmt::If {
                cond: HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(HirExpr::Binary {
                        op: HirBinaryOp::Eq,
                        lhs: Box::new(HirExpr::Var("param_1".into())),
                        rhs: Box::new(HirExpr::Const(0, int_ty(32, true))),
                        ty: NirType::Bool,
                    }),
                    ty: NirType::Bool,
                },
                then_body: vec![HirStmt::Return(Some(HirExpr::Const(1, int_ty(32, true))))],
                else_body: vec![HirStmt::Return(Some(HirExpr::Const(0, int_ty(32, true))))],
            }],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        assert!(
            !code.contains("!(") && !code.contains("== 0"),
            "!(x == 0) should peel toward != form:\n{code}"
        );
    }

    #[test]
    fn condition_rewrite_pushes_demorgan_negation_into_conjunction() {
        // !(a == 0 && b == 0) -> !(a == 0) || !(b == 0) -> a != 0 || b != 0,
        // exercising De Morgan's push together with the existing !(x==0)
        // peel in the same fixed-point pass.
        let mut expr = HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr: Box::new(HirExpr::Binary {
                op: HirBinaryOp::LogicalAnd,
                lhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Eq,
                    lhs: Box::new(HirExpr::Var("a".into())),
                    rhs: Box::new(HirExpr::Const(0, int_ty(32, true))),
                    ty: NirType::Bool,
                }),
                rhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Eq,
                    lhs: Box::new(HirExpr::Var("b".into())),
                    rhs: Box::new(HirExpr::Const(0, int_ty(32, true))),
                    ty: NirType::Bool,
                }),
                ty: NirType::Bool,
            }),
            ty: NirType::Bool,
        };
        while canonicalize_conditions_in_expr(&mut expr) {}
        assert_eq!(
            expr,
            HirExpr::Binary {
                op: HirBinaryOp::LogicalOr,
                lhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Ne,
                    lhs: Box::new(HirExpr::Var("a".into())),
                    rhs: Box::new(HirExpr::Const(0, int_ty(32, true))),
                    ty: NirType::Bool,
                }),
                rhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Ne,
                    lhs: Box::new(HirExpr::Var("b".into())),
                    rhs: Box::new(HirExpr::Const(0, int_ty(32, true))),
                    ty: NirType::Bool,
                }),
                ty: NirType::Bool,
            }
        );
    }

    #[test]
    fn condition_rewrite_turns_bitwise_or_zero_check_into_logical_form() {
        // (a | b) == 0 -> (a == 0) && (b == 0); a flags-mask check reads as
        // boolean logic instead of a bitwise compare.
        let mut expr = HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs: Box::new(HirExpr::Binary {
                op: HirBinaryOp::Or,
                lhs: Box::new(HirExpr::Var("a".into())),
                rhs: Box::new(HirExpr::Var("b".into())),
                ty: int_ty(32, false),
            }),
            rhs: Box::new(HirExpr::Const(0, int_ty(32, false))),
            ty: NirType::Bool,
        };
        assert!(canonicalize_conditions_in_expr(&mut expr));
        assert_eq!(
            expr,
            HirExpr::Binary {
                op: HirBinaryOp::LogicalAnd,
                lhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Eq,
                    lhs: Box::new(HirExpr::Var("a".into())),
                    rhs: Box::new(HirExpr::Const(0, int_ty(32, false))),
                    ty: NirType::Bool,
                }),
                rhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Eq,
                    lhs: Box::new(HirExpr::Var("b".into())),
                    rhs: Box::new(HirExpr::Const(0, int_ty(32, false))),
                    ty: NirType::Bool,
                }),
                ty: NirType::Bool,
            }
        );
    }

    #[test]
    fn condition_rewrite_removes_redundant_ite_comparison() {
        // (cond ? 1 : 0) == 1 -> cond; (cond ? 1 : 0) == 0 -> !cond.
        let select = |cond: &str| HirExpr::Select {
            cond: Box::new(HirExpr::Var(cond.into())),
            then_expr: Box::new(HirExpr::Const(1, int_ty(32, true))),
            else_expr: Box::new(HirExpr::Const(0, int_ty(32, true))),
            ty: int_ty(32, true),
        };

        let mut eq_true = HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs: Box::new(select("cond")),
            rhs: Box::new(HirExpr::Const(1, int_ty(32, true))),
            ty: NirType::Bool,
        };
        assert!(canonicalize_conditions_in_expr(&mut eq_true));
        assert_eq!(eq_true, HirExpr::Var("cond".into()));

        let mut eq_false = HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs: Box::new(select("cond")),
            rhs: Box::new(HirExpr::Const(0, int_ty(32, true))),
            ty: NirType::Bool,
        };
        assert!(canonicalize_conditions_in_expr(&mut eq_false));
        assert_eq!(
            eq_false,
            HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: Box::new(HirExpr::Var("cond".into())),
                ty: NirType::Bool,
            }
        );
    }

    #[test]
    fn hir_presentation_folds_seed_if_overwrite_assign() {
        let mut func = HirFunction {
            name: "clamp0".into(),
            params: vec![param("param_1")],
            locals: vec![local("x")],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("x".into()),
                    rhs: HirExpr::Var("param_1".into()),
                },
                HirStmt::If {
                    cond: le("x", "0"),
                    then_body: vec![HirStmt::Assign {
                        lhs: HirLValue::Var("x".into()),
                        rhs: HirExpr::Const(0, int_ty(32, true)),
                    }],
                    else_body: vec![],
                },
                HirStmt::Return(Some(HirExpr::Var("x".into()))),
            ],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        assert!(
            !code.contains("if (") && (code.contains('?') || code.contains("return")),
            "expected seed + if overwrite → select:\n{code}"
        );
        assert!(
            !code.contains("goto") && code.contains("param_1"),
            "must keep formal and drop control noise:\n{code}"
        );
    }

    fn eq_const(var: &str, value: i64) -> HirExpr {
        HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs: Box::new(HirExpr::Var(var.into())),
            rhs: Box::new(HirExpr::Const(value, int_ty(32, true))),
            ty: NirType::Bool,
        }
    }

    #[test]
    fn hir_presentation_recovers_switch_from_lowered_if_chain() {
        let mut func = HirFunction {
            name: "lowered_switch".into(),
            params: vec![param("param_1")],
            locals: vec![],
            return_type: int_ty(32, true),
            body: vec![HirStmt::If {
                cond: eq_const("param_1", 1),
                then_body: vec![HirStmt::Return(Some(HirExpr::Const(10, int_ty(32, true))))],
                else_body: vec![HirStmt::If {
                    cond: eq_const("param_1", 2),
                    then_body: vec![HirStmt::Return(Some(HirExpr::Const(20, int_ty(32, true))))],
                    else_body: vec![HirStmt::If {
                        cond: eq_const("param_1", 3),
                        then_body: vec![HirStmt::Return(Some(HirExpr::Const(
                            30,
                            int_ty(32, true),
                        )))],
                        else_body: vec![HirStmt::Return(Some(HirExpr::Const(
                            -1,
                            int_ty(32, true),
                        )))],
                    }],
                }],
            }],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        assert!(
            code.contains("switch (param_1)")
                && code.contains("case 1:")
                && code.contains("case 2:")
                && code.contains("case 3:")
                && code.contains("default:"),
            "three-arm lowered if-chain should recover to a switch:\n{code}"
        );
        assert!(
            !code.contains("if ("),
            "if-chain should be fully consumed by the switch:\n{code}"
        );
    }

    #[test]
    fn hir_presentation_keeps_two_arm_if_chain_as_if_else() {
        // Below MIN_LOWERED_SWITCH_CASES: two arms read fine as if/else,
        // not worth the switch(){} ceremony.
        let mut func = HirFunction {
            name: "two_arm".into(),
            params: vec![param("param_1")],
            locals: vec![],
            return_type: int_ty(32, true),
            body: vec![HirStmt::If {
                cond: eq_const("param_1", 1),
                then_body: vec![HirStmt::Return(Some(HirExpr::Const(10, int_ty(32, true))))],
                else_body: vec![HirStmt::If {
                    cond: eq_const("param_1", 2),
                    then_body: vec![HirStmt::Return(Some(HirExpr::Const(20, int_ty(32, true))))],
                    else_body: vec![HirStmt::Return(Some(HirExpr::Const(-1, int_ty(32, true))))],
                }],
            }],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        assert!(
            !code.contains("switch ("),
            "two-arm if-chain must not become a switch:\n{code}"
        );
    }

    #[test]
    fn hir_presentation_declines_switch_recovery_for_impure_load_expr() {
        // `*param_1 == c` repeated per arm would need to re-read memory up
        // to N times; a switch would collapse that to exactly one read.
        // Not safe to assume equivalent -- must stay an if-chain.
        let load = |ptr: &str| HirExpr::Load {
            ptr: Box::new(HirExpr::Var(ptr.into())),
            ty: int_ty(32, true),
        };
        let cond = |ptr: &str, value: i64| HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs: Box::new(load(ptr)),
            rhs: Box::new(HirExpr::Const(value, int_ty(32, true))),
            ty: NirType::Bool,
        };
        let mut func = HirFunction {
            name: "impure_switch".into(),
            params: vec![param("param_1")],
            locals: vec![],
            return_type: int_ty(32, true),
            body: vec![HirStmt::If {
                cond: cond("param_1", 1),
                then_body: vec![HirStmt::Return(Some(HirExpr::Const(10, int_ty(32, true))))],
                else_body: vec![HirStmt::If {
                    cond: cond("param_1", 2),
                    then_body: vec![HirStmt::Return(Some(HirExpr::Const(20, int_ty(32, true))))],
                    else_body: vec![HirStmt::If {
                        cond: cond("param_1", 3),
                        then_body: vec![HirStmt::Return(Some(HirExpr::Const(
                            30,
                            int_ty(32, true),
                        )))],
                        else_body: vec![HirStmt::Return(Some(HirExpr::Const(
                            -1,
                            int_ty(32, true),
                        )))],
                    }],
                }],
            }],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        assert!(
            !code.contains("switch ("),
            "impure (Load) switch expression must not collapse to a single-evaluation switch:\n{code}"
        );
    }

    fn gt_const(var: &str, value: i64) -> HirExpr {
        HirExpr::Binary {
            op: HirBinaryOp::Gt,
            lhs: Box::new(HirExpr::Var(var.into())),
            rhs: Box::new(HirExpr::Const(value, int_ty(32, true))),
            ty: NirType::Bool,
        }
    }

    fn lt_const(var: &str, value: i64) -> HirExpr {
        HirExpr::Binary {
            op: HirBinaryOp::SLt,
            lhs: Box::new(HirExpr::Var(var.into())),
            rhs: Box::new(HirExpr::Const(value, int_ty(32, true))),
            ty: NirType::Bool,
        }
    }

    fn select(cond: HirExpr, then_expr: HirExpr, else_expr: HirExpr) -> HirExpr {
        HirExpr::Select {
            cond: Box::new(cond),
            then_expr: Box::new(then_expr),
            else_expr: Box::new(else_expr),
            ty: int_ty(32, true),
        }
    }

    fn const_i(v: i64) -> HirExpr {
        HirExpr::Const(v, int_ty(32, true))
    }

    #[test]
    fn hir_presentation_recovers_switch_from_binary_search_decision_tree() {
        // return x==5 ? 100 : x>5 ? (x==10 ? 200 : -1) : (x==1 ? 300 : -1)
        // A GCC-style binary-search-lowered switch: three cases (5, 10, 1)
        // reached via a mix of equality (case boundaries) and range
        // (`x > 5`) splits, with the *same* default (-1) appearing at two
        // different, differently-shaped leaves.
        let tree = select(
            eq_const("param_1", 5),
            const_i(100),
            select(
                gt_const("param_1", 5),
                select(eq_const("param_1", 10), const_i(200), const_i(-1)),
                select(eq_const("param_1", 1), const_i(300), const_i(-1)),
            ),
        );
        let mut func = HirFunction {
            name: "binary_search_switch".into(),
            params: vec![param("param_1")],
            locals: vec![],
            return_type: int_ty(32, true),
            body: vec![HirStmt::Return(Some(tree))],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        assert!(
            code.contains("switch (param_1)")
                && code.contains("case 5:")
                && code.contains("case 10:")
                && code.contains("case 1:")
                && code.contains("default:"),
            "binary-search decision tree with 3 cases should recover to a switch:\n{code}"
        );
    }

    #[test]
    fn hir_presentation_declines_decision_tree_with_inconsistent_default() {
        // Same shape, but the two "default" leaves disagree (-1 vs -2) --
        // the tree's true fallthrough value is ambiguous, must not guess.
        let tree = select(
            eq_const("param_1", 5),
            const_i(100),
            select(
                gt_const("param_1", 5),
                select(eq_const("param_1", 10), const_i(200), const_i(-1)),
                select(eq_const("param_1", 1), const_i(300), const_i(-2)),
            ),
        );
        let mut func = HirFunction {
            name: "inconsistent_default".into(),
            params: vec![param("param_1")],
            locals: vec![],
            return_type: int_ty(32, true),
            body: vec![HirStmt::Return(Some(tree))],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        assert!(
            !code.contains("switch ("),
            "inconsistent default leaves must not collapse to a switch:\n{code}"
        );
    }

    #[test]
    fn hir_presentation_decision_tree_leaves_unrelated_leaf_ternary_intact() {
        // return x==5 ? (y>0 ? 7 : 9) : x>5 ? -1 : (x==1 ? 300 : (x==2 ? 400 : -1))
        // `y>0 ? 7 : 9` is an unrelated ternary that happens to be case 5's
        // *own result value* -- since its condition isn't a direct
        // comparison on `x` (param_1), it must never be walked into or
        // decomposed, just carried through as case 5's body verbatim.
        let unrelated_leaf = select(gt_const("param_2", 0), const_i(7), const_i(9));
        let tree = select(
            eq_const("param_1", 5),
            unrelated_leaf,
            select(
                gt_const("param_1", 5),
                const_i(-1),
                select(
                    eq_const("param_1", 1),
                    const_i(300),
                    select(eq_const("param_1", 2), const_i(400), const_i(-1)),
                ),
            ),
        );
        let mut func = HirFunction {
            name: "unrelated_leaf".into(),
            params: vec![param("param_1"), param("param_2")],
            locals: vec![],
            return_type: int_ty(32, true),
            body: vec![HirStmt::Return(Some(tree))],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        assert!(
            code.contains("switch (param_1)")
                && code.contains("case 5:")
                && code.contains("case 1:")
                && code.contains("case 2:")
                && code.contains("param_2"),
            "case 5's own unrelated ternary must survive verbatim as its body:\n{code}"
        );
    }

    #[test]
    fn hir_presentation_recovers_switch_with_biased_range_group_and_conditional_default() {
        // Direct reproduction of the dev corpus's `classify_range` (`gcc
        // -O0`), source: `switch (value) { case 0: return 0; case 1: case
        // 2: case 3: return 1; case 10: return 2; default: return value <
        // 0 ? -1 : 3; }`. The compiler's actual decision tree:
        //   x==10 ? 2
        //   : x>10 ? DFLT
        //   : !x ? 0
        //   : x<0 ? DFLT
        //   : (uint)(x-1)>2 ? DFLT : 1
        // where DFLT = `x<0 ? -1 : 3` appears at four *differently shaped*
        // leaves. The case-1/2/3 grouping is only reachable through the
        // biased-subtraction unsigned range check (`Gt` on `x-1`, not a
        // signed comparison) -- this is the exact case that motivated
        // adding `DirectSplitOnVar::Range` and, separately, the
        // `subtree_has_real_case_boundary` fix (an earlier version of this
        // pass shattered the `x<0 ? -1 : 3` default into two inconsistent
        // leaves, `-1` and `3`, and declined the whole tree).
        let default_expr = || select(lt_const("param_1", 0), const_i(-1), const_i(3));
        let range_split = select(
            HirExpr::Binary {
                op: HirBinaryOp::Gt,
                lhs: Box::new(HirExpr::Binary {
                    op: HirBinaryOp::Sub,
                    lhs: Box::new(HirExpr::Var("param_1".into())),
                    rhs: Box::new(const_i(1)),
                    ty: int_ty(32, true),
                }),
                rhs: Box::new(const_i(2)),
                ty: NirType::Bool,
            },
            default_expr(),
            const_i(1),
        );
        let tree = select(
            eq_const("param_1", 10),
            const_i(2),
            select(
                gt_const("param_1", 10),
                default_expr(),
                select(
                    HirExpr::Unary {
                        op: HirUnaryOp::Not,
                        expr: Box::new(HirExpr::Var("param_1".into())),
                        ty: NirType::Bool,
                    },
                    const_i(0),
                    select(lt_const("param_1", 0), default_expr(), range_split),
                ),
            ),
        );
        let mut func = HirFunction {
            name: "classify_range".into(),
            params: vec![param("param_1")],
            locals: vec![],
            return_type: int_ty(32, true),
            body: vec![HirStmt::Return(Some(tree))],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        let code = crate::midend::print_hir_function(&func);
        assert!(
            code.contains("switch (param_1)")
                && code.contains("case 10:")
                && code.contains("case 0:")
                && code.contains("case 1:")
                && code.contains("case 2:")
                && code.contains("case 3:")
                && code.contains("default:")
                && code.contains("param_1 < 0"),
            "should fully recover the grouped-range case and preserve the \
             conditional default expression intact:\n{code}"
        );
    }

    /// ADR 0011: layered render must not mutate the input tree; NIR keeps homes.
    #[test]
    fn layered_render_does_not_mutate_input_and_keeps_nir_mechanical() {
        let func = HirFunction {
            name: "add_like".into(),
            params: vec![param("param_1"), param("param_2")],
            locals: vec![local("param_10"), local("param_18"), local("uVar6")],
            return_type: int_ty(32, true),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("param_10".into()),
                    rhs: HirExpr::Var("param_1".into()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("param_18".into()),
                    rhs: HirExpr::Var("param_2".into()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("uVar6".into()),
                    rhs: HirExpr::Binary {
                        op: HirBinaryOp::Add,
                        lhs: Box::new(HirExpr::Var("param_10".into())),
                        rhs: Box::new(HirExpr::Var("param_18".into())),
                        ty: int_ty(32, true),
                    },
                },
                HirStmt::Return(Some(HirExpr::Var("uVar6".into()))),
            ],
            ..Default::default()
        };
        let before = func.clone();
        let layered = render_layered_pseudocode(&func, &MlilPreviewOptions::default());
        assert_eq!(
            func, before,
            "render_layered_pseudocode must not mutate the input HirFunction"
        );
        assert!(
            layered.nir.contains("param_10") && layered.nir.contains("uVar6"),
            "NIR must stay mechanical:\n{}",
            layered.nir
        );
        assert!(
            !layered.hir.contains("param_10") && layered.hir.contains('+'),
            "HIR may fold aliases:\n{}",
            layered.hir
        );
    }

    /// `i = 0; while (i < n) { …; i = i + 1; }` → `for (i = 0; i < n; i = i + 1)`.
    #[test]
    fn hir_presentation_folds_seed_while_to_for() {
        let i32 = int_ty(32, true);
        let func = HirFunction {
            name: "sum_range".into(),
            params: vec![param("n")],
            locals: vec![local("i"), local("acc")],
            return_type: i32.clone(),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("acc".into()),
                    rhs: HirExpr::Const(0, i32.clone()),
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var("i".into()),
                    rhs: HirExpr::Const(0, i32.clone()),
                },
                HirStmt::While {
                    cond: HirExpr::Binary {
                        op: HirBinaryOp::SLt,
                        lhs: Box::new(HirExpr::Var("i".into())),
                        rhs: Box::new(HirExpr::Var("n".into())),
                        ty: NirType::Bool,
                    },
                    body: vec![
                        HirStmt::Assign {
                            lhs: HirLValue::Var("acc".into()),
                            rhs: HirExpr::Binary {
                                op: HirBinaryOp::Add,
                                lhs: Box::new(HirExpr::Var("acc".into())),
                                rhs: Box::new(HirExpr::Var("i".into())),
                                ty: i32.clone(),
                            },
                        },
                        HirStmt::Assign {
                            lhs: HirLValue::Var("i".into()),
                            rhs: HirExpr::Binary {
                                op: HirBinaryOp::Add,
                                lhs: Box::new(HirExpr::Var("i".into())),
                                rhs: Box::new(HirExpr::Const(1, i32.clone())),
                                ty: i32.clone(),
                            },
                        },
                    ],
                },
                HirStmt::Return(Some(HirExpr::Var("acc".into()))),
            ],
            ..Default::default()
        };
        // Dual-layer: NIR keeps while; HIR presentation folds to for.
        let layered = render_layered_pseudocode(&func, &MlilPreviewOptions::default());
        assert!(
            layered.nir.contains("while (") && !layered.nir.contains("for ("),
            "NIR oracle must keep while:\n{}",
            layered.nir
        );
        assert!(
            layered.hir.contains("for (") && !layered.hir.contains("while ("),
            "HIR should fold seed+while → for:\n{}",
            layered.hir
        );
        assert!(
            layered.hir.contains("acc") && layered.hir.contains("return"),
            "loop body and return must remain:\n{}",
            layered.hir
        );

        let mut polished = func;
        apply_hir_presentation(&mut polished);
        assert!(
            matches!(
                polished
                    .body
                    .iter()
                    .find(|s| matches!(s, HirStmt::For { .. } | HirStmt::While { .. })),
                Some(HirStmt::For { .. })
            ),
            "expected For after presentation, body={:?}",
            polished.body
        );
    }

    /// Mid-body redefinition of the induction blocks for-fold (presentation-safe).
    /// Use a non-pure mid assign so self-update fold cannot merge it into the tail.
    #[test]
    fn hir_presentation_does_not_for_fold_when_induction_reassigned_in_body() {
        let i32 = int_ty(32, true);
        let mut func = HirFunction {
            name: "messy".into(),
            params: vec![param("n")],
            locals: vec![local("i")],
            return_type: i32.clone(),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("i".into()),
                    rhs: HirExpr::Const(0, i32.clone()),
                },
                HirStmt::While {
                    cond: HirExpr::Binary {
                        op: HirBinaryOp::SLt,
                        lhs: Box::new(HirExpr::Var("i".into())),
                        rhs: Box::new(HirExpr::Var("n".into())),
                        ty: NirType::Bool,
                    },
                    body: vec![
                        HirStmt::Assign {
                            lhs: HirLValue::Var("i".into()),
                            rhs: HirExpr::Call {
                                target: "other".into(),
                                args: vec![],
                                ty: i32.clone(),
                            },
                        },
                        HirStmt::Assign {
                            lhs: HirLValue::Var("i".into()),
                            rhs: HirExpr::Binary {
                                op: HirBinaryOp::Add,
                                lhs: Box::new(HirExpr::Var("i".into())),
                                rhs: Box::new(HirExpr::Const(1, i32.clone())),
                                ty: i32.clone(),
                            },
                        },
                    ],
                },
            ],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        assert!(
            func.body.iter().any(|s| matches!(s, HirStmt::While { .. })),
            "must keep While when induction is reassigned mid-body: {:?}",
            func.body
        );
        assert!(
            !func.body.iter().any(|s| matches!(s, HirStmt::For { .. })),
            "must not emit For for messy induction: {:?}",
            func.body
        );
    }

    /// Seed unrelated to while condition must not fold.
    #[test]
    fn hir_presentation_does_not_for_fold_unrelated_seed() {
        let i32 = int_ty(32, true);
        let mut func = HirFunction {
            name: "unrelated".into(),
            params: vec![param("n")],
            locals: vec![local("x"), local("i")],
            return_type: i32.clone(),
            body: vec![
                HirStmt::Assign {
                    lhs: HirLValue::Var("x".into()),
                    rhs: HirExpr::Const(0, i32.clone()),
                },
                HirStmt::While {
                    cond: HirExpr::Binary {
                        op: HirBinaryOp::SLt,
                        lhs: Box::new(HirExpr::Var("i".into())),
                        rhs: Box::new(HirExpr::Var("n".into())),
                        ty: NirType::Bool,
                    },
                    body: vec![HirStmt::Assign {
                        lhs: HirLValue::Var("i".into()),
                        rhs: HirExpr::Binary {
                            op: HirBinaryOp::Add,
                            lhs: Box::new(HirExpr::Var("i".into())),
                            rhs: Box::new(HirExpr::Const(1, i32.clone())),
                            ty: i32.clone(),
                        },
                    }],
                },
            ],
            ..Default::default()
        };
        apply_hir_presentation(&mut func);
        assert!(
            func.body.iter().any(|s| matches!(s, HirStmt::While { .. })),
            "unrelated seed must not become for-init: {:?}",
            func.body
        );
    }
}
