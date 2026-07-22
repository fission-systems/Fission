//! Sparse conditional constant propagation (SCCP) on structured HIR.
//!
//! Tracks a lattice of `Var → (i64, NirType)` along straight-line flow, merges at
//! `if`/`switch` joins, and conservatively drops variables written in loop bodies
//! from the map after `while`/`for`/`do-while`.  This complements
//! [`super::super::analysis::defuse::constant_folding_pass`] (single-statement fold) and VSA
//! [`fission_midend_dir::vsa::jump_resolver`] (intervals, not a constant lattice).

use super::super::analysis::defuse::{eval_hir_expr_with_const_env, fold_expr_hir};
use super::super::cleanup::expr_has_side_effects;
use super::super::pipeline::is_large_hir_function;
use crate::prelude::*;
use crate::{HashMap, HashSet};

type ConstEnv = HashMap<String, (i64, NirType)>;

pub fn apply_sccp_pass(func: &mut DirFunction) -> bool {
    let max_rounds = if is_large_hir_function(func) { 2 } else { 8 };
    let goto_targets = collect_goto_targets(&func.body);
    let mut all_xvars = HashSet::default();
    collect_xvars_in_stmts(&func.body, &mut all_xvars);
    let mut any = false;
    for _ in 0..max_rounds {
        let mut env = ConstEnv::default();
        if !sccp_transform_stmts(&mut func.body, &mut env, &goto_targets, &all_xvars) {
            break;
        }
        any = true;
    }
    any
}

fn collect_goto_targets(stmts: &[DirStmt]) -> HashSet<String> {
    let mut targets = HashSet::default();
    for stmt in stmts {
        collect_goto_targets_stmt(stmt, &mut targets);
    }
    targets
}

fn collect_goto_targets_stmt(stmt: &DirStmt, targets: &mut HashSet<String>) {
    match stmt {
        DirStmt::Goto(label) => {
            targets.insert(label.clone());
        }
        DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            for s in body {
                collect_goto_targets_stmt(s, targets);
            }
        }
        DirStmt::For {
            init, update, body, ..
        } => {
            if let Some(s) = init.as_deref() {
                collect_goto_targets_stmt(s, targets);
            }
            for s in body {
                collect_goto_targets_stmt(s, targets);
            }
            if let Some(s) = update.as_deref() {
                collect_goto_targets_stmt(s, targets);
            }
        }
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                collect_goto_targets_stmt(s, targets);
            }
            for s in else_body {
                collect_goto_targets_stmt(s, targets);
            }
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in &case.body {
                    collect_goto_targets_stmt(s, targets);
                }
            }
            for s in default {
                collect_goto_targets_stmt(s, targets);
            }
        }
        _ => {}
    }
}

fn merge_env(a: &ConstEnv, b: &ConstEnv) -> ConstEnv {
    let keys: HashSet<_> = a.keys().chain(b.keys()).cloned().collect();
    let mut out = ConstEnv::default();
    for k in keys {
        match (a.get(&k), b.get(&k)) {
            (Some(ca), Some(cb)) if ca == cb => {
                out.insert(k, ca.clone());
            }
            _ => {}
        }
    }
    out
}

fn env_without_vars(env: &ConstEnv, vars: &HashSet<String>) -> ConstEnv {
    let mut out = env.clone();
    for var in vars {
        out.remove(var);
    }
    out
}

fn stmt_list_has_side_effects(stmts: &[DirStmt]) -> bool {
    stmts.iter().any(stmt_has_side_effects)
}

fn stmt_has_side_effects(stmt: &DirStmt) -> bool {
    match stmt {
        DirStmt::Assign { lhs, rhs } => lvalue_has_side_effects(lhs) || expr_has_side_effects(rhs),
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => expr_has_side_effects(expr),
        DirStmt::Block(body) => stmt_list_has_side_effects(body),
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => stmt_list_has_side_effects(then_body) || stmt_list_has_side_effects(else_body),
        DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } | DirStmt::For { body, .. } => {
            stmt_list_has_side_effects(body)
        }
        DirStmt::Switch { cases, default, .. } => {
            cases.iter().any(|c| stmt_list_has_side_effects(&c.body))
                || stmt_list_has_side_effects(default)
        }
        DirStmt::VaStart { .. } => true,
        _ => false,
    }
}

