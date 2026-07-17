//! HIR presentation pass — readability-only tree polish before HIR print.
//!
//! Contract: [`docs/adr/0011-hir-presentation-contract.md`] and `render/AGENTS.md`.
//!
//! - Clone-only: NIR print uses the pre-presentation tree.
//! - Preserve evaluation count/order for calls and loads (no double-eval inlines).
//! - Structural invariants only — no function/address/binary special cases.
//! - Semantic recovery stays in normalize/structuring.
//! - Post-pass structural firewall ([`invariants`]): on violation, restore pre-polish tree.

mod invariants;

use super::{
    HirBinaryOp, HirExpr, HirFunction, HirLValue, HirStmt, HirUnaryOp, NirBinding,
    NirBindingOrigin, NirType,
};
use invariants::check_hir_presentation_invariants;
use std::collections::{HashMap, HashSet};

/// Apply HIR-facing presentation polish in place.
///
/// On structural invariant failure (use-without-def, call/load inflation, empty
/// if shells), restores the pre-polish tree so broken presentation never ships.
pub(crate) fn apply_hir_presentation(func: &mut HirFunction) {
    let before = func.clone();
    apply_hir_presentation_passes(func);
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

fn apply_hir_presentation_passes(func: &mut HirFunction) {
    for _ in 0..16 {
        let mut changed = false;
        changed |= flatten_redundant_blocks(&mut func.body);
        changed |= propagate_pure_var_aliases(func);
        changed |= fold_self_update_after_seed(&mut func.body);
        // Shared `goto L; ... L: return e` → direct returns (enables if-else recovery).
        changed |= expand_goto_shared_returns(&mut func.body);
        changed |= collapse_trivial_assign_returns(&mut func.body);
        // O0-style `if (c) goto L; body; L:` → structured if/else for readability.
        changed |= recover_if_else_from_gotos(&mut func.body);
        // O0 while: `goto Lcond; Lbody: …; Lcond: if (c) goto Lbody;`
        changed |= recover_while_from_gotos(&mut func.body);
        // Structuring often emits `while (1) { if (!c) break; body }` → `while (c)`.
        changed |= fold_while_true_break_guard(&mut func.body);
        // `if (c) { x = a; } else { x = b; }` → `x = c ? a : b` (pure values only).
        changed |= fold_if_else_pure_same_var_assign(&mut func.body);
        // `if (c) { return a; } else { return b; }` → `return c ? a : b` (pure values).
        changed |= fold_if_else_pure_returns_to_select(&mut func.body);
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
        changed |= prune_unreachable_after_total_return(&mut func.body);
        changed |= inline_single_use_pure_assigns(func);
        changed |= eliminate_pure_dead_assigns(func);
        changed |= remove_unreferenced_labels(&mut func.body);
        if !changed {
            break;
        }
    }
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
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => true,
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

fn expr_mentions_var(expr: &HirExpr, name: &str) -> bool {
    match expr {
        HirExpr::Var(n) => n == name,
        HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
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
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
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
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
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
        HirExpr::Var(n) => usize::from(n == name),
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

/// Substitute single-def pure `x = y` aliases (param home slots, renames).
fn propagate_pure_var_aliases(func: &mut HirFunction) -> bool {
    let formal: HashSet<&str> = func.params.iter().map(|b| b.name.as_str()).collect();
    let mut def_counts = HashMap::new();
    count_defs_in_stmts(&func.body, &mut def_counts);

    // Collect x → y for single-def pure var copies. Resolve short chains.
    let mut copy_map: HashMap<String, String> = HashMap::new();
    collect_pure_var_copies(&func.body, &formal, &def_counts, &mut copy_map);
    if copy_map.is_empty() {
        return false;
    }

    // Resolve chains x → y → z to x → z (bounded).
    let keys: Vec<String> = copy_map.keys().cloned().collect();
    for k in keys {
        let mut seen = HashSet::new();
        let mut cur = k.clone();
        while let Some(next) = copy_map.get(&cur).cloned() {
            if !seen.insert(cur.clone()) {
                break;
            }
            cur = next;
        }
        if let Some(src) = copy_map.get(&k) {
            if src != &cur {
                copy_map.insert(k, cur);
            }
        }
    }

    let mut changed = false;
    for (name, source) in &copy_map {
        let replacement = HirExpr::Var(source.clone());
        for stmt in &mut func.body {
            replace_var_in_stmt(stmt, name, &replacement);
        }
        changed = true;
    }
    changed |= remove_copy_assigns(&mut func.body, &copy_map);
    changed
}

fn collect_pure_var_copies(
    stmts: &[HirStmt],
    formal: &HashSet<&str>,
    def_counts: &HashMap<String, usize>,
    out: &mut HashMap<String, String>,
) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs: HirExpr::Var(source),
            } if name != source
                && !formal.contains(name.as_str())
                && def_counts.get(name.as_str()).copied().unwrap_or(0) == 1 =>
            {
                out.insert(name.clone(), source.clone());
            }
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                collect_pure_var_copies(body, formal, def_counts, out)
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_pure_var_copies(then_body, formal, def_counts, out);
                collect_pure_var_copies(else_body, formal, def_counts, out);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    collect_pure_var_copies(
                        std::slice::from_ref(i.as_ref()),
                        formal,
                        def_counts,
                        out,
                    );
                }
                if let Some(u) = update {
                    collect_pure_var_copies(
                        std::slice::from_ref(u.as_ref()),
                        formal,
                        def_counts,
                        out,
                    );
                }
                collect_pure_var_copies(body, formal, def_counts, out);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_pure_var_copies(&case.body, formal, def_counts, out);
                }
                collect_pure_var_copies(default, formal, def_counts, out);
            }
            _ => {}
        }
    }
}

fn remove_copy_assigns(stmts: &mut Vec<HirStmt>, copy_map: &HashMap<String, String>) -> bool {
    let mut changed = false;
    let before = stmts.len();
    stmts.retain(|stmt| {
        !matches!(
            stmt,
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs: HirExpr::Var(_),
            } if copy_map.contains_key(name)
        )
    });
    if stmts.len() != before {
        changed = true;
    }
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                changed |= remove_copy_assigns(body, copy_map);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= remove_copy_assigns(then_body, copy_map);
                changed |= remove_copy_assigns(else_body, copy_map);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let HirStmt::Block(b) = i.as_mut() {
                        changed |= remove_copy_assigns(b, copy_map);
                    }
                }
                if let Some(u) = update {
                    if let HirStmt::Block(b) = u.as_mut() {
                        changed |= remove_copy_assigns(b, copy_map);
                    }
                }
                changed |= remove_copy_assigns(body, copy_map);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= remove_copy_assigns(&mut case.body, copy_map);
                }
                changed |= remove_copy_assigns(default, copy_map);
            }
            _ => {}
        }
    }
    changed
}

