//! Sparse conditional constant propagation (SCCP) on structured HIR.
//!
//! Tracks a lattice of `Var → (i64, NirType)` along straight-line flow, merges at
//! `if`/`switch` joins, and conservatively drops variables written in loop bodies
//! from the map after `while`/`for`/`do-while`.  This complements
//! [`super::super::analysis::defuse::constant_folding_pass`] (single-statement fold) and VSA
//! [`fission_midend_prehir::vsa::jump_resolver`] (intervals, not a constant lattice).

use super::super::analysis::defuse::{eval_hir_expr_with_const_env, fold_expr_hir};
use super::super::cleanup::expr_has_side_effects;
use super::super::pipeline::is_large_hir_function;
use crate::prelude::*;
use crate::{HashMap, HashSet};

type ConstEnv = HashMap<String, (i64, NirType)>;

pub fn apply_sccp_pass(func: &mut PreHirFunction) -> bool {
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

fn collect_goto_targets(stmts: &[PreHirStmt]) -> HashSet<String> {
    let mut targets = HashSet::default();
    for stmt in stmts {
        collect_goto_targets_stmt(stmt, &mut targets);
    }
    targets
}

fn collect_goto_targets_stmt(stmt: &PreHirStmt, targets: &mut HashSet<String>) {
    match stmt {
        PreHirStmt::Goto(label) => {
            targets.insert(label.clone());
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => {
            for s in body.iter() {
                collect_goto_targets_stmt(s, targets);
            }
        }
        PreHirStmt::For {
            init, update, body, ..
        } => {
            if let Some(s) = init.as_deref() {
                collect_goto_targets_stmt(s, targets);
            }
            for s in body.iter() {
                collect_goto_targets_stmt(s, targets);
            }
            if let Some(s) = update.as_deref() {
                collect_goto_targets_stmt(s, targets);
            }
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body.iter() {
                collect_goto_targets_stmt(s, targets);
            }
            for s in else_body.iter() {
                collect_goto_targets_stmt(s, targets);
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in case.body.iter() {
                    collect_goto_targets_stmt(s, targets);
                }
            }
            for s in default.iter() {
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

fn stmt_list_has_side_effects(stmts: &[PreHirStmt]) -> bool {
    stmts.iter().any(stmt_has_side_effects)
}

/// Whether `branch` defines a label some `Goto` elsewhere in the function
/// still targets. GCC loop-rotation emits `if (precheck) { while (1) { L:
/// ... } }` where the real entry is an unconditional `goto L` from a
/// different scope, landing straight past this guard -- once constant
/// propagation reduces `precheck` to a literal, discarding the branch that
/// contains `L` would erase it out from under that goto, even though
/// nothing in `branch` itself makes the discard obviously unsafe (no side
/// effects visible from here). `goto_targets` is the whole-function
/// referenced-label set collected once in `apply_sccp_pass`, so this sees
/// cross-scope references a purely local scan would miss.
fn branch_defines_goto_target(branch: &[PreHirStmt], goto_targets: &HashSet<String>) -> bool {
    super::super::cleanup::utils::collect_defined_labels(branch)
        .iter()
        .any(|label| goto_targets.contains(label))
}

fn stmt_has_side_effects(stmt: &PreHirStmt) -> bool {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            lvalue_has_side_effects(lhs) || expr_has_side_effects(rhs)
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => expr_has_side_effects(expr),
        PreHirStmt::Block(body) => stmt_list_has_side_effects(body),
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => stmt_list_has_side_effects(then_body) || stmt_list_has_side_effects(else_body),
        PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. }
        | PreHirStmt::For { body, .. } => stmt_list_has_side_effects(body),
        PreHirStmt::Switch { cases, default, .. } => {
            cases.iter().any(|c| stmt_list_has_side_effects(&c.body))
                || stmt_list_has_side_effects(default)
        }
        PreHirStmt::VaStart { .. } => true,
        _ => false,
    }
}

fn lvalue_has_side_effects(lhs: &PreHirLValue) -> bool {
    match lhs {
        PreHirLValue::Var(_) => false,
        PreHirLValue::Deref { ptr, .. } => expr_has_side_effects(ptr),
        PreHirLValue::Index { base, index, .. } => {
            expr_has_side_effects(base) || expr_has_side_effects(index)
        }
        PreHirLValue::FieldAccess { base, .. } => expr_has_side_effects(base),
    }
}

fn loop_variant_vars(body: &[PreHirStmt], all_xvars: &HashSet<String>) -> HashSet<String> {
    let mut vars = HashSet::default();
    for stmt in body {
        loop_variant_stmt(stmt, &mut vars);
    }
    for xvar in all_xvars {
        vars.insert(xvar.clone());
    }
    vars
}

fn loop_variant_stmt(stmt: &PreHirStmt, out: &mut HashSet<String>) {
    match stmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            ..
        } => {
            out.insert(name.clone());
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            for s in then_body.iter() {
                loop_variant_stmt(s, out);
            }
            for s in else_body.iter() {
                loop_variant_stmt(s, out);
            }
        }
        PreHirStmt::Block(body) => {
            for s in body.iter() {
                loop_variant_stmt(s, out);
            }
        }
        PreHirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in case.body.iter() {
                    loop_variant_stmt(s, out);
                }
            }
            for s in default.iter() {
                loop_variant_stmt(s, out);
            }
        }
        PreHirStmt::While { .. } | PreHirStmt::DoWhile { .. } | PreHirStmt::For { .. } => {}
        _ => {}
    }
}