fn lvalue_has_side_effects(lhs: &DirLValue) -> bool {
    match lhs {
        DirLValue::Var(_) => false,
        DirLValue::Deref { ptr, .. } => expr_has_side_effects(ptr),
        DirLValue::Index { base, index, .. } => {
            expr_has_side_effects(base) || expr_has_side_effects(index)
        }
        DirLValue::FieldAccess { base, .. } => expr_has_side_effects(base),
    }
}

fn loop_variant_vars(body: &[DirStmt], all_xvars: &HashSet<String>) -> HashSet<String> {
    let mut vars = HashSet::default();
    for stmt in body {
        loop_variant_stmt(stmt, &mut vars);
    }
    for xvar in all_xvars {
        vars.insert(xvar.clone());
    }
    vars
}

fn loop_variant_stmt(stmt: &DirStmt, out: &mut HashSet<String>) {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(name),
            ..
        } => {
            out.insert(name.clone());
        }
        DirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body {
                loop_variant_stmt(s, out);
            }
            for s in else_body {
                loop_variant_stmt(s, out);
            }
        }
        DirStmt::Block(body) => {
            for s in body {
                loop_variant_stmt(s, out);
            }
        }
        DirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in &case.body {
                    loop_variant_stmt(s, out);
                }
            }
            for s in default {
                loop_variant_stmt(s, out);
            }
        }
        DirStmt::While { .. } | DirStmt::DoWhile { .. } | DirStmt::For { .. } => {}
        _ => {}
    }
}