/// Fold `x = seed; x = x ⊕ rhs` into `x = seed ⊕ rhs` when pure.
fn fold_self_update_after_seed(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i + 1 < stmts.len() {
        let folded = match (&stmts[i], &stmts[i + 1]) {
            (
                HirStmt::Assign {
                    lhs: HirLValue::Var(x1),
                    rhs: seed,
                },
                HirStmt::Assign {
                    lhs: HirLValue::Var(x2),
                    rhs:
                        HirExpr::Binary {
                            op,
                            lhs: bin_lhs,
                            rhs: bin_rhs,
                            ty,
                        },
                },
            ) if x1 == x2
                && expr_is_presentation_pure(seed)
                && !expr_mentions_var(seed, x1)
                && matches!(bin_lhs.as_ref(), HirExpr::Var(n) if n == x1)
                && expr_is_presentation_pure(bin_rhs)
                && !expr_mentions_var(bin_rhs, x1) =>
            {
                Some(HirStmt::Assign {
                    lhs: HirLValue::Var(x1.clone()),
                    rhs: HirExpr::Binary {
                        op: *op,
                        lhs: Box::new(seed.clone()),
                        rhs: bin_rhs.clone(),
                        ty: ty.clone(),
                    },
                })
            }
            _ => None,
        };
        if let Some(stmt) = folded {
            stmts[i] = stmt;
            stmts.remove(i + 1);
            changed = true;
            // Re-examine from same index in case of chains.
            continue;
        }
        i += 1;
    }

    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                changed |= fold_self_update_after_seed(body);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= fold_self_update_after_seed(then_body);
                changed |= fold_self_update_after_seed(else_body);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init_stmt) = init {
                    if let HirStmt::Block(b) = init_stmt.as_mut() {
                        changed |= fold_self_update_after_seed(b);
                    }
                }
                if let Some(upd) = update {
                    if let HirStmt::Block(b) = upd.as_mut() {
                        changed |= fold_self_update_after_seed(b);
                    }
                }
                changed |= fold_self_update_after_seed(body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= fold_self_update_after_seed(&mut case.body);
                }
                changed |= fold_self_update_after_seed(default);
            }
            _ => {}
        }
    }
    changed
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
                && expr_is_presentation_pure(rhs)
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

// ── Goto / label presentation recovery ───────────────────────────────────────

fn invert_cond(cond: HirExpr) -> HirExpr {
    match cond {
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } => *expr,
        other => HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr: Box::new(other),
            ty: NirType::Bool,
        },
    }
}

fn if_is_single_goto(stmt: &HirStmt) -> Option<(&HirExpr, &str)> {
    match stmt {
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } if else_body.is_empty() => match then_body.as_slice() {
            [HirStmt::Goto(label)] => Some((cond, label.as_str())),
            _ => None,
        },
        _ => None,
    }
}

fn stmts_have_label(stmts: &[HirStmt], label: &str) -> bool {
    stmts.iter().any(|s| match s {
        HirStmt::Label(l) => l == label,
        HirStmt::Block(b)
        | HirStmt::While { body: b, .. }
        | HirStmt::DoWhile { body: b, .. }
        | HirStmt::For { body: b, .. } => stmts_have_label(b, label),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => stmts_have_label(then_body, label) || stmts_have_label(else_body, label),
        HirStmt::Switch { cases, default, .. } => {
            cases.iter().any(|c| stmts_have_label(&c.body, label))
                || stmts_have_label(default, label)
        }
        _ => false,
    })
}

fn count_goto_refs(stmts: &[HirStmt], label: &str) -> usize {
    stmts.iter().map(|s| count_goto_refs_stmt(s, label)).sum()
}

fn count_goto_refs_stmt(stmt: &HirStmt, label: &str) -> usize {
    match stmt {
        HirStmt::Goto(l) => usize::from(l == label),
        HirStmt::Block(b) | HirStmt::While { body: b, .. } | HirStmt::DoWhile { body: b, .. } => {
            count_goto_refs(b, label)
        }
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => count_goto_refs(then_body, label) + count_goto_refs(else_body, label),
        HirStmt::Switch { cases, default, .. } => {
            cases
                .iter()
                .map(|c| count_goto_refs(&c.body, label))
                .sum::<usize>()
                + count_goto_refs(default, label)
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            init.as_ref().map_or(0, |s| count_goto_refs_stmt(s, label))
                + update
                    .as_ref()
                    .map_or(0, |s| count_goto_refs_stmt(s, label))
                + count_goto_refs(body, label)
        }
        _ => 0,
    }
}

fn replace_goto_with_return(stmts: &mut [HirStmt], label: &str, ret: &Option<HirExpr>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Goto(l) if l == label => {
                *stmt = HirStmt::Return(ret.clone());
                changed = true;
            }
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. } => {
                changed |= replace_goto_with_return(b, label, ret);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= replace_goto_with_return(then_body, label, ret);
                changed |= replace_goto_with_return(else_body, label, ret);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= replace_goto_with_return(&mut case.body, label, ret);
                }
                changed |= replace_goto_with_return(default, label, ret);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    changed |=
                        replace_goto_with_return(std::slice::from_mut(i.as_mut()), label, ret);
                }
                if let Some(u) = update {
                    changed |=
                        replace_goto_with_return(std::slice::from_mut(u.as_mut()), label, ret);
                }
                changed |= replace_goto_with_return(body, label, ret);
            }
            _ => {}
        }
    }
    changed
}

