/// x86 EFLAGS condition-code recovery pass.
///
/// After HIR building, branch conditions that reference raw flag variables
/// (`cf`, `zf`, `sf`, `of`, `pf`) are recovered into high-level comparisons
/// using the 16 x86 Jcc semantics:
///
/// | Jcc   | Raw HIR condition   | Recovered            |
/// |-------|---------------------|----------------------|
/// | JE    | `zf`                | `a == b`             |
/// | JNE   | `!zf`               | `a != b`             |
/// | JB    | `cf`                | `a < b` (unsigned)   |
/// | JAE   | `!cf`               | `a >= b` (unsigned)  |
/// | JBE   | `cf \|\| zf`        | `a <= b` (unsigned)  |
/// | JA    | `!cf && !zf`        | `a > b`  (unsigned)  |
/// | JL    | `sf != of`          | `a < b`  (signed)    |
/// | JGE   | `sf == of`          | `a >= b` (signed)    |
/// | JLE   | `zf \|\| sf != of`  | `a <= b` (signed)    |
/// | JG    | `!zf && sf == of`   | `a > b`  (signed)    |
/// | JS    | `sf`                | `result < 0`         |
/// | JNS   | `!sf`               | `result >= 0`        |
/// | JO    | `of`                | (overflow)           |
/// | JNO   | `!of`               | (!overflow)          |
/// | JP    | `pf`                | (parity)             |
/// | JNP   | `!pf`               | (!parity)            |
///
/// Algorithm:
/// 1. Scan all assignments to flag variables; record definitions for flags with
///    EXACTLY ONE assignment (conservative — skip ambiguous/re-assigned flags).
/// 2. Walk every branch condition in the HIR; pattern-match against the table above.
/// 3. Reconstruct the high-level expression using the flag definitions.
/// 4. Return `true` if any substitution was made (caller re-runs cleanup passes).
use super::super::*;
use std::collections::HashMap;

/// x86 EFLAGS variable names produced by `arch::x86::unique_x86_register_name`.
const FLAG_NAMES: &[&str] = &["cf", "pf", "af", "zf", "sf", "of"];

fn is_flag_var(name: &str) -> bool {
    matches!(name, "cf" | "pf" | "af" | "zf" | "sf" | "of")
}

// ── Phase 1: Definition scan ──────────────────────────────────────────────────

/// Count how many times each flag variable is assigned in the entire body.
fn count_flag_defs(stmts: &[HirStmt], counts: &mut HashMap<String, usize>) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                ..
            } if is_flag_var(name) => {
                *counts.entry(name.clone()).or_insert(0) += 1;
            }
            HirStmt::Block(body) => count_flag_defs(body, counts),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                count_flag_defs(then_body, counts);
                count_flag_defs(else_body, counts);
            }
            HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                count_flag_defs(body, counts)
            }
            HirStmt::For { body, .. } => count_flag_defs(body, counts),
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    count_flag_defs(&case.body, counts);
                }
                count_flag_defs(default, counts);
            }
            _ => {}
        }
    }
}

/// Collect definitions for flags that have exactly ONE assignment in the body.
fn collect_single_defs(stmts: &[HirStmt]) -> HashMap<String, HirExpr> {
    // First pass: count assignments per flag.
    let mut counts: HashMap<String, usize> = HashMap::new();
    count_flag_defs(stmts, &mut counts);

    // Only retain singly-defined flags (conservative correctness).
    let single: std::collections::HashSet<String> = counts
        .into_iter()
        .filter(|(_, c)| *c == 1)
        .map(|(k, _)| k)
        .collect();

    if single.is_empty() {
        return HashMap::new();
    }

    // Second pass: collect the actual definition expressions.
    let mut defs: HashMap<String, HirExpr> = HashMap::new();
    collect_defs_for(stmts, &single, &mut defs);
    defs
}

fn collect_defs_for(
    stmts: &[HirStmt],
    wanted: &std::collections::HashSet<String>,
    defs: &mut HashMap<String, HirExpr>,
) {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                rhs,
            } if wanted.contains(name.as_str()) => {
                defs.entry(name.clone()).or_insert_with(|| rhs.clone());
            }
            HirStmt::Block(body) => collect_defs_for(body, wanted, defs),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                collect_defs_for(then_body, wanted, defs);
                collect_defs_for(else_body, wanted, defs);
            }
            HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                collect_defs_for(body, wanted, defs)
            }
            HirStmt::For { body, .. } => collect_defs_for(body, wanted, defs),
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    collect_defs_for(&case.body, wanted, defs);
                }
                collect_defs_for(default, wanted, defs);
            }
            _ => {}
        }
    }
}