fn sccp_subst_expr(expr: &mut DirExpr, env: &ConstEnv) -> bool {
    let mut changed = false;
    match expr {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
            if let Some((v, ty)) = env.get(name) {
                *expr = DirExpr::Const(*v, ty.clone());
                changed = true;
            }
        }
        DirExpr::Unary { expr: inner, .. } => changed |= sccp_subst_expr(inner, env),
        DirExpr::Binary { lhs, rhs, .. } => {
            changed |= sccp_subst_expr(lhs, env);
            changed |= sccp_subst_expr(rhs, env);
        }
        DirExpr::Cast { expr: inner, .. } => changed |= sccp_subst_expr(inner, env),
        DirExpr::Load { ptr, .. } => changed |= sccp_subst_expr(ptr, env),
        DirExpr::PtrOffset { base, .. } => changed |= sccp_subst_expr(base, env),
        DirExpr::FieldAccess { base, .. } => changed |= sccp_subst_expr(base, env),
        DirExpr::Index { base, index, .. } => {
            changed |= sccp_subst_expr(base, env);
            changed |= sccp_subst_expr(index, env);
        }
        DirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                changed |= sccp_subst_expr(a, env);
            }
        }
        DirExpr::AggregateCopy { src, .. } => changed |= sccp_subst_expr(src, env),
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= sccp_subst_expr(cond, env);
            changed |= sccp_subst_expr(then_expr, env);
            changed |= sccp_subst_expr(else_expr, env);
        }
        DirExpr::Const(_, _) => {}
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
// prelude via parent

    fn int(bits: u32) -> NirType {
        NirType::Int {
            bits,
            signed: false,
        }
    }

    fn var(name: &str) -> DirExpr {
        DirExpr::Var(name.to_string())
    }

    #[test]
    fn sccp_keeps_backedge_label_values_nonconstant() {
        let mut func = DirFunction {
            name: "test_sccp_unstructured_backedge".to_string(),
            int_param_offsets: Vec::new(),
            return_type: int(32),
            body: vec![
                DirStmt::Assign {
                    lhs: DirLValue::Var("x".to_string()),
                    rhs: DirExpr::Const(0, int(32)),
                },
                DirStmt::Label("loop".to_string()),
                DirStmt::Assign {
                    lhs: DirLValue::Var("x".to_string()),
                    rhs: DirExpr::Binary {
                        op: DirBinaryOp::Add,
                        lhs: Box::new(var("x")),
                        rhs: Box::new(DirExpr::Const(1, int(32))),
                        ty: int(32),
                    },
                },
                DirStmt::If {
                    cond: DirExpr::Binary {
                        op: DirBinaryOp::Sub,
                        lhs: Box::new(var("rows")),
                        rhs: Box::new(var("x")),
                        ty: int(32),
                    },
                    then_body: vec![DirStmt::Goto("loop".to_string())],
                    else_body: vec![],
                },
            ],
            ..Default::default()
        };

        apply_sccp_pass(&mut func);

        let DirStmt::If { cond, .. } = &func.body[3] else {
            panic!("expected loop branch to remain an if");
        };
        let DirExpr::Binary { rhs, .. } = cond else {
            panic!("expected branch condition to remain binary");
        };
        assert_eq!(rhs.as_ref(), &var("x"));
    }

    #[test]
    fn sccp_does_not_prune_if_when_discarded_branch_has_side_effects() {
        let mut func = DirFunction {
            name: "test".to_string(),
            int_param_offsets: Vec::new(),
            return_type: int(32),
            body: vec![DirStmt::If {
                cond: DirExpr::Const(1, int(32)),
                then_body: vec![DirStmt::Expr(DirExpr::Call {
                    target: "add".to_string(),
                    args: vec![DirExpr::Const(1, int(32)), DirExpr::Const(2, int(32))],
                    ty: int(32),
                })],
                else_body: vec![DirStmt::Expr(DirExpr::Call {
                    target: "max".to_string(),
                    args: vec![DirExpr::Const(3, int(32)), DirExpr::Const(4, int(32))],
                    ty: int(32),
                })],
            }],
            ..Default::default()
        };

        apply_sccp_pass(&mut func);

        let DirStmt::If {
            then_body,
            else_body,
            ..
        } = &func.body[0]
        else {
            panic!("side-effectful branch should not be pruned");
        };
        assert_eq!(then_body.len(), 1);
        assert_eq!(else_body.len(), 1);
    }
}

fn eval_truth(expr: &DirExpr, env: &ConstEnv) -> Option<bool> {
    let (v, _) = eval_hir_expr_with_const_env(expr, env)?;
    Some(v != 0)
}

/// Ghidra ActionConditionalConst analog: given a branch condition, derive
/// constant bindings that are known to hold in the then/else branches.
///
/// Returns `(then_bindings, else_bindings)` where each binding is `(name, value, ty)`.
///
/// Handles:
/// - `x == K`  → then: x=K
/// - `x != K`  → else: x=K
/// - `!(x == K)` → else: x=K (same as x != K)
/// - `cond1 && cond2` → then: union of both
fn derive_branch_constants(
    cond: &DirExpr,
) -> (Vec<(String, i64, NirType)>, Vec<(String, i64, NirType)>) {
    let mut then_bindings: Vec<(String, i64, NirType)> = Vec::new();
    let mut else_bindings: Vec<(String, i64, NirType)> = Vec::new();
    extract_branch_constants(cond, false, &mut then_bindings, &mut else_bindings);
    (then_bindings, else_bindings)
}