/// `…; goto L; …; L: return e;` → replace gotos with `return e` (presentation).
fn expand_goto_shared_returns(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    // Recurse into nested structured bodies first.
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= expand_goto_shared_returns(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= expand_goto_shared_returns(then_body);
                changed |= expand_goto_shared_returns(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= expand_goto_shared_returns(&mut case.body);
                }
                changed |= expand_goto_shared_returns(default);
            }
            _ => {}
        }
    }

    let mut i = 0;
    while i + 1 < stmts.len() {
        if let (HirStmt::Label(label), HirStmt::Return(ret)) = (&stmts[i], &stmts[i + 1]) {
            let label = label.clone();
            let ret = ret.clone();
            if count_goto_refs(stmts, &label) > 0 {
                changed |= replace_goto_with_return(stmts, &label, &ret);
                // Drop the label if nothing targets it anymore; keep the return
                // for fall-through predecessors.
                if count_goto_refs(stmts, &label) == 0 {
                    if matches!(&stmts[i], HirStmt::Label(l) if l == &label) {
                        stmts.remove(i);
                        changed = true;
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    changed
}

fn body_is_goto_recoverable(stmts: &[HirStmt]) -> bool {
    // Fallthrough/else bodies used in recovery must not define labels (would
    // break outer label indexing). Nested if/return/assign/goto are fine.
    !stmts.iter().any(|s| matches!(s, HirStmt::Label(_)))
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
            None => invert_cond(guard_cond),
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
                    *cond = invert_cond(std::mem::replace(
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

/// Normalize condition/comparison presentation forms in place.
fn canonicalize_presentation_conditions(func: &mut HirFunction) -> bool {
    let mut changed = false;
    for stmt in &mut func.body {
        changed |= canonicalize_conditions_in_stmt(stmt);
    }
    changed
}

fn canonicalize_conditions_in_stmt(stmt: &mut HirStmt) -> bool {
    let mut changed = false;
    match stmt {
        HirStmt::Assign { rhs, .. } => {
            changed |= canonicalize_conditions_in_expr(rhs);
        }
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) | HirStmt::VaStart { va_list: e, .. } => {
            changed |= canonicalize_conditions_in_expr(e);
        }
        HirStmt::Return(None)
        | HirStmt::Label(_)
        | HirStmt::Goto(_)
        | HirStmt::Break
        | HirStmt::Continue => {}
        HirStmt::Block(body) => {
            for s in body.iter_mut() {
                changed |= canonicalize_conditions_in_stmt(s);
            }
        }
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            changed |= canonicalize_conditions_in_expr(cond);
            for s in body.iter_mut() {
                changed |= canonicalize_conditions_in_stmt(s);
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            changed |= canonicalize_conditions_in_expr(cond);
            for s in then_body.iter_mut() {
                changed |= canonicalize_conditions_in_stmt(s);
            }
            for s in else_body.iter_mut() {
                changed |= canonicalize_conditions_in_stmt(s);
            }
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(c) = cond {
                changed |= canonicalize_conditions_in_expr(c);
            }
            if let Some(i) = init {
                changed |= canonicalize_conditions_in_stmt(i);
            }
            if let Some(u) = update {
                changed |= canonicalize_conditions_in_stmt(u);
            }
            for s in body.iter_mut() {
                changed |= canonicalize_conditions_in_stmt(s);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            changed |= canonicalize_conditions_in_expr(expr);
            for case in cases {
                for s in case.body.iter_mut() {
                    changed |= canonicalize_conditions_in_stmt(s);
                }
            }
            for s in default.iter_mut() {
                changed |= canonicalize_conditions_in_stmt(s);
            }
        }
    }
    changed
}

fn canonicalize_conditions_in_expr(expr: &mut HirExpr) -> bool {
    let mut changed = false;
    match expr {
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr, .. } | HirExpr::Unary { expr, .. } => {
            changed |= canonicalize_conditions_in_expr(expr);
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            changed |= canonicalize_conditions_in_expr(lhs);
            changed |= canonicalize_conditions_in_expr(rhs);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= canonicalize_conditions_in_expr(cond);
            changed |= canonicalize_conditions_in_expr(then_expr);
            changed |= canonicalize_conditions_in_expr(else_expr);
        }
        HirExpr::Call { args, .. } => {
            for a in args {
                changed |= canonicalize_conditions_in_expr(a);
            }
        }
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. }
        | HirExpr::AggregateCopy { src: ptr, .. } => {
            changed |= canonicalize_conditions_in_expr(ptr);
        }
        HirExpr::Index { base, index, .. } => {
            changed |= canonicalize_conditions_in_expr(base);
            changed |= canonicalize_conditions_in_expr(index);
        }
    }
    // Post-order local rewrites so nested forms normalize first.
    changed |= rewrite_presentation_condition_form(expr);
    changed
}

fn rewrite_presentation_condition_form(expr: &mut HirExpr) -> bool {
    // `!(x == 0)` → `x != 0`, `!(x != 0)` → `x == 0`.
    // `!!e` only when the outer result is Bool (value `!!x` ≠ `x` for int x∉{0,1}).
    if let HirExpr::Unary {
        op: HirUnaryOp::Not,
        expr: inner,
        ty: outer_ty,
    } = expr
    {
        if matches!(outer_ty, NirType::Bool) {
            if let HirExpr::Unary {
                op: HirUnaryOp::Not,
                expr: inner2,
                ..
            } = inner.as_ref()
            {
                *expr = inner2.as_ref().clone();
                return true;
            }
        }
        if let HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs,
            rhs,
            ty,
        } = inner.as_ref()
        {
            *expr = HirExpr::Binary {
                op: HirBinaryOp::Ne,
                lhs: lhs.clone(),
                rhs: rhs.clone(),
                ty: ty.clone(),
            };
            return true;
        }
        if let HirExpr::Binary {
            op: HirBinaryOp::Ne,
            lhs,
            rhs,
            ty,
        } = inner.as_ref()
        {
            *expr = HirExpr::Binary {
                op: HirBinaryOp::Eq,
                lhs: lhs.clone(),
                rhs: rhs.clone(),
                ty: ty.clone(),
            };
            return true;
        }
    }

    // Const-left comparisons → var/expr-left with flipped op.
    if let HirExpr::Binary {
        op,
        lhs,
        rhs,
        ty,
    } = expr
    {
        let lhs_is_const = matches!(lhs.as_ref(), HirExpr::Const(_, _));
        let rhs_is_const = matches!(rhs.as_ref(), HirExpr::Const(_, _));
        if lhs_is_const && !rhs_is_const {
            if let Some(flipped) = flip_comparison_op(*op) {
                let new_lhs = std::mem::replace(rhs.as_mut(), HirExpr::Const(0, NirType::Unknown));
                let new_rhs = std::mem::replace(lhs.as_mut(), HirExpr::Const(0, NirType::Unknown));
                *op = flipped;
                *lhs.as_mut() = new_lhs;
                *rhs.as_mut() = new_rhs;
                let _ = ty;
                return true;
            }
            if matches!(*op, HirBinaryOp::Eq | HirBinaryOp::Ne) {
                std::mem::swap(lhs, rhs);
                return true;
            }
        }
    }
    false
}

fn flip_comparison_op(op: HirBinaryOp) -> Option<HirBinaryOp> {
    Some(match op {
        HirBinaryOp::Lt => HirBinaryOp::Gt,
        HirBinaryOp::Le => HirBinaryOp::Ge,
        HirBinaryOp::Gt => HirBinaryOp::Lt,
        HirBinaryOp::Ge => HirBinaryOp::Le,
        HirBinaryOp::SLt => HirBinaryOp::SGt,
        HirBinaryOp::SLe => HirBinaryOp::SGe,
        HirBinaryOp::SGt => HirBinaryOp::SLt,
        HirBinaryOp::SGe => HirBinaryOp::SLe,
        _ => return None,
    })
}

fn body_is_effectively_empty(stmts: &[HirStmt]) -> bool {
    stmts.iter().all(|s| match s {
        HirStmt::Block(inner) => body_is_effectively_empty(inner),
        HirStmt::Label(_) => true,
        _ => false,
    })
}

fn is_presentation_noise_stmt(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Label(_) => true,
        HirStmt::Block(b) if b.is_empty() => true,
        _ => false,
    }
}

fn single_var_assign(stmts: &[HirStmt]) -> Option<(&str, &HirExpr)> {
    let meaningful: Vec<&HirStmt> = stmts
        .iter()
        .filter(|s| !is_presentation_noise_stmt(s))
        .collect();
    match meaningful.as_slice() {
        [HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        }] => Some((name.as_str(), rhs)),
        [HirStmt::Block(inner)] => single_var_assign(inner),
        _ => None,
    }
}

fn single_return_expr(stmts: &[HirStmt]) -> Option<&HirExpr> {
    let meaningful: Vec<&HirStmt> = stmts
        .iter()
        .filter(|s| !is_presentation_noise_stmt(s))
        .collect();
    match meaningful.as_slice() {
        [HirStmt::Return(Some(expr))] => Some(expr),
        [HirStmt::Block(inner)] => single_return_expr(inner),
        _ => None,
    }
}

fn expr_result_type(expr: &HirExpr) -> NirType {
    match expr {
        HirExpr::Const(_, ty)
        | HirExpr::Unary { ty, .. }
        | HirExpr::Binary { ty, .. }
        | HirExpr::Select { ty, .. }
        | HirExpr::Call { ty, .. }
        | HirExpr::Load { ty, .. }
        | HirExpr::Cast { ty, .. }
        | HirExpr::FieldAccess { ty, .. }
        | HirExpr::Index { elem_ty: ty, .. } => ty.clone(),
        HirExpr::PtrOffset { .. } | HirExpr::AddressOfGlobal(_) => {
            NirType::Ptr(Box::new(NirType::Unknown))
        }
        HirExpr::Var(_) | HirExpr::AggregateCopy { .. } => NirType::Unknown,
    }
}

/// Fold null-check join sugar:
/// `x = c ? a : b; return ~c ? x : a` → `return x` (then collapse even with call).
fn fold_redundant_select_return_join(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= fold_redundant_select_return_join(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= fold_redundant_select_return_join(then_body);
                changed |= fold_redundant_select_return_join(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= fold_redundant_select_return_join(&mut case.body);
                }
                changed |= fold_redundant_select_return_join(default);
            }
            _ => {}
        }
    }

    let mut i = 0;
    while i + 1 < stmts.len() {
        let can_fold_to_x = match (&stmts[i], &stmts[i + 1]) {
            (
                HirStmt::Assign {
                    lhs: HirLValue::Var(x),
                    rhs:
                        HirExpr::Select {
                            cond: c1,
                            then_expr: t1,
                            else_expr: _e1,
                            ..
                        },
                },
                HirStmt::Return(Some(HirExpr::Select {
                    cond: c2,
                    then_expr: t2,
                    else_expr: e2,
                    ..
                })),
            ) => {
                let x_var = |e: &HirExpr| matches!(e, HirExpr::Var(n) if n == x);
                // apply_binop: x = !p ? 0 : call; return !(p==0) ? x : 0
                // c1 nullish, c2 non-nullish, t2=x, e2=t1
                (cond_are_negations(c1, c2) && x_var(t2) && t1.as_ref() == e2.as_ref())
                    // x = c ? a : b; return c ? a : x
                    || (cond_logically_same(c1, c2) && t1.as_ref() == t2.as_ref() && x_var(e2))
                    // x = c ? a : b; return ~c ? a : x  (less common)
                    || (cond_are_negations(c1, c2) && t1.as_ref() == t2.as_ref() && x_var(e2))
            }
            _ => false,
        };
        if can_fold_to_x {
            if let HirStmt::Assign {
                lhs: HirLValue::Var(x),
                ..
            } = &stmts[i]
            {
                let x = x.clone();
                stmts[i + 1] = HirStmt::Return(Some(HirExpr::Var(x)));
                changed = true;
            }
        }
        // Collapse `x = rhs; return x` for any rhs (call/select safe: single eval).
        if let (
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs,
            },
            HirStmt::Return(Some(HirExpr::Var(ret_name))),
        ) = (&stmts[i], &stmts[i + 1])
        {
            if name == ret_name {
                stmts[i] = HirStmt::Return(Some(rhs.clone()));
                stmts.remove(i + 1);
                changed = true;
                continue;
            }
        }
        i += 1;
    }
    changed
}