// ── Phase 2: Pattern extraction helpers ──────────────────────────────────────

/// Extract `(a, b)` from `__sborrow(a, b)` or `__scarry(a, b)`.
fn extract_sborrow_args(expr: &HirExpr) -> Option<(HirExpr, HirExpr)> {
    if let HirExpr::Call { target, args, .. } = expr {
        if (target == "__sborrow" || target == "__scarry") && args.len() == 2 {
            return Some((args[0].clone(), args[1].clone()));
        }
    }
    None
}

/// Extract `(a, b)` from `a < b` (unsigned Lt).
fn extract_lt_args(expr: &HirExpr) -> Option<(HirExpr, HirExpr)> {
    if let HirExpr::Binary {
        op: HirBinaryOp::Lt,
        lhs,
        rhs,
        ..
    } = expr
    {
        return Some((*lhs.clone(), *rhs.clone()));
    }
    None
}

/// Extract `(a, b)` from `a == b`.
fn extract_eq_args(expr: &HirExpr) -> Option<(HirExpr, HirExpr)> {
    if let HirExpr::Binary {
        op: HirBinaryOp::Eq,
        lhs,
        rhs,
        ..
    } = expr
    {
        return Some((*lhs.clone(), *rhs.clone()));
    }
    None
}

// ── Phase 3: Condition pattern matching ──────────────────────────────────────

/// Check whether `expr` is `Var(flag)`.
fn is_flag_expr(expr: &HirExpr, flag: &str) -> bool {
    matches!(expr, HirExpr::Var(n) if n == flag)
}

/// Check whether `expr` is `!Var(flag)`.
fn is_not_flag(expr: &HirExpr, flag: &str) -> bool {
    matches!(expr, HirExpr::Unary { op: HirUnaryOp::Not, expr: inner, .. }
             if is_flag_expr(inner, flag))
}

/// Check whether `expr` is `Var(sf) == Var(of)` or `Var(of) == Var(sf)`.
fn is_sf_eq_of(expr: &HirExpr) -> bool {
    matches!(expr,
        HirExpr::Binary { op: HirBinaryOp::Eq, lhs, rhs, .. }
        if (is_flag_expr(lhs, "sf") && is_flag_expr(rhs, "of"))
            || (is_flag_expr(lhs, "of") && is_flag_expr(rhs, "sf")))
}

/// Check whether `expr` is `Var(sf) != Var(of)` or `Var(of) != Var(sf)`.
fn is_sf_ne_of(expr: &HirExpr) -> bool {
    matches!(expr,
        HirExpr::Binary { op: HirBinaryOp::Ne, lhs, rhs, .. }
        if (is_flag_expr(lhs, "sf") && is_flag_expr(rhs, "of"))
            || (is_flag_expr(lhs, "of") && is_flag_expr(rhs, "sf")))
}

/// Helper: build `Binary { op, lhs, rhs, ty: Bool }`.
fn bool_binary(op: HirBinaryOp, lhs: HirExpr, rhs: HirExpr) -> HirExpr {
    HirExpr::Binary {
        op,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
        ty: NirType::Bool,
    }
}

