//! HIR presentation pass — readability-only tree polish before HIR print.
//!
//! Must not change control-flow structure or expression meaning beyond
//! presentation (alias/temp folding for printing, unused local drop, sugar).
//! Semantic recovery stays in normalize/; NIR print uses the pre-presentation tree.

use super::super::*;
use std::collections::{HashMap, HashSet};

/// Apply HIR-facing presentation polish in place.
pub(crate) fn apply_hir_presentation(func: &mut HirFunction) {
    for _ in 0..8 {
        let mut changed = false;
        changed |= flatten_redundant_blocks(&mut func.body);
        changed |= propagate_pure_var_aliases(func);
        changed |= fold_self_update_after_seed(&mut func.body);
        changed |= collapse_trivial_assign_returns(&mut func.body);
        changed |= inline_single_use_pure_assigns(func);
        changed |= eliminate_pure_dead_assigns(func);
        if !changed {
            break;
        }
    }
    drop_unused_presentation_locals(func);
}

// ── Pure expression helpers ──────────────────────────────────────────────────

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
        // Loads, calls, aggregate copies, and field/index may alias memory —
        // keep them out of presentation inlining/propagation.
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

/// Collapse `x = pure; return x` → `return pure`.
fn collapse_trivial_assign_returns(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i + 1 < stmts.len() {
        let replacement = match (&stmts[i], &stmts[i + 1]) {
            (
                HirStmt::Assign {
                    lhs: HirLValue::Var(name),
                    rhs,
                },
                HirStmt::Return(Some(HirExpr::Var(ret))),
            ) if name == ret && expr_is_presentation_pure(rhs) => Some(rhs.clone()),
            _ => None,
        };
        if let Some(expr) = replacement {
            stmts[i + 1] = HirStmt::Return(Some(expr));
            stmts.remove(i);
            changed = true;
            continue;
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
            if found.is_some() || uses != 1 {
                return None;
            }
            // Only inline into simple consumers at this presentation layer.
            if !matches!(
                stmt,
                HirStmt::Assign { .. } | HirStmt::Return(_) | HirStmt::Expr(_)
            ) {
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

fn eliminate_pure_dead_assigns(func: &mut HirFunction) -> bool {
    let formal: HashSet<&str> = func.params.iter().map(|b| b.name.as_str()).collect();
    eliminate_pure_dead_in_stmts(&mut func.body, &formal)
}

fn eliminate_pure_dead_in_stmts(stmts: &mut Vec<HirStmt>, formal: &HashSet<&str>) -> bool {
    // Recompute uses on this subtree + siblings via whole list.
    let names: Vec<String> = {
        let mut defs = HashMap::new();
        count_defs_in_stmts(stmts, &mut defs);
        defs.into_keys().collect()
    };
    let use_counts: HashMap<String, usize> = names
        .iter()
        .map(|n| (n.clone(), count_uses_in_stmts(stmts, n)))
        .collect();

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
                changed |= eliminate_pure_dead_in_stmts(body, formal);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= eliminate_pure_dead_in_stmts(then_body, formal);
                changed |= eliminate_pure_dead_in_stmts(else_body, formal);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init_stmt) = init {
                    if let HirStmt::Block(b) = init_stmt.as_mut() {
                        changed |= eliminate_pure_dead_in_stmts(b, formal);
                    }
                }
                if let Some(upd) = update {
                    if let HirStmt::Block(b) = upd.as_mut() {
                        changed |= eliminate_pure_dead_in_stmts(b, formal);
                    }
                }
                changed |= eliminate_pure_dead_in_stmts(body, formal);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= eliminate_pure_dead_in_stmts(&mut case.body, formal);
                }
                changed |= eliminate_pure_dead_in_stmts(default, formal);
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
    use crate::nir::render::pipeline::render_layered_pseudocode;
    use crate::nir::MlilPreviewOptions;

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
        let code = crate::nir::print_hir_function(&func);
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
}