fn cond_logically_same(a: &HirExpr, b: &HirExpr) -> bool {
    if a == b {
        return true;
    }
    normalize_truthiness(a) == normalize_truthiness(b)
}

fn cond_are_negations(a: &HirExpr, b: &HirExpr) -> bool {
    if let Some(inner) = peel_logical_not(a) {
        if cond_logically_same(&inner, b) {
            return true;
        }
    }
    if let Some(inner) = peel_logical_not(b) {
        if cond_logically_same(&inner, a) {
            return true;
        }
    }
    match (normalize_truthiness(a), normalize_truthiness(b)) {
        (Truthiness::Zero(x), Truthiness::NonZero(y))
        | (Truthiness::NonZero(x), Truthiness::Zero(y)) => x == y,
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Truthiness {
    /// Expression is true when `var == 0` / nullish.
    Zero(String),
    /// Expression is true when `var != 0` / non-null.
    NonZero(String),
    Other,
}

fn normalize_truthiness(expr: &HirExpr) -> Truthiness {
    match expr {
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } => match normalize_truthiness(expr) {
            Truthiness::Zero(v) => Truthiness::NonZero(v),
            Truthiness::NonZero(v) => Truthiness::Zero(v),
            Truthiness::Other => Truthiness::Other,
        },
        HirExpr::Binary {
            op: HirBinaryOp::Eq,
            lhs,
            rhs,
            ..
        } => match (lhs.as_ref(), rhs.as_ref()) {
            (HirExpr::Var(v), HirExpr::Const(0, _)) | (HirExpr::Const(0, _), HirExpr::Var(v)) => {
                Truthiness::Zero(v.clone())
            }
            _ => Truthiness::Other,
        },
        HirExpr::Binary {
            op: HirBinaryOp::Ne,
            lhs,
            rhs,
            ..
        } => match (lhs.as_ref(), rhs.as_ref()) {
            (HirExpr::Var(v), HirExpr::Const(0, _)) | (HirExpr::Const(0, _), HirExpr::Var(v)) => {
                Truthiness::NonZero(v.clone())
            }
            _ => Truthiness::Other,
        },
        // Bare `var` used as condition ⇒ non-zero / non-null.
        HirExpr::Var(v) => Truthiness::NonZero(v.clone()),
        // `!var` handled above via Unary Not.
        HirExpr::Cast { expr, .. } => normalize_truthiness(expr),
        _ => Truthiness::Other,
    }
}

fn peel_logical_not(expr: &HirExpr) -> Option<HirExpr> {
    match expr {
        HirExpr::Unary {
            op: HirUnaryOp::Not,
            expr,
            ..
        } => Some(expr.as_ref().clone()),
        _ => None,
    }
}

/// ```text
/// goto Lcond;
/// Lbody:
///   body…
/// Lcond:
///   if (cond) goto Lbody;
/// ```
fn recover_while_from_gotos(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= recover_while_from_gotos(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= recover_while_from_gotos(then_body);
                changed |= recover_while_from_gotos(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= recover_while_from_gotos(&mut case.body);
                }
                changed |= recover_while_from_gotos(default);
            }
            _ => {}
        }
    }

    let mut i = 0;
    while i < stmts.len() {
        if try_recover_while_at(stmts, i) {
            changed = true;
            continue;
        }
        i += 1;
    }
    changed
}