/// Substitute any raw flag `Var` references in `expr` with their definitions.
/// Returns `Some(new_expr)` if any substitution occurred, `None` otherwise.
fn substitute_single_flags(expr: &HirExpr, defs: &HashMap<String, HirExpr>) -> Option<HirExpr> {
    match expr {
        HirExpr::Var(name) if is_flag_var(name) => defs.get(name).cloned(),
        HirExpr::Unary {
            op,
            expr: inner,
            ty,
        } => substitute_single_flags(inner, defs).map(|new_inner| HirExpr::Unary {
            op: *op,
            expr: Box::new(new_inner),
            ty: ty.clone(),
        }),
        HirExpr::Binary { op, lhs, rhs, ty } => {
            let new_lhs = substitute_single_flags(lhs, defs);
            let new_rhs = substitute_single_flags(rhs, defs);
            if new_lhs.is_some() || new_rhs.is_some() {
                Some(HirExpr::Binary {
                    op: *op,
                    lhs: Box::new(new_lhs.unwrap_or_else(|| *lhs.clone())),
                    rhs: Box::new(new_rhs.unwrap_or_else(|| *rhs.clone())),
                    ty: ty.clone(),
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Try to recover a high-level comparison from a condition that references
/// raw x86 flag variables. Returns `Some(recovered)` on success.
pub(super) fn try_recover_flag_condition(
    cond: &HirExpr,
    defs: &HashMap<String, HirExpr>,
) -> Option<HirExpr> {
    // ── JL / JGE: signed SF != OF / SF == OF ──────────────────────────────
    // JL (signed less than): SF != OF → a < b (signed)
    if is_sf_ne_of(cond) {
        if let Some(of_def) = defs.get("of") {
            if let Some((a, b)) = extract_sborrow_args(of_def) {
                return Some(bool_binary(HirBinaryOp::SLt, a, b));
            }
        }
    }
    // JGE (signed greater or equal): SF == OF → a >= b (signed) = !(a < b)
    if is_sf_eq_of(cond) {
        if let Some(of_def) = defs.get("of") {
            if let Some((a, b)) = extract_sborrow_args(of_def) {
                return Some(HirExpr::Unary {
                    op: HirUnaryOp::Not,
                    expr: Box::new(bool_binary(HirBinaryOp::SLt, a, b)),
                    ty: NirType::Bool,
                });
            }
        }
    }

    // ── JLE / JG: zf + sf/of ─────────────────────────────────────────────
    // JLE (signed <=): ZF=1 OR (SF != OF) → a <= b signed
    // Try: LogicalOr(zf, sf_ne_of) or LogicalOr(sf_ne_of, zf)
    if let HirExpr::Binary {
        op: HirBinaryOp::LogicalOr | HirBinaryOp::Or,
        lhs,
        rhs,
        ..
    } = cond
    {
        let (lhs, rhs) = (lhs.as_ref(), rhs.as_ref());
        if (is_flag_expr(lhs, "zf") && is_sf_ne_of(rhs))
            || (is_sf_ne_of(lhs) && is_flag_expr(rhs, "zf"))
        {
            if let Some(of_def) = defs.get("of") {
                if let Some((a, b)) = extract_sborrow_args(of_def) {
                    return Some(bool_binary(HirBinaryOp::SLe, a, b));
                }
            }
        }
        // JBE (unsigned <=): CF=1 OR ZF=1 → a <= b unsigned
        if (is_flag_expr(lhs, "cf") && is_flag_expr(rhs, "zf"))
            || (is_flag_expr(lhs, "zf") && is_flag_expr(rhs, "cf"))
        {
            if let Some(cf_def) = defs.get("cf") {
                if let Some((a, b)) = extract_lt_args(cf_def) {
                    return Some(bool_binary(HirBinaryOp::Le, a, b));
                }
            }
        }
    }

    // JG (signed >): !ZF AND (SF == OF) → a > b signed = b < a
    // Try: LogicalAnd(!zf, sf_eq_of) or LogicalAnd(sf_eq_of, !zf)
    if let HirExpr::Binary {
        op: HirBinaryOp::LogicalAnd | HirBinaryOp::And,
        lhs,
        rhs,
        ..
    } = cond
    {
        let (lhs, rhs) = (lhs.as_ref(), rhs.as_ref());
        if (is_not_flag(lhs, "zf") && is_sf_eq_of(rhs))
            || (is_sf_eq_of(lhs) && is_not_flag(rhs, "zf"))
        {
            if let Some(of_def) = defs.get("of") {
                if let Some((a, b)) = extract_sborrow_args(of_def) {
                    // a > b signed = b < a signed
                    return Some(bool_binary(HirBinaryOp::SLt, b, a));
                }
            }
        }
        // JA (unsigned >): !CF AND !ZF → a > b unsigned = b < a
        if (is_not_flag(lhs, "cf") && is_not_flag(rhs, "zf"))
            || (is_not_flag(lhs, "zf") && is_not_flag(rhs, "cf"))
        {
            if let Some(cf_def) = defs.get("cf") {
                if let Some((a, b)) = extract_lt_args(cf_def) {
                    // a > b unsigned = b < a
                    return Some(bool_binary(HirBinaryOp::Lt, b, a));
                }
            }
        }
    }

    // ── Single-flag substitution ──────────────────────────────────────────
    // For any remaining flag var references, substitute definitions directly.
    // The existing normalizer will further simplify (e.g. !(a==b) → a!=b).
    substitute_single_flags(cond, defs)
}

// ── Phase 4: Walk statements ──────────────────────────────────────────────────

fn recover_in_cond(cond: &mut HirExpr, defs: &HashMap<String, HirExpr>, changed: &mut bool) {
    if let Some(recovered) = try_recover_flag_condition(cond, defs) {
        *cond = recovered;
        *changed = true;
        // Re-normalize the substituted expression.
        super::super::pipeline::normalize_expr(cond);
    }
}

fn recover_in_stmts_box(
    stmt: &mut Box<HirStmt>,
    defs: &HashMap<String, HirExpr>,
    changed: &mut bool,
) {
    let mut tmp = vec![*stmt.clone()];
    recover_in_stmts(&mut tmp, defs, changed);
    if let Some(s) = tmp.into_iter().next() {
        **stmt = s;
    }
}

fn recover_in_stmts(stmts: &mut Vec<HirStmt>, defs: &HashMap<String, HirExpr>, changed: &mut bool) {
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                recover_in_cond(cond, defs, changed);
                recover_in_stmts(then_body, defs, changed);
                recover_in_stmts(else_body, defs, changed);
            }
            HirStmt::While { cond, body } => {
                recover_in_cond(cond, defs, changed);
                recover_in_stmts(body, defs, changed);
            }
            HirStmt::DoWhile { body, cond } => {
                recover_in_stmts(body, defs, changed);
                recover_in_cond(cond, defs, changed);
            }
            HirStmt::For {
                cond,
                body,
                init,
                update,
                ..
            } => {
                if let Some(c) = cond {
                    recover_in_cond(c, defs, changed);
                }
                if let Some(i) = init {
                    recover_in_stmts_box(i, defs, changed);
                }
                if let Some(u) = update {
                    recover_in_stmts_box(u, defs, changed);
                }
                recover_in_stmts(body, defs, changed);
            }
            HirStmt::Block(body) => recover_in_stmts(body, defs, changed),
            HirStmt::Switch { cases, default, .. } => {
                for case in cases.iter_mut() {
                    recover_in_stmts(&mut case.body, defs, changed);
                }
                recover_in_stmts(default, defs, changed);
            }
            _ => {}
        }
    }
}

// ── Dead flag assignment elimination ─────────────────────────────────────────

/// Remove assignments to x86 flag variables that have zero rvalue uses in the
/// function body.  Unlike `defuse_dead_assignment_pass`, this also handles
/// non-Temp-origin bindings (flag variables are named registers, not temps).
fn remove_dead_flag_assigns(func: &mut HirFunction) {
    // Build a use-count map for the whole body.
    let mut uses: HashMap<String, usize> = HashMap::new();
    count_uses_in_stmts(&func.body, &mut uses);

    // Remove top-level and nested assignments to flag variables with 0 uses.
    let mut dummy = false;
    remove_dead_flags_in_stmts(&mut func.body, &uses, &mut dummy);

    // Prune flag variable bindings that are now unreferenced.
    func.locals
        .retain(|b| !is_flag_var(&b.name) || uses.get(&b.name).copied().unwrap_or(0) > 0);
}

fn count_uses_in_stmts(stmts: &[HirStmt], uses: &mut HashMap<String, usize>) {
    for stmt in stmts {
        count_uses_in_stmt(stmt, uses);
    }
}

fn count_uses_in_stmt(stmt: &HirStmt, uses: &mut HashMap<String, usize>) {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            // LHS write is NOT a use; only ptr/index sub-expressions are uses.
            match lhs {
                HirLValue::Deref { ptr, .. } => count_uses_in_expr(ptr, uses),
                HirLValue::Index { base, index, .. } => {
                    count_uses_in_expr(base, uses);
                    count_uses_in_expr(index, uses);
                }
                HirLValue::Var(_) => {}
            }
            count_uses_in_expr(rhs, uses);
        }
        HirStmt::Expr(e) | HirStmt::Return(Some(e)) => count_uses_in_expr(e, uses),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            count_uses_in_expr(cond, uses);
            count_uses_in_stmts(then_body, uses);
            count_uses_in_stmts(else_body, uses);
        }
        HirStmt::While { cond, body } => {
            count_uses_in_expr(cond, uses);
            count_uses_in_stmts(body, uses);
        }
        HirStmt::DoWhile { body, cond } => {
            count_uses_in_stmts(body, uses);
            count_uses_in_expr(cond, uses);
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                count_uses_in_stmt(i, uses);
            }
            if let Some(c) = cond {
                count_uses_in_expr(c, uses);
            }
            if let Some(u) = update {
                count_uses_in_stmt(u, uses);
            }
            count_uses_in_stmts(body, uses);
        }
        HirStmt::Block(body) => count_uses_in_stmts(body, uses),
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            count_uses_in_expr(expr, uses);
            for case in cases {
                count_uses_in_stmts(&case.body, uses);
            }
            count_uses_in_stmts(default, uses);
        }
        _ => {}
    }
}