fn extract_branch_constants(
    cond: &DirExpr,
    negated: bool,
    then_bindings: &mut Vec<(String, i64, NirType)>,
    else_bindings: &mut Vec<(String, i64, NirType)>,
) {
    match cond {
        // NOT: flip then/else roles.
        DirExpr::Unary {
            op: DirUnaryOp::Not,
            expr: inner,
            ..
        } => {
            extract_branch_constants(inner, !negated, then_bindings, else_bindings);
        }
        // x == K or K == x  → then: x=K ; x != K or K != x → else: x=K
        DirExpr::Binary {
            op: op @ (DirBinaryOp::Eq | DirBinaryOp::Ne),
            lhs,
            rhs,
            ..
        } => {
            let (var_name, const_val, ty) = match (lhs.as_ref(), rhs.as_ref()) {
                (DirExpr::Var(name), DirExpr::Const(k, ty)) => (name.clone(), *k, ty.clone()),
                (DirExpr::Const(k, ty), DirExpr::Var(name)) => (name.clone(), *k, ty.clone()),
                _ => return,
            };
            // For `==`: const holds in then-branch (unless negated → else-branch).
            // For `!=`: const holds in else-branch.
            let const_in_then = matches!(op, DirBinaryOp::Eq) ^ negated;
            if const_in_then {
                then_bindings.push((var_name, const_val, ty));
            } else {
                else_bindings.push((var_name, const_val, ty));
            }
        }
        // cond_a && cond_b → then: both hold; else: nothing (either could be false).
        DirExpr::Binary {
            op: DirBinaryOp::And,
            lhs,
            rhs,
            ..
        } if !negated => {
            extract_branch_constants(lhs, false, then_bindings, else_bindings);
            extract_branch_constants(rhs, false, then_bindings, else_bindings);
        }
        _ => {}
    }
}

fn sccp_transform_stmts(
    stmts: &mut Vec<DirStmt>,
    env: &mut ConstEnv,
    goto_targets: &HashSet<String>,
    all_xvars: &HashSet<String>,
) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        changed |= sccp_stmt(&mut stmts[i], env, goto_targets, all_xvars);
        i += 1;
    }
    changed
}