fn sccp_subst_expr(expr: &mut PreHirExpr, env: &ConstEnv) -> bool {
    let mut changed = false;
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => {
            if let Some((v, ty)) = env.get(name) {
                *expr = PreHirExpr::Const(*v, ty.clone());
                changed = true;
            }
        }
        PreHirExpr::Unary { expr: inner, .. } => changed |= sccp_subst_expr(inner, env),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            changed |= sccp_subst_expr(lhs, env);
            changed |= sccp_subst_expr(rhs, env);
        }
        PreHirExpr::Cast { expr: inner, .. } => changed |= sccp_subst_expr(inner, env),
        PreHirExpr::Load { ptr, .. } => changed |= sccp_subst_expr(ptr, env),
        PreHirExpr::PtrOffset { base, .. } => changed |= sccp_subst_expr(base, env),
        PreHirExpr::FieldAccess { base, .. } => changed |= sccp_subst_expr(base, env),
        PreHirExpr::Index { base, index, .. } => {
            changed |= sccp_subst_expr(base, env);
            changed |= sccp_subst_expr(index, env);
        }
        PreHirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                changed |= sccp_subst_expr(a, env);
            }
        }
        PreHirExpr::AggregateCopy { src, .. } => changed |= sccp_subst_expr(src, env),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            changed |= sccp_subst_expr(cond, env);
            changed |= sccp_subst_expr(then_expr, env);
            changed |= sccp_subst_expr(else_expr, env);
        }
        PreHirExpr::Const(_, _) => {}
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

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_string())
    }

    #[test]
    fn sccp_keeps_backedge_label_values_nonconstant() {
        let mut func = PreHirFunction {
            name: "test_sccp_unstructured_backedge".to_string(),
            int_param_offsets: Vec::new(),
            return_type: int(32),
            body: vec![
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Const(0, int(32)),
                },
                PreHirStmt::Label("loop".to_string()),
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var("x".to_string()),
                    rhs: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Add,
                        lhs: Box::new(var("x")),
                        rhs: Box::new(PreHirExpr::Const(1, int(32))),
                        ty: int(32),
                    },
                },
                PreHirStmt::If {
                    cond: PreHirExpr::Binary {
                        op: PreHirBinaryOp::Sub,
                        lhs: Box::new(var("rows")),
                        rhs: Box::new(var("x")),
                        ty: int(32),
                    },
                    then_body: vec![PreHirStmt::Goto("loop".to_string())].into(),
                    else_body: vec![].into(),
                },
            ],
            ..Default::default()
        };

        apply_sccp_pass(&mut func);

        let PreHirStmt::If { cond, .. } = &func.body[3] else {
            panic!("expected loop branch to remain an if");
        };
        let PreHirExpr::Binary { rhs, .. } = cond else {
            panic!("expected branch condition to remain binary");
        };
        assert_eq!(rhs.as_ref(), &var("x"));
    }

    #[test]
    fn sccp_does_not_prune_if_when_discarded_branch_has_side_effects() {
        let mut func = PreHirFunction {
            name: "test".to_string(),
            int_param_offsets: Vec::new(),
            return_type: int(32),
            body: vec![PreHirStmt::If {
                cond: PreHirExpr::Const(1, int(32)),
                then_body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                    target: "add".to_string(),
                    args: vec![PreHirExpr::Const(1, int(32)), PreHirExpr::Const(2, int(32))],
                    ty: int(32),
                })]
                .into(),
                else_body: vec![PreHirStmt::Expr(PreHirExpr::Call {
                    target: "max".to_string(),
                    args: vec![PreHirExpr::Const(3, int(32)), PreHirExpr::Const(4, int(32))],
                    ty: int(32),
                })]
                .into(),
            }],
            ..Default::default()
        };

        apply_sccp_pass(&mut func);

        let PreHirStmt::If {
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

fn eval_truth(expr: &PreHirExpr, env: &ConstEnv) -> Option<bool> {
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
    cond: &PreHirExpr,
) -> (Vec<(String, i64, NirType)>, Vec<(String, i64, NirType)>) {
    let mut then_bindings: Vec<(String, i64, NirType)> = Vec::new();
    let mut else_bindings: Vec<(String, i64, NirType)> = Vec::new();
    extract_branch_constants(cond, false, &mut then_bindings, &mut else_bindings);
    (then_bindings, else_bindings)
}

fn extract_branch_constants(
    cond: &PreHirExpr,
    negated: bool,
    then_bindings: &mut Vec<(String, i64, NirType)>,
    else_bindings: &mut Vec<(String, i64, NirType)>,
) {
    match cond {
        // NOT: flip then/else roles.
        PreHirExpr::Unary {
            op: PreHirUnaryOp::Not,
            expr: inner,
            ..
        } => {
            extract_branch_constants(inner, !negated, then_bindings, else_bindings);
        }
        // x == K or K == x  → then: x=K ; x != K or K != x → else: x=K
        PreHirExpr::Binary {
            op: op @ (PreHirBinaryOp::Eq | PreHirBinaryOp::Ne),
            lhs,
            rhs,
            ..
        } => {
            let (var_name, const_val, ty) = match (lhs.as_ref(), rhs.as_ref()) {
                (PreHirExpr::Var(name), PreHirExpr::Const(k, ty)) => (name.clone(), *k, ty.clone()),
                (PreHirExpr::Const(k, ty), PreHirExpr::Var(name)) => (name.clone(), *k, ty.clone()),
                _ => return,
            };
            // For `==`: const holds in then-branch (unless negated → else-branch).
            // For `!=`: const holds in else-branch.
            let const_in_then = matches!(op, PreHirBinaryOp::Eq) ^ negated;
            if const_in_then {
                then_bindings.push((var_name, const_val, ty));
            } else {
                else_bindings.push((var_name, const_val, ty));
            }
        }
        // cond_a && cond_b → then: both hold; else: nothing (either could be false).
        PreHirExpr::Binary {
            op: PreHirBinaryOp::And,
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
    stmts: &mut Vec<PreHirStmt>,
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
    stmt: &mut PreHirStmt,
    env: &mut ConstEnv,
    goto_targets: &HashSet<String>,
    all_xvars: &HashSet<String>,
) -> bool {
    let mut changed = false;
    loop {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                if let PreHirLValue::Var(name) = lhs {
                    changed |= sccp_subst_expr(rhs, env);
                    changed |= fold_expr_hir(rhs);
                    if let Some((v, ty)) = eval_hir_expr_with_const_env(rhs, env) {
                        if !matches!(rhs, PreHirExpr::Const(cv, _) if *cv == v) {
                            *rhs = PreHirExpr::Const(v, ty.clone());
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
            PreHirStmt::VaStart { va_list, .. } => {
                changed |= sccp_subst_expr(va_list, env);
                changed |= fold_expr_hir(va_list);
                break;
            }
            PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
                changed |= sccp_subst_expr(expr, env);
                changed |= fold_expr_hir(expr);
                break;
            }
            PreHirStmt::Block(stmts) => {
                changed |= sccp_transform_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts),
                    env,
                    goto_targets,
                    all_xvars,
                );
                break;
            }
            PreHirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let pre = env.clone();
                changed |= sccp_subst_expr(cond, &pre);
                changed |= fold_expr_hir(cond);
                match eval_truth(cond, &pre) {
                    Some(true)
                        if !stmt_list_has_side_effects(else_body)
                            && !branch_defines_goto_target(else_body, goto_targets) =>
                    {
                        *stmt = PreHirStmt::Block(std::mem::take(then_body));
                        changed = true;
                        continue;
                    }
                    Some(false)
                        if !stmt_list_has_side_effects(then_body)
                            && !branch_defines_goto_target(then_body, goto_targets) =>
                    {
                        *stmt = PreHirStmt::Block(std::mem::take(else_body));
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
                        changed |= sccp_transform_stmts(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                            &mut e1,
                            goto_targets,
                            all_xvars,
                        );
                        changed |= sccp_transform_stmts(
                            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                            &mut e2,
                            goto_targets,
                            all_xvars,
                        );
                        *env = merge_env(&e1, &e2);
                    }
                }
                break;
            }
            PreHirStmt::While { cond, body } => {
                let pre = env.clone();
                let modified = loop_variant_vars(body, all_xvars);
                let loop_entry = env_without_vars(&pre, &modified);
                changed |= sccp_subst_expr(cond, &loop_entry);
                changed |= fold_expr_hir(cond);
                let mut inner = loop_entry;
                changed |= sccp_transform_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    &mut inner,
                    goto_targets,
                    all_xvars,
                );
                *env = env_without_vars(&pre, &modified);
                break;
            }
            PreHirStmt::DoWhile { body, cond } => {
                let pre = env.clone();
                let modified = loop_variant_vars(body, all_xvars);
                let mut inner = env_without_vars(&pre, &modified);
                changed |= sccp_transform_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    &mut inner,
                    goto_targets,
                    all_xvars,
                );
                let cond_env = env_without_vars(&inner, &modified);
                changed |= sccp_subst_expr(cond, &cond_env);
                changed |= fold_expr_hir(cond);
                *env = env_without_vars(&pre, &modified);
                break;
            }
            PreHirStmt::For {
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
                    if let PreHirStmt::Assign {
                        lhs: PreHirLValue::Var(n),
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
                changed |= sccp_transform_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(body),
                    &mut inner,
                    goto_targets,
                    all_xvars,
                );
                *env = env_without_vars(&loop_entry, &modified);
                if let Some(u) = update.as_mut() {
                    let mut update_env = env_without_vars(&inner, &modified);
                    changed |= sccp_stmt(u, &mut update_env, goto_targets, all_xvars);
                }
                break;
            }
            PreHirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                let pre = env.clone();
                changed |= sccp_subst_expr(expr, &pre);
                changed |= fold_expr_hir(expr);
                if let Some((v, _)) = eval_hir_expr_with_const_env(expr, &pre) {
                    let mut taken: Option<Vec<PreHirStmt>> = None;
                    for case in cases.iter_mut() {
                        if case.values.iter().any(|x| *x == v) {
                            taken = Some(std::mem::take(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(
                                &mut case.body,
                            )));
                            break;
                        }
                    }
                    let discarded_have_side_effects = cases
                        .iter()
                        .filter(|case| !case.values.iter().any(|x| *x == v))
                        .any(|case| stmt_list_has_side_effects(&case.body))
                        || stmt_list_has_side_effects(default);
                    if !discarded_have_side_effects {
                        let blk = taken.unwrap_or_else(|| {
                            std::mem::take(std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default))
                        });
                        *stmt = PreHirStmt::Block(blk.into());
                        changed = true;
                        continue;
                    }
                }
                let mut acc: Option<ConstEnv> = None;
                for case in cases.iter_mut() {
                    let mut e = pre.clone();
                    changed |= sccp_transform_stmts(
                        std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                        &mut e,
                        goto_targets,
                        all_xvars,
                    );
                    acc = Some(match acc {
                        None => e,
                        Some(a) => merge_env(&a, &e),
                    });
                }
                let mut ed = pre.clone();
                changed |= sccp_transform_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                    &mut ed,
                    goto_targets,
                    all_xvars,
                );
                *env = merge_env(acc.as_ref().unwrap_or(&pre), &ed);
                break;
            }
            PreHirStmt::Label(label) => {
                if goto_targets.contains(label) {
                    env.clear();
                }
                break;
            }
            PreHirStmt::Return(None)
            | PreHirStmt::Break
            | PreHirStmt::Continue
            | PreHirStmt::Goto(_) => {
                env.clear();
                break;
            }
        }
    }
    changed
}