fn count_uses_in_expr(expr: &HirExpr, uses: &mut HashMap<String, usize>) {
    match expr {
        HirExpr::Var(name) | HirExpr::AddressOfGlobal(name) => {
            *uses.entry(name.clone()).or_default() += 1;
        }
        HirExpr::Const(_, _) => {}
        HirExpr::Cast { expr: inner, .. }
        | HirExpr::Unary { expr: inner, .. }
        | HirExpr::Load { ptr: inner, .. }
        | HirExpr::PtrOffset { base: inner, .. }
        | HirExpr::AggregateCopy { src: inner, .. } => count_uses_in_expr(inner, uses),
        HirExpr::Binary { lhs, rhs, .. } => {
            count_uses_in_expr(lhs, uses);
            count_uses_in_expr(rhs, uses);
        }
        HirExpr::Call { args, .. } => {
            for a in args {
                count_uses_in_expr(a, uses);
            }
        }
        HirExpr::Index { base, index, .. } => {
            count_uses_in_expr(base, uses);
            count_uses_in_expr(index, uses);
        }
    }
}

fn remove_dead_flags_in_stmts(
    stmts: &mut Vec<HirStmt>,
    uses: &HashMap<String, usize>,
    changed: &mut bool,
) {
    // Recurse into nested bodies first.
    for stmt in stmts.iter_mut() {
        remove_dead_flags_in_nested(stmt, uses, changed);
    }
    // Then remove flat-level dead flag assignments.
    stmts.retain(|stmt| {
        if let HirStmt::Assign {
            lhs: HirLValue::Var(name),
            rhs,
        } = stmt
        {
            if is_flag_var(name) && uses.get(name.as_str()).copied().unwrap_or(0) == 0 {
                // Only remove if the RHS has no side effects.
                if !expr_has_flag_side_effects(rhs) {
                    *changed = true;
                    return false;
                }
            }
        }
        true
    });
}