fn try_recover_while_at(stmts: &mut Vec<HirStmt>, i: usize) -> bool {
    let HirStmt::Goto(lcond) = &stmts[i] else {
        return false;
    };
    let lcond = lcond.clone();

    let Some(c_idx) = stmts
        .iter()
        .enumerate()
        .skip(i + 1)
        .find_map(|(idx, s)| match s {
            HirStmt::Label(l) if l == &lcond => Some(idx),
            _ => None,
        })
    else {
        return false;
    };

    if c_idx + 1 >= stmts.len() {
        return false;
    }

    let body_region = &stmts[i + 1..c_idx];
    // Body must start with Lbody label targeted by the condition if.
    let Some(HirStmt::Label(lbody)) = body_region.first() else {
        return false;
    };
    let lbody = lbody.clone();

    let Some((cond, target)) = if_is_single_goto(&stmts[c_idx + 1]) else {
        return false;
    };
    if target != lbody {
        return false;
    }
    let cond = cond.clone();

    // Only this loop's back-edge should target Lbody (plus nothing else in range).
    // Allow gotos to Lbody only from the condition if we are about to remove.
    let goto_lbody = count_goto_refs(stmts, &lbody);
    // The condition if contributes 1; any other ref blocks recovery.
    if goto_lbody != 1 {
        return false;
    }

    let mut body: Vec<HirStmt> = body_region[1..].to_vec();
    if stmts_have_label(&body, &lcond) || stmts_have_label(&body, &lbody) {
        return false;
    }
    // No other labels inside body (would break linear while).
    if body.iter().any(|s| matches!(s, HirStmt::Label(_))) {
        return false;
    }

    // `goto Lcond` inside body → continue (recheck condition).
    rewrite_goto_to_continue(&mut body, &lcond);

    let mut rebuilt = Vec::with_capacity(stmts.len());
    rebuilt.extend_from_slice(&stmts[..i]);
    rebuilt.push(HirStmt::While { cond, body });
    // Skip: goto Lcond, Lbody.., Label(Lcond), if (cond) goto Lbody
    rebuilt.extend_from_slice(&stmts[c_idx + 2..]);
    *stmts = rebuilt;
    true
}

fn rewrite_goto_to_continue(stmts: &mut [HirStmt], label: &str) {
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Goto(l) if l == label => {
                *stmt = HirStmt::Continue;
            }
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => rewrite_goto_to_continue(b, label),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                rewrite_goto_to_continue(then_body, label);
                rewrite_goto_to_continue(else_body, label);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    rewrite_goto_to_continue(&mut case.body, label);
                }
                rewrite_goto_to_continue(default, label);
            }
            _ => {}
        }
    }
}

/// Peel redundant integer casts in the presentation tree (HIR only).
fn simplify_presentation_casts(func: &mut HirFunction) {
    let mut var_types: HashMap<String, NirType> = HashMap::new();
    for b in func.params.iter().chain(func.locals.iter()) {
        var_types.insert(b.name.clone(), b.ty.clone());
    }
    simplify_casts_in_stmts(&mut func.body, &var_types);
}

fn simplify_casts_in_stmts(stmts: &mut [HirStmt], var_types: &HashMap<String, NirType>) {
    for stmt in stmts.iter_mut() {
        simplify_casts_in_stmt(stmt, var_types);
    }
}

fn simplify_casts_in_stmt(stmt: &mut HirStmt, var_types: &HashMap<String, NirType>) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            simplify_casts_in_lvalue(lhs, var_types);
            simplify_casts_in_expr(rhs, var_types);
        }
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) | HirStmt::VaStart { va_list: e, .. } => {
            simplify_casts_in_expr(e, var_types)
        }
        HirStmt::Block(b) => simplify_casts_in_stmts(b, var_types),
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            simplify_casts_in_expr(cond, var_types);
            simplify_casts_in_stmts(body, var_types);
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            simplify_casts_in_expr(cond, var_types);
            simplify_casts_in_stmts(then_body, var_types);
            simplify_casts_in_stmts(else_body, var_types);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                simplify_casts_in_stmt(i, var_types);
            }
            if let Some(c) = cond {
                simplify_casts_in_expr(c, var_types);
            }
            if let Some(u) = update {
                simplify_casts_in_stmt(u, var_types);
            }
            simplify_casts_in_stmts(body, var_types);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            simplify_casts_in_expr(expr, var_types);
            for case in cases {
                simplify_casts_in_stmts(&mut case.body, var_types);
            }
            simplify_casts_in_stmts(default, var_types);
        }
        _ => {}
    }
}

fn simplify_casts_in_lvalue(lhs: &mut HirLValue, var_types: &HashMap<String, NirType>) {
    match lhs {
        HirLValue::Var(_) => {}
        HirLValue::Deref { ptr, .. } => simplify_casts_in_expr(ptr, var_types),
        HirLValue::Index { base, index, .. } => {
            simplify_casts_in_expr(base, var_types);
            simplify_casts_in_expr(index, var_types);
        }
        HirLValue::FieldAccess { base, .. } => simplify_casts_in_expr(base, var_types),
    }
}