fn sccp_stmt(
    stmt: &mut DirStmt,
    env: &mut ConstEnv,
    goto_targets: &HashSet<String>,
    all_xvars: &HashSet<String>,
) -> bool {
    let mut changed = false;
    loop {
        match stmt {
            DirStmt::Assign { lhs, rhs } => {
                if let DirLValue::Var(name) = lhs {
                    changed |= sccp_subst_expr(rhs, env);
                    changed |= fold_expr_hir(rhs);
                    if let Some((v, ty)) = eval_hir_expr_with_const_env(rhs, env) {
                        if !matches!(rhs, DirExpr::Const(cv, _) if *cv == v) {
                            *rhs = DirExpr::Const(v, ty.clone());
                            changed = true;
                        }
                        env.insert(name.clone(), (v, ty));
                    } else {
                        env.remove(name);
                    }
                } else {
                    changed |= sccp_subst_expr(rhs, env);
                    changed |= fold_expr_hir(rhs);
                }
                break;
            }
            DirStmt::VaStart { va_list, .. } => {
                changed |= sccp_subst_expr(va_list, env);
                changed |= fold_expr_hir(va_list);
                break;
            }
            DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
                changed |= sccp_subst_expr(expr, env);
                changed |= fold_expr_hir(expr);
                break;
            }
            DirStmt::Block(stmts) => {
                changed |= sccp_transform_stmts(stmts, env, goto_targets, all_xvars);
                break;
            }
            DirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let pre = env.clone();
                changed |= sccp_subst_expr(cond, &pre);
                changed |= fold_expr_hir(cond);
                match eval_truth(cond, &pre) {
                    Some(true) if !stmt_list_has_side_effects(else_body) => {
                        *stmt = DirStmt::Block(std::mem::take(then_body));
                        changed = true;
                        continue;
                    }
                    Some(false) if !stmt_list_has_side_effects(then_body) => {
                        *stmt = DirStmt::Block(std::mem::take(else_body));
                        changed = true;
                        continue;
                    }
                    Some(_) | None => {
                        // Ghidra ActionConditionalConst: derive constants from the branch condition.
                        // Pattern: `if (x == K)` → inside then-branch, x=K
                        // Pattern: `if (x != K)` → inside else-branch, x=K
                        let (then_extra, else_extra) = derive_branch_constants(cond);
                        let mut e1 = pre.clone();
                        let mut e2 = pre.clone();
                        for (name, val, ty) in then_extra {
                            e1.insert(name, (val, ty));
                        }
                        for (name, val, ty) in else_extra {
                            e2.insert(name, (val, ty));
                        }
                        changed |=
                            sccp_transform_stmts(then_body, &mut e1, goto_targets, all_xvars);
                        changed |=
                            sccp_transform_stmts(else_body, &mut e2, goto_targets, all_xvars);
                        *env = merge_env(&e1, &e2);
                    }
                }
                break;
            }
            DirStmt::While { cond, body } => {
                let pre = env.clone();
                let modified = loop_variant_vars(body, all_xvars);
                let loop_entry = env_without_vars(&pre, &modified);
                changed |= sccp_subst_expr(cond, &loop_entry);
                changed |= fold_expr_hir(cond);
                let mut inner = loop_entry;
                changed |= sccp_transform_stmts(body, &mut inner, goto_targets, all_xvars);
                *env = env_without_vars(&pre, &modified);
                break;
            }
            DirStmt::DoWhile { body, cond } => {
                let pre = env.clone();
                let modified = loop_variant_vars(body, all_xvars);
                let mut inner = env_without_vars(&pre, &modified);
                changed |= sccp_transform_stmts(body, &mut inner, goto_targets, all_xvars);
                let cond_env = env_without_vars(&inner, &modified);
                changed |= sccp_subst_expr(cond, &cond_env);
                changed |= fold_expr_hir(cond);
                *env = env_without_vars(&pre, &modified);
                break;
            }
            DirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(i) = init.as_mut() {
                    changed |= sccp_stmt(i, env, goto_targets, all_xvars);
                }
                let loop_entry = env.clone();
                let mut modified = loop_variant_vars(body, all_xvars);
                if let Some(u) = update {
                    if let DirStmt::Assign {
                        lhs: DirLValue::Var(n),
                        ..
                    } = u.as_ref()
                    {
                        modified.insert(n.clone());
                    }
                }
                let loop_body_entry = env_without_vars(&loop_entry, &modified);
                if let Some(c) = cond.as_mut() {
                    changed |= sccp_subst_expr(c, &loop_body_entry);
                    changed |= fold_expr_hir(c);
                }
                let mut inner = loop_body_entry;
                changed |= sccp_transform_stmts(body, &mut inner, goto_targets, all_xvars);
                *env = env_without_vars(&loop_entry, &modified);
                if let Some(u) = update.as_mut() {
                    let mut update_env = env_without_vars(&inner, &modified);
                    changed |= sccp_stmt(u, &mut update_env, goto_targets, all_xvars);
                }
                break;
            }
            DirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                let pre = env.clone();
                changed |= sccp_subst_expr(expr, &pre);
                changed |= fold_expr_hir(expr);
                if let Some((v, _)) = eval_hir_expr_with_const_env(expr, &pre) {
                    let mut taken: Option<Vec<DirStmt>> = None;
                    for case in cases.iter_mut() {
                        if case.values.iter().any(|x| *x == v) {
                            taken = Some(std::mem::take(&mut case.body));
                            break;
                        }
                    }
                    let discarded_have_side_effects = cases
                        .iter()
                        .filter(|case| !case.values.iter().any(|x| *x == v))
                        .any(|case| stmt_list_has_side_effects(&case.body))
                        || stmt_list_has_side_effects(default);
                    if !discarded_have_side_effects {
                        let blk = taken.unwrap_or_else(|| std::mem::take(default));
                        *stmt = DirStmt::Block(blk);
                        changed = true;
                        continue;
                    }
                }
                let mut acc: Option<ConstEnv> = None;
                for case in cases.iter_mut() {
                    let mut e = pre.clone();
                    changed |=
                        sccp_transform_stmts(&mut case.body, &mut e, goto_targets, all_xvars);
                    acc = Some(match acc {
                        None => e,
                        Some(a) => merge_env(&a, &e),
                    });
                }
                let mut ed = pre.clone();
                changed |= sccp_transform_stmts(default, &mut ed, goto_targets, all_xvars);
                *env = merge_env(acc.as_ref().unwrap_or(&pre), &ed);
                break;
            }
            DirStmt::Label(label) => {
                if goto_targets.contains(label) {
                    env.clear();
                }
                break;
            }
            DirStmt::Return(None) | DirStmt::Break | DirStmt::Continue | DirStmt::Goto(_) => {
                env.clear();
                break;
            }
        }
    }
    changed
}