fn remove_dead_flags_in_nested(
    stmt: &mut HirStmt,
    uses: &HashMap<String, usize>,
    changed: &mut bool,
) {
    match stmt {
        HirStmt::Block(body) => remove_dead_flags_in_stmts(body, uses, changed),
        HirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            remove_dead_flags_in_stmts(then_body, uses, changed);
            remove_dead_flags_in_stmts(else_body, uses, changed);
        }
        HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
            remove_dead_flags_in_stmts(body, uses, changed);
        }
        HirStmt::For {
            init, update, body, ..
        } => {
            if let Some(i) = init {
                remove_dead_flags_in_nested(i, uses, changed);
            }
            if let Some(u) = update {
                remove_dead_flags_in_nested(u, uses, changed);
            }
            remove_dead_flags_in_stmts(body, uses, changed);
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases.iter_mut() {
                remove_dead_flags_in_stmts(&mut case.body, uses, changed);
            }
            remove_dead_flags_in_stmts(default, uses, changed);
        }
        _ => {}
    }
}

/// Conservative side-effect check for flag assignment RHS.
/// We allow removal if the RHS is a comparison, call to __sborrow/__scarry/__carry,
/// or a simple arithmetic expression with no observable side effects.
fn expr_has_flag_side_effects(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => false,
        HirExpr::Cast { expr: inner, .. } | HirExpr::Unary { expr: inner, .. } => {
            expr_has_flag_side_effects(inner)
        }
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_has_flag_side_effects(lhs) || expr_has_flag_side_effects(rhs)
        }
        HirExpr::Call { target, args, .. } => {
            if matches!(
                target.as_str(),
                "__sborrow" | "__scarry" | "__carry" | "__popcount"
            ) {
                args.iter().any(expr_has_flag_side_effects)
            } else {
                true // unknown calls have side effects
            }
        }
        // Loads, pointer arithmetic, etc. — conservative
        _ => true,
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Apply the x86 EFLAGS condition-code recovery pass to `func`.
///
/// Returns `true` if any branch condition was rewritten; the caller should
/// follow up with `defuse_dead_assignment_pass` to remove now-dead flag
/// assignments.
pub(crate) fn apply_flag_recovery_pass(func: &mut HirFunction) -> bool {
    let defs = collect_single_defs(&func.body);
    if defs.is_empty() {
        return false;
    }
    let mut changed = false;
    recover_in_stmts(&mut func.body, &defs, &mut changed);
    if changed {
        // Remove assignments to flag variables that are now dead after recovery.
        remove_dead_flag_assigns(func);
    }
    changed
}

// ── Helpers used by other passes ─────────────────────────────────────────────

/// Returns `true` if `name` is an x86 flag variable name.
pub(super) fn is_x86_flag_variable(name: &str) -> bool {
    is_flag_var(name)
}

/// Returns the set of all x86 flag variable names.
pub(super) fn x86_flag_names() -> &'static [&'static str] {
    FLAG_NAMES
}