fn simplify_casts_in_expr(expr: &mut HirExpr, var_types: &HashMap<String, NirType>) {
    match expr {
        HirExpr::Cast { ty, expr: inner } => {
            simplify_casts_in_expr(inner, var_types);
            // Peel (T)(T)x
            if let HirExpr::Cast {
                ty: inner_ty,
                expr: deeper,
            } = inner.as_ref()
            {
                if inner_ty == ty {
                    *inner = deeper.clone();
                    simplify_casts_in_expr(expr, var_types);
                    return;
                }
                // Peel outer wider unsigned over inner unsigned int cast family:
                // (ulonglong)(uint)x → (ulonglong)x when only width sugar.
                if let (
                    NirType::Int {
                        bits: outer_bits,
                        signed: false,
                    },
                    NirType::Int {
                        bits: inner_bits,
                        signed: false,
                    },
                ) = (&*ty, inner_ty)
                {
                    if outer_bits >= inner_bits {
                        *inner = deeper.clone();
                    }
                }
            }
            // (T)v when v is declared as T → v
            if let HirExpr::Var(name) = inner.as_ref() {
                if var_types.get(name.as_str()).is_some_and(|vt| vt == &*ty) {
                    *expr = HirExpr::Var(name.clone());
                    return;
                }
            }
            // (T)const when const already carries T
            if let HirExpr::Const(v, cty) = inner.as_ref() {
                if cty == &*ty {
                    *expr = HirExpr::Const(*v, ty.clone());
                }
            }
        }
        HirExpr::Unary { expr: e, .. } => simplify_casts_in_expr(e, var_types),
        HirExpr::Binary { lhs, rhs, .. } => {
            simplify_casts_in_expr(lhs, var_types);
            simplify_casts_in_expr(rhs, var_types);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            simplify_casts_in_expr(cond, var_types);
            simplify_casts_in_expr(then_expr, var_types);
            simplify_casts_in_expr(else_expr, var_types);
        }
        HirExpr::Call { args, .. } => {
            for a in args {
                simplify_casts_in_expr(a, var_types);
            }
        }
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. }
        | HirExpr::AggregateCopy { src: ptr, .. } => simplify_casts_in_expr(ptr, var_types),
        HirExpr::Index { base, index, .. } => {
            simplify_casts_in_expr(base, var_types);
            simplify_casts_in_expr(index, var_types);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }
}

/// Recover structured if/else from O0 `if (c) goto` / label shapes.
fn recover_if_else_from_gotos(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= recover_if_else_from_gotos(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= recover_if_else_from_gotos(then_body);
                changed |= recover_if_else_from_gotos(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= recover_if_else_from_gotos(&mut case.body);
                }
                changed |= recover_if_else_from_gotos(default);
            }
            _ => {}
        }
    }

    let mut i = 0;
    while i < stmts.len() {
        if try_recover_if_else_at(stmts, i) {
            changed = true;
            // Restart from i so nested/adjacent patterns can fire.
            continue;
        }
        i += 1;
    }
    changed
}

fn try_recover_if_else_at(stmts: &mut Vec<HirStmt>, i: usize) -> bool {
    // After goto→return expansion: `if (C) return a; return b;` → if/else.
    if try_recover_if_return_else_fallthrough(stmts, i) {
        return true;
    }

    let Some((cond, lelse)) = if_is_single_goto(&stmts[i]) else {
        return false;
    };
    let cond = cond.clone();
    let lelse = lelse.to_string();

    // Find Label(lelse) after i.
    let Some(le_idx) = stmts
        .iter()
        .enumerate()
        .skip(i + 1)
        .find_map(|(idx, s)| match s {
            HirStmt::Label(l) if l == &lelse => Some(idx),
            _ => None,
        })
    else {
        return false;
    };

    let fallthrough = &stmts[i + 1..le_idx];
    if !body_is_goto_recoverable(fallthrough) {
        return false;
    }

    // Pattern: if (C) goto Lelse; THEN; goto Lend; Label(Lelse); ELSE; Label(Lend);
    if let Some(HirStmt::Goto(lend)) = fallthrough.last() {
        let lend = lend.clone();
        if let Some(lend_idx) = stmts
            .iter()
            .enumerate()
            .skip(le_idx + 1)
            .find_map(|(idx, s)| match s {
                HirStmt::Label(l) if l == &lend => Some(idx),
                _ => None,
            })
        {
            let else_body = &stmts[le_idx + 1..lend_idx];
            if body_is_goto_recoverable(else_body) && !stmts_have_label(else_body, &lend) {
                // if (C) { else_body } else { THEN without final goto }
                let then_for_c: Vec<HirStmt> = else_body.to_vec();
                let else_for_c: Vec<HirStmt> = fallthrough[..fallthrough.len() - 1].to_vec();
                let mut rebuilt = Vec::with_capacity(stmts.len());
                rebuilt.extend_from_slice(&stmts[..i]);
                rebuilt.push(HirStmt::If {
                    cond,
                    then_body: then_for_c,
                    else_body: else_for_c,
                });
                // Keep Label(lend) and tail for other fallthroughs.
                rebuilt.extend_from_slice(&stmts[lend_idx..]);
                *stmts = rebuilt;
                return true;
            }
        }
    }

    // Pattern: if (C) goto Lelse; THEN...; Label(Lelse); ELSE...
    // where THEN has no trailing shared lend label — both sides are self-contained
    // (typically after goto→return expansion).
    let then_body = fallthrough.to_vec();
    // ELSE runs from after Lelse until next top-level label or end. If the next
    // statement after Lelse is another Label that is not needed, take until end
    // of contiguous non-label run... Actually for:
    //   if (C) goto L; THEN; L: ELSE_STMTS...
    // ELSE is everything after L until we cannot safely absorb — use rest of list
    // only when THEN is terminal (ends with return/goto/break/continue) so control
    // never falls from THEN into ELSE without the label.
    let then_terminal = then_body.last().is_some_and(|s| {
        matches!(
            s,
            HirStmt::Return(_) | HirStmt::Goto(_) | HirStmt::Break | HirStmt::Continue
        )
    });
    if then_terminal || then_body.is_empty() {
        // Take else body as statements after label until next Label (exclusive) or end.
        let else_end = stmts[le_idx + 1..]
            .iter()
            .position(|s| matches!(s, HirStmt::Label(_)))
            .map(|p| le_idx + 1 + p)
            .unwrap_or(stmts.len());
        let else_body = stmts[le_idx + 1..else_end].to_vec();
        if body_is_goto_recoverable(&else_body) {
            // if (C) goto Lelse → when C, run else_body; when !C, run then_body
            let mut rebuilt = Vec::with_capacity(stmts.len());
            rebuilt.extend_from_slice(&stmts[..i]);
            rebuilt.push(HirStmt::If {
                cond,
                then_body: else_body,
                else_body: then_body,
            });
            rebuilt.extend_from_slice(&stmts[else_end..]);
            *stmts = rebuilt;
            return true;
        }
    }

    // Pattern: if (C) goto Lskip; BODY; Label(Lskip);  (no else body — skip only)
    // → if (!C) { BODY }
    if le_idx + 1 == stmts.len()
        || !matches!(&stmts[le_idx + 1], HirStmt::Label(_)) && count_goto_refs(stmts, &lelse) == 1
    {
        // If there is content after the label that is not exclusively the else of
        // this if, only recover skip when nothing after label belongs to an else
        // that was meant to pair — i.e. when fallthrough is the only body and
        // label has a single goto ref.
        if count_goto_refs(stmts, &lelse) == 1 && body_is_goto_recoverable(fallthrough) {
            // Only pure skip when there is no "else" material that should pair —
            // empty after label OR after label continues sequential code that
            // both paths should reach (fallthrough from label).
            // if (!C) { fallthrough }; Label; tail
            let mut rebuilt = Vec::with_capacity(stmts.len());
            rebuilt.extend_from_slice(&stmts[..i]);
            if !fallthrough.is_empty() {
                rebuilt.push(HirStmt::If {
                    cond: invert_cond(cond),
                    then_body: fallthrough.to_vec(),
                    else_body: vec![],
                });
            }
            // Keep label for fallthrough join if still needed by others; if single
            // ref (this if, now removed), drop label.
            if count_goto_refs(&stmts[le_idx + 1..], &lelse) > 0 {
                rebuilt.extend_from_slice(&stmts[le_idx..]);
            } else {
                rebuilt.extend_from_slice(&stmts[le_idx + 1..]);
            }
            *stmts = rebuilt;
            return true;
        }
    }

    false
}

