//! Single-use pure-assignment inlining and trivial assign/return collapse.

use super::*;

/// Collapse `x = rhs; return x` → `return rhs` (labels between are skipped).
/// Allows call/select RHS: single evaluation is preserved.
/// `x = e; return x;` -> `return e;`. Not for a global: assigning one is a
/// store the program makes, and `g = g - 1; return g;` folded to
/// `return g - 1;` lost it.
pub(super) fn collapse_trivial_assign_returns(
    stmts: &mut Vec<HirStmt>,
    globals: &HashSet<String>,
) -> bool {
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
        if globals.contains(name) {
            i += 1;
            continue;
        }
        let name = name.clone();
        let rhs = rhs.clone();
        // Do not fold across intermediate labels (a branch target may enter
        // with a different value in `name`).
        let j = i + 1;
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
                changed |= collapse_trivial_assign_returns(body, globals);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= collapse_trivial_assign_returns(then_body, globals);
                changed |= collapse_trivial_assign_returns(else_body, globals);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init_stmt) = init {
                    if let HirStmt::Block(b) = init_stmt.as_mut() {
                        changed |= collapse_trivial_assign_returns(b, globals);
                    }
                }
                if let Some(upd) = update {
                    if let HirStmt::Block(b) = upd.as_mut() {
                        changed |= collapse_trivial_assign_returns(b, globals);
                    }
                }
                changed |= collapse_trivial_assign_returns(body, globals);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= collapse_trivial_assign_returns(&mut case.body, globals);
                }
                changed |= collapse_trivial_assign_returns(default, globals);
            }
            _ => {}
        }
    }
    changed
}

/// Inline single-def, single-use pure assigns into their use site (temps + noise).
///
/// A global is memory, not a temporary: an assignment to one is a store this
/// function makes, and a read of one is a load another function (a callee)
/// can change. So `g = 5; f(); return g;` keeps both the store and the read.
pub(super) fn inline_single_use_pure_assigns(
    func: &mut HirFunction,
    globals: &HashSet<String>,
) -> bool {
    let formal: HashSet<&str> = func.params.iter().map(|b| b.name.as_str()).collect();
    let mut def_counts = HashMap::new();
    count_defs_in_stmts(&func.body, &mut def_counts);
    inline_single_use_in_stmts(&mut func.body, &formal, &def_counts, globals)
}

pub(super) fn inline_single_use_in_stmts(
    stmts: &mut Vec<HirStmt>,
    formal: &HashSet<&str>,
    def_counts: &HashMap<String, usize>,
    globals: &HashSet<String>,
) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        let candidate = match &stmts[i] {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs,
            } if !formal.contains(name.as_str())
                && !globals.contains(name.as_str())
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
                    let reads_memory = !expr_is_presentation_pure(&rhs)
                        || globals.iter().any(|global| expr_mentions_var(&rhs, global));
                    if reads_memory && memory_clobbered_between(stmts, i + 1, target) {
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
                changed |= inline_single_use_in_stmts(body, formal, def_counts, globals);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= inline_single_use_in_stmts(then_body, formal, def_counts, globals);
                changed |= inline_single_use_in_stmts(else_body, formal, def_counts, globals);
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(init_stmt) = init {
                    if let HirStmt::Block(b) = init_stmt.as_mut() {
                        changed |= inline_single_use_in_stmts(b, formal, def_counts, globals);
                    }
                }
                if let Some(upd) = update {
                    if let HirStmt::Block(b) = upd.as_mut() {
                        changed |= inline_single_use_in_stmts(b, formal, def_counts, globals);
                    }
                }
                changed |= inline_single_use_in_stmts(body, formal, def_counts, globals);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |=
                        inline_single_use_in_stmts(&mut case.body, formal, def_counts, globals);
                }
                changed |= inline_single_use_in_stmts(default, formal, def_counts, globals);
            }
            _ => {}
        }
    }
    changed
}

/// True if a free variable of an expression is assigned before its use.
///
/// A condition on a loop is evaluated again after the loop body, so a write in
/// the target loop body also counts as "before" later uses.  Treating only
/// top-level assignments as redefinitions would incorrectly move an entry
/// snapshot such as `end = data + length` into `data != end` after `data` is
/// advanced by the body.
pub(super) fn pure_expr_free_var_redefined_before(
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
        if free
            .iter()
            .any(|name| assigns_var_name(stmt, name.as_str()))
        {
            return true;
        }
    }
    stmts.get(end).is_some_and(|stmt| {
        let body = match stmt {
            HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => Some(body),
            HirStmt::For { body, update, .. } => {
                if free.iter().any(|name| {
                    update
                        .as_ref()
                        .is_some_and(|s| assigns_var_name(s, name.as_str()))
                }) {
                    return true;
                }
                Some(body)
            }
            _ => None,
        };
        body.is_some_and(|body| {
            free.iter()
                .any(|name| body.iter().any(|s| assigns_var_name(s, name.as_str())))
        })
    })
}

pub(super) fn collect_free_vars_in_expr(expr: &HirExpr, out: &mut HashSet<String>) {
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

pub(super) fn find_single_use_target(stmts: &[HirStmt], start: usize, name: &str) -> Option<usize> {
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
pub(super) fn pure_var_assign(stmt: &HirStmt) -> Option<(&str, &HirExpr)> {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } if expr_is_presentation_pure(rhs) => Some((name.as_str(), rhs)),
        _ => None,
    }
}