fn collect_xvars_in_stmts(stmts: &[DirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_xvars_in_stmt(stmt, out);
    }
}

fn collect_xvars_in_stmt(stmt: &DirStmt, out: &mut HashSet<String>) {
    match stmt {
        DirStmt::Assign { lhs, rhs } => {
            collect_xvars_in_lvalue(lhs, out);
            collect_xvars_in_expr(rhs, out);
        }
        DirStmt::VaStart { va_list, .. } => {
            collect_xvars_in_expr(va_list, out);
        }
        DirStmt::Expr(expr) | DirStmt::Return(Some(expr)) => {
            collect_xvars_in_expr(expr, out);
        }
        DirStmt::Block(body) | DirStmt::While { body, .. } | DirStmt::DoWhile { body, .. } => {
            collect_xvars_in_stmts(body, out);
        }
        DirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(s) = init {
                collect_xvars_in_stmt(s, out);
            }
            if let Some(e) = cond {
                collect_xvars_in_expr(e, out);
            }
            if let Some(s) = update {
                collect_xvars_in_stmt(s, out);
            }
            collect_xvars_in_stmts(body, out);
        }
        DirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_xvars_in_expr(cond, out);
            collect_xvars_in_stmts(then_body, out);
            collect_xvars_in_stmts(else_body, out);
        }
        DirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            collect_xvars_in_expr(expr, out);
            for case in cases {
                collect_xvars_in_stmts(&case.body, out);
            }
            collect_xvars_in_stmts(default, out);
        }
        _ => {}
    }
}

fn collect_xvars_in_lvalue(lhs: &DirLValue, out: &mut HashSet<String>) {
    match lhs {
        DirLValue::Var(name) => {
            if name.starts_with("xVar") {
                out.insert(name.clone());
            }
        }
        DirLValue::Deref { ptr, .. } => {
            collect_xvars_in_expr(ptr, out);
        }
        DirLValue::Index { base, index, .. } => {
            collect_xvars_in_expr(base, out);
            collect_xvars_in_expr(index, out);
        }
        DirLValue::FieldAccess { base, .. } => {
            collect_xvars_in_expr(base, out);
        }
    }
}

fn collect_xvars_in_expr(expr: &DirExpr, out: &mut HashSet<String>) {
    match expr {
        DirExpr::Var(name) | DirExpr::AddressOfGlobal(name) => {
            if name.starts_with("xVar") {
                out.insert(name.clone());
            }
        }
        DirExpr::Cast { expr: inner, .. }
        | DirExpr::Unary { expr: inner, .. }
        | DirExpr::Load { ptr: inner, .. }
        | DirExpr::FieldAccess { base: inner, .. } => {
            collect_xvars_in_expr(inner, out);
        }
        DirExpr::PtrOffset { base, .. } => {
            collect_xvars_in_expr(base, out);
        }
        DirExpr::AggregateCopy { src, .. } => {
            collect_xvars_in_expr(src, out);
        }
        DirExpr::Binary { lhs, rhs, .. } => {
            collect_xvars_in_expr(lhs, out);
            collect_xvars_in_expr(rhs, out);
        }
        DirExpr::Index { base, index, .. } => {
            collect_xvars_in_expr(base, out);
            collect_xvars_in_expr(index, out);
        }
        DirExpr::Call { args, .. } => {
            for arg in args {
                collect_xvars_in_expr(arg, out);
            }
        }
        DirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_xvars_in_expr(cond, out);
            collect_xvars_in_expr(then_expr, out);
            collect_xvars_in_expr(else_expr, out);
        }
        DirExpr::Const(_, _) => {}
    }
}