/// `if (C) { return …; } <terminal fallthrough>` → if/else.
fn try_recover_if_return_else_fallthrough(stmts: &mut Vec<HirStmt>, i: usize) -> bool {
    let HirStmt::If {
        cond,
        then_body,
        else_body,
    } = &stmts[i]
    else {
        return false;
    };
    if !else_body.is_empty() {
        return false;
    }
    let then_is_return = matches!(then_body.as_slice(), [HirStmt::Return(_)]);
    if !then_is_return {
        return false;
    }
    if i + 1 >= stmts.len() {
        return false;
    }
    // Absorb a straight-line terminal suffix as the else branch.
    let mut end = i + 1;
    while end < stmts.len() {
        match &stmts[end] {
            HirStmt::Label(_) => break,
            HirStmt::Return(_) => {
                end += 1;
                break;
            }
            HirStmt::Assign { .. } | HirStmt::Expr(_) => {
                end += 1;
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } if matches!(then_body.as_slice(), [HirStmt::Return(_)])
                && (else_body.is_empty()
                    || matches!(else_body.as_slice(), [HirStmt::Return(_)])) =>
            {
                // Nested already-structured if-return may be part of else.
                end += 1;
            }
            _ => break,
        }
    }
    if end == i + 1 {
        return false;
    }
    // Else must end terminal.
    if !matches!(
        stmts[end - 1],
        HirStmt::Return(_) | HirStmt::Break | HirStmt::Continue
    ) {
        return false;
    }
    let cond = cond.clone();
    let then_body = then_body.clone();
    let else_body = stmts[i + 1..end].to_vec();
    let mut rebuilt = Vec::with_capacity(stmts.len());
    rebuilt.extend_from_slice(&stmts[..i]);
    rebuilt.push(HirStmt::If {
        cond,
        then_body,
        else_body,
    });
    rebuilt.extend_from_slice(&stmts[end..]);
    *stmts = rebuilt;
    true
}

/// Drop fallthrough stmts after an if whose every branch already returns.
fn prune_unreachable_after_total_return(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= prune_unreachable_after_total_return(b);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= prune_unreachable_after_total_return(then_body);
                changed |= prune_unreachable_after_total_return(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for c in cases {
                    changed |= prune_unreachable_after_total_return(&mut c.body);
                }
                changed |= prune_unreachable_after_total_return(default);
            }
            _ => {}
        }
    }

    let mut i = 0;
    while i < stmts.len() {
        if stmt_seq_always_returns(std::slice::from_ref(&stmts[i])) {
            let before = stmts.len();
            stmts.truncate(i + 1);
            if stmts.len() != before {
                changed = true;
            }
            break;
        }
        i += 1;
    }
    changed
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SeqExit {
    /// Every path ends in `return`.
    Return,
    /// Some path falls through the end of the sequence/statement.
    Fallthrough,
    /// Some path leaves via goto/break/continue (or mixed non-return exit).
    Other,
}

fn stmt_seq_always_returns(stmts: &[HirStmt]) -> bool {
    matches!(seq_exit(stmts), SeqExit::Return)
}

fn seq_exit(stmts: &[HirStmt]) -> SeqExit {
    let mut i = 0;
    while i < stmts.len() {
        match stmt_exit(&stmts[i]) {
            SeqExit::Return => return SeqExit::Return,
            SeqExit::Other => return SeqExit::Other,
            SeqExit::Fallthrough => i += 1,
        }
    }
    SeqExit::Fallthrough
}

fn stmt_exit(stmt: &HirStmt) -> SeqExit {
    match stmt {
        HirStmt::Return(_) => SeqExit::Return,
        HirStmt::Goto(_) | HirStmt::Break | HirStmt::Continue => SeqExit::Other,
        HirStmt::Block(b) => seq_exit(b),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            let t = seq_exit(then_body);
            let e = if else_body.is_empty() {
                SeqExit::Fallthrough
            } else {
                seq_exit(else_body)
            };
            merge_branch_exit(t, e)
        }
        HirStmt::Switch { cases, default, .. } => {
            if cases.is_empty() {
                return SeqExit::Fallthrough;
            }
            let mut acc = seq_exit(default);
            for case in cases {
                acc = merge_branch_exit(acc, seq_exit(&case.body));
            }
            acc
        }
        // Assigns/labels fall through; loops conservatively may fall through.
        _ => SeqExit::Fallthrough,
    }
}

fn merge_branch_exit(a: SeqExit, b: SeqExit) -> SeqExit {
    use SeqExit::*;
    match (a, b) {
        (Return, Return) => Return,
        (Fallthrough, Fallthrough) => Fallthrough,
        (Return, Fallthrough) | (Fallthrough, Return) => Fallthrough,
        _ => Other,
    }
}

fn remove_unreferenced_labels(stmts: &mut Vec<HirStmt>) -> bool {
    let mut labels = HashSet::new();
    collect_labels(stmts, &mut labels);
    let mut referenced = HashSet::new();
    for lab in &labels {
        if count_goto_refs(stmts, lab) > 0 {
            referenced.insert(lab.clone());
        }
    }
    remove_labels_not_in(stmts, &referenced)
}

fn collect_labels(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for s in stmts {
        match s {
            HirStmt::Label(l) => {
                out.insert(l.clone());
            }
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => collect_labels(b, out),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_labels(then_body, out);
                collect_labels(else_body, out);
            }
            HirStmt::Switch { cases, default, .. } => {
                for c in cases {
                    collect_labels(&c.body, out);
                }
                collect_labels(default, out);
            }
            _ => {}
        }
    }
}