fn collect_xvars_in_stmts(stmts: &[PreHirStmt], out: &mut HashSet<String>) {
    for stmt in stmts {
        collect_xvars_in_stmt(stmt, out);
    }
}

fn collect_xvars_in_stmt(stmt: &PreHirStmt, out: &mut HashSet<String>) {
    match stmt {
        PreHirStmt::Assign { lhs, rhs } => {
            collect_xvars_in_lvalue(lhs, out);
            collect_xvars_in_expr(rhs, out);
        }
        PreHirStmt::VaStart { va_list, .. } => {
            collect_xvars_in_expr(va_list, out);
        }
        PreHirStmt::Expr(expr) | PreHirStmt::Return(Some(expr)) => {
            collect_xvars_in_expr(expr, out);
        }
        PreHirStmt::Block(body)
        | PreHirStmt::While { body, .. }
        | PreHirStmt::DoWhile { body, .. } => {
            collect_xvars_in_stmts(body, out);
        }
        PreHirStmt::For {
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
        PreHirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_xvars_in_expr(cond, out);
            collect_xvars_in_stmts(then_body, out);
            collect_xvars_in_stmts(else_body, out);
        }
        PreHirStmt::Switch {
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

fn collect_xvars_in_lvalue(lhs: &PreHirLValue, out: &mut HashSet<String>) {
    match lhs {
        PreHirLValue::Var(name) => {
            if name.starts_with("xVar") {
                out.insert(name.clone());
            }
        }
        PreHirLValue::Deref { ptr, .. } => {
            collect_xvars_in_expr(ptr, out);
        }
        PreHirLValue::Index { base, index, .. } => {
            collect_xvars_in_expr(base, out);
            collect_xvars_in_expr(index, out);
        }
        PreHirLValue::FieldAccess { base, .. } => {
            collect_xvars_in_expr(base, out);
        }
    }
}

fn collect_xvars_in_expr(expr: &PreHirExpr, out: &mut HashSet<String>) {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => {
            if name.starts_with("xVar") {
                out.insert(name.clone());
            }
        }
        PreHirExpr::Cast { expr: inner, .. }
        | PreHirExpr::Unary { expr: inner, .. }
        | PreHirExpr::Load { ptr: inner, .. }
        | PreHirExpr::FieldAccess { base: inner, .. } => {
            collect_xvars_in_expr(inner, out);
        }
        PreHirExpr::PtrOffset { base, .. } => {
            collect_xvars_in_expr(base, out);
        }
        PreHirExpr::AggregateCopy { src, .. } => {
            collect_xvars_in_expr(src, out);
        }
        PreHirExpr::Binary { lhs, rhs, .. } => {
            collect_xvars_in_expr(lhs, out);
            collect_xvars_in_expr(rhs, out);
        }
        PreHirExpr::Index { base, index, .. } => {
            collect_xvars_in_expr(base, out);
            collect_xvars_in_expr(index, out);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                collect_xvars_in_expr(arg, out);
            }
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            collect_xvars_in_expr(cond, out);
            collect_xvars_in_expr(then_expr, out);
            collect_xvars_in_expr(else_expr, out);
        }
        PreHirExpr::Const(_, _) => {}
    }
}