fn remove_labels_not_in(stmts: &mut Vec<HirStmt>, keep: &HashSet<String>) -> bool {
    let before = stmts.len();
    stmts.retain(|s| match s {
        HirStmt::Label(l) => keep.contains(l),
        _ => true,
    });
    let mut changed = stmts.len() != before;
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(b)
            | HirStmt::While { body: b, .. }
            | HirStmt::DoWhile { body: b, .. }
            | HirStmt::For { body: b, .. } => {
                changed |= remove_labels_not_in(b, keep);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= remove_labels_not_in(then_body, keep);
                changed |= remove_labels_not_in(else_body, keep);
            }
            HirStmt::Switch { cases, default, .. } => {
                for c in cases {
                    changed |= remove_labels_not_in(&mut c.body, keep);
                }
                changed |= remove_labels_not_in(default, keep);
            }
            _ => {}
        }
    }
    changed
}

fn eliminate_pure_dead_assigns(func: &mut HirFunction) -> bool {
    let formal: HashSet<&str> = func.params.iter().map(|b| b.name.as_str()).collect();
    // Whole-function use counts only. Nested subtree-local counts incorrectly
    // treat `if { x = e; } return x;` as dead `x` (use lives outside the if body).
    let mut any = false;
    for _ in 0..16 {
        let mut defs = HashMap::new();
        count_defs_in_stmts(&func.body, &mut defs);
        let use_counts: HashMap<String, usize> = defs
            .keys()
            .map(|n| (n.clone(), count_uses_in_stmts(&func.body, n)))
            .collect();
        let changed = eliminate_pure_dead_in_stmts(&mut func.body, &formal, &use_counts);
        if !changed {
            break;
        }
        any = true;
    }
    any
}

fn eliminate_pure_dead_in_stmts(
    stmts: &mut Vec<HirStmt>,
    formal: &HashSet<&str>,
    use_counts: &HashMap<String, usize>,
) -> bool {
    let mut changed = false;
    let before = stmts.len();
    stmts.retain(|stmt| match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } if !formal.contains(name.as_str())
            && use_counts.get(name.as_str()).copied().unwrap_or(0) == 0
            && expr_is_presentation_pure(rhs) =>
        {
            // Includes flag (zf/sf/…) and pure-intrinsic temps once unused.
            changed = true;
            false
        }
        _ => true,
    });
    if stmts.len() != before {
        changed = true;
    }

    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                changed |= eliminate_pure_dead_in_stmts(body, formal, use_counts);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= eliminate_pure_dead_in_stmts(then_body, formal, use_counts);
                changed |= eliminate_pure_dead_in_stmts(else_body, formal, use_counts);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init_stmt) = init {
                    if let HirStmt::Block(b) = init_stmt.as_mut() {
                        changed |= eliminate_pure_dead_in_stmts(b, formal, use_counts);
                    }
                }
                if let Some(upd) = update {
                    if let HirStmt::Block(b) = upd.as_mut() {
                        changed |= eliminate_pure_dead_in_stmts(b, formal, use_counts);
                    }
                }
                changed |= eliminate_pure_dead_in_stmts(body, formal, use_counts);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= eliminate_pure_dead_in_stmts(&mut case.body, formal, use_counts);
                }
                changed |= eliminate_pure_dead_in_stmts(default, formal, use_counts);
            }
            _ => {}
        }
    }
    changed
}

fn drop_unused_presentation_locals(func: &mut HirFunction) {
    let mut used = HashSet::new();
    collect_used_names_stmts(&func.body, &mut used);
    for p in &func.params {
        used.insert(p.name.clone());
    }
    // Drop any never-referenced local for HIR presentation (including home
    // scaffold and temps whose assigns were folded away).
    func.locals.retain(|b| used.contains(&b.name));
}

fn collect_used_names_stmts(stmts: &[HirStmt], out: &mut HashSet<String>) {
    for s in stmts {
        collect_used_names_stmt(s, out);
    }
}

fn collect_used_names_stmt(stmt: &HirStmt, out: &mut HashSet<String>) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            collect_used_names_lvalue(lhs, out);
            collect_used_names_expr(rhs, out);
        }
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) => collect_used_names_expr(e, out),
        HirStmt::Return(None) => {}
        HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            collect_used_names_stmts(body, out)
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_used_names_expr(cond, out);
            collect_used_names_stmts(then_body, out);
            collect_used_names_stmts(else_body, out);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                collect_used_names_stmt(i, out);
            }
            if let Some(c) = cond {
                collect_used_names_expr(c, out);
            }
            if let Some(u) = update {
                collect_used_names_stmt(u, out);
            }
            collect_used_names_stmts(body, out);
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_used_names_expr(expr, out);
            for case in cases {
                collect_used_names_stmts(&case.body, out);
            }
            collect_used_names_stmts(default, out);
        }
        HirStmt::VaStart { va_list, .. } => collect_used_names_expr(va_list, out),
        _ => {}
    }
}

fn collect_used_names_lvalue(lhs: &HirLValue, out: &mut HashSet<String>) {
    match lhs {
        // Assigned vars still need a declaration while the assign remains.
        HirLValue::Var(n) => {
            out.insert(n.clone());
        }
        HirLValue::Deref { ptr, .. } => collect_used_names_expr(ptr, out),
        HirLValue::Index { base, index, .. } => {
            collect_used_names_expr(base, out);
            collect_used_names_expr(index, out);
        }
        HirLValue::FieldAccess { base, .. } => collect_used_names_expr(base, out),
    }
}

fn collect_used_names_expr(expr: &HirExpr, out: &mut HashSet<String>) {
    match expr {
        HirExpr::Var(n) | HirExpr::AddressOfGlobal(n) => {
            out.insert(n.clone());
        }
        HirExpr::Const(_, _) => {}
        HirExpr::Unary { expr, .. } | HirExpr::Cast { expr, .. } => {
            collect_used_names_expr(expr, out)
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            collect_used_names_expr(lhs, out);
            collect_used_names_expr(rhs, out);
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_used_names_expr(cond, out);
            collect_used_names_expr(then_expr, out);
            collect_used_names_expr(else_expr, out);
        }
        HirExpr::Call { args, .. } => {
            for a in args {
                collect_used_names_expr(a, out);
            }
        }
        HirExpr::Load { ptr, .. }
        | HirExpr::PtrOffset { base: ptr, .. }
        | HirExpr::FieldAccess { base: ptr, .. }
        | HirExpr::AggregateCopy { src: ptr, .. } => collect_used_names_expr(ptr, out),
        HirExpr::Index { base, index, .. } => {
            collect_used_names_expr(base, out);
            collect_used_names_expr(index, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::render_layered_pseudocode;
    use crate::midend::MlilPreviewOptions;

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
            code.contains("return")
                && (code.contains("param_1") || code.contains('0')),
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
        assert_eq!(else_count, 0, "empty then should invert to single if:\n{code}");
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
}
