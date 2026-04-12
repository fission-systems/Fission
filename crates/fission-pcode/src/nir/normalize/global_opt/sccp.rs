//! Sparse conditional constant propagation (SCCP) on structured HIR.
//!
//! Tracks a lattice of `Var → (i64, NirType)` along straight-line flow, merges at
//! `if`/`switch` joins, and conservatively drops variables written in loop bodies
//! from the map after `while`/`for`/`do-while`.  This complements
//! [`super::super::analysis::defuse::constant_folding_pass`] (single-statement fold) and VSA
//! [`crate::nir::vsa::jump_resolver`] (intervals, not a constant lattice).

use super::super::analysis::defuse::{eval_hir_expr_with_const_env, fold_expr_hir};
use super::super::pipeline::is_large_hir_function;
use super::super::*;
use std::collections::{HashMap, HashSet};

type ConstEnv = HashMap<String, (i64, NirType)>;

pub(crate) fn apply_sccp_pass(func: &mut HirFunction) -> bool {
    let max_rounds = if is_large_hir_function(func) { 2 } else { 8 };
    let mut any = false;
    for _ in 0..max_rounds {
        let mut env = ConstEnv::new();
        if !sccp_transform_stmts(&mut func.body, &mut env) {
            break;
        }
        any = true;
    }
    any
}

fn merge_env(a: &ConstEnv, b: &ConstEnv) -> ConstEnv {
    let keys: HashSet<_> = a.keys().chain(b.keys()).cloned().collect();
    let mut out = ConstEnv::new();
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

fn loop_variant_vars(body: &[HirStmt]) -> HashSet<String> {
    let mut vars = HashSet::new();
    for stmt in body {
        loop_variant_stmt(stmt, &mut vars);
    }
    vars
}

fn loop_variant_stmt(stmt: &HirStmt, out: &mut HashSet<String>) {
    match stmt {
        HirStmt::Assign {
            lhs: HirLValue::Var(name),
            ..
        } => {
            out.insert(name.clone());
        }
        HirStmt::If {
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
        HirStmt::Block(body) => {
            for s in body {
                loop_variant_stmt(s, out);
            }
        }
        HirStmt::Switch { cases, default, .. } => {
            for case in cases {
                for s in &case.body {
                    loop_variant_stmt(s, out);
                }
            }
            for s in default {
                loop_variant_stmt(s, out);
            }
        }
        HirStmt::While { .. } | HirStmt::DoWhile { .. } | HirStmt::For { .. } => {}
        _ => {}
    }
}

fn sccp_subst_expr(expr: &mut HirExpr, env: &ConstEnv) {
    match expr {
        HirExpr::Var(name) => {
            if let Some((v, ty)) = env.get(name) {
                *expr = HirExpr::Const(*v, ty.clone());
            }
        }
        HirExpr::Unary { expr: inner, .. } => sccp_subst_expr(inner, env),
        HirExpr::Binary { lhs, rhs, .. } => {
            sccp_subst_expr(lhs, env);
            sccp_subst_expr(rhs, env);
        }
        HirExpr::Cast { expr: inner, .. } => sccp_subst_expr(inner, env),
        HirExpr::Load { ptr, .. } => sccp_subst_expr(ptr, env),
        HirExpr::PtrOffset { base, .. } => sccp_subst_expr(base, env),
        HirExpr::Index { base, index, .. } => {
            sccp_subst_expr(base, env);
            sccp_subst_expr(index, env);
        }
        HirExpr::Call { args, .. } => {
            for a in args.iter_mut() {
                sccp_subst_expr(a, env);
            }
        }
        HirExpr::AggregateCopy { src, .. } => sccp_subst_expr(src, env),
        HirExpr::Const(_, _) => {}
    }
}

fn eval_truth(expr: &HirExpr, env: &ConstEnv) -> Option<bool> {
    let (v, _) = eval_hir_expr_with_const_env(expr, env)?;
    Some(v != 0)
}

fn sccp_transform_stmts(stmts: &mut Vec<HirStmt>, env: &mut ConstEnv) -> bool {
    let mut changed = false;
    let mut i = 0;
    while i < stmts.len() {
        changed |= sccp_stmt(&mut stmts[i], env);
        i += 1;
    }
    changed
}

fn sccp_stmt(stmt: &mut HirStmt, env: &mut ConstEnv) -> bool {
    let mut changed = false;
    loop {
        match stmt {
            HirStmt::Assign { lhs, rhs } => {
                if let HirLValue::Var(name) = lhs {
                    sccp_subst_expr(rhs, env);
                    fold_expr_hir(rhs);
                    if let Some((v, ty)) = eval_hir_expr_with_const_env(rhs, env) {
                        if !matches!(rhs, HirExpr::Const(cv, _) if *cv == v) {
                            *rhs = HirExpr::Const(v, ty.clone());
                            changed = true;
                        }
                        env.insert(name.clone(), (v, ty));
                    } else {
                        env.remove(name);
                    }
                } else {
                    sccp_subst_expr(rhs, env);
                    fold_expr_hir(rhs);
                }
                break;
            }
            HirStmt::VaStart { va_list, .. } => {
                sccp_subst_expr(va_list, env);
                changed |= fold_expr_hir(va_list);
                break;
            }
            HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => {
                sccp_subst_expr(expr, env);
                changed |= fold_expr_hir(expr);
                break;
            }
            HirStmt::Block(stmts) => {
                changed |= sccp_transform_stmts(stmts, env);
                break;
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let pre = env.clone();
                sccp_subst_expr(cond, &pre);
                fold_expr_hir(cond);
                match eval_truth(cond, &pre) {
                    Some(true) => {
                        *stmt = HirStmt::Block(std::mem::take(then_body));
                        changed = true;
                        continue;
                    }
                    Some(false) => {
                        *stmt = HirStmt::Block(std::mem::take(else_body));
                        changed = true;
                        continue;
                    }
                    None => {
                        let mut e1 = pre.clone();
                        let mut e2 = pre.clone();
                        changed |= sccp_transform_stmts(then_body, &mut e1);
                        changed |= sccp_transform_stmts(else_body, &mut e2);
                        *env = merge_env(&e1, &e2);
                    }
                }
                break;
            }
            HirStmt::While { cond, body } => {
                let pre = env.clone();
                sccp_subst_expr(cond, &pre);
                fold_expr_hir(cond);
                let modified = loop_variant_vars(body);
                let mut inner = pre.clone();
                changed |= sccp_transform_stmts(body, &mut inner);
                *env = pre;
                for m in modified {
                    env.remove(&m);
                }
                break;
            }
            HirStmt::DoWhile { body, cond } => {
                let pre = env.clone();
                let modified = loop_variant_vars(body);
                let mut inner = pre.clone();
                changed |= sccp_transform_stmts(body, &mut inner);
                sccp_subst_expr(cond, &inner);
                fold_expr_hir(cond);
                *env = pre;
                for m in modified {
                    env.remove(&m);
                }
                break;
            }
            HirStmt::For {
                init,
                cond,
                update,
                body,
            } => {
                if let Some(i) = init.as_mut() {
                    changed |= sccp_stmt(i, env);
                }
                let loop_entry = env.clone();
                if let Some(c) = cond.as_mut() {
                    sccp_subst_expr(c, &loop_entry);
                    fold_expr_hir(c);
                }
                let mut modified = loop_variant_vars(body);
                if let Some(u) = update {
                    if let HirStmt::Assign {
                        lhs: HirLValue::Var(n),
                        ..
                    } = u.as_ref()
                    {
                        modified.insert(n.clone());
                    }
                }
                let mut inner = loop_entry.clone();
                changed |= sccp_transform_stmts(body, &mut inner);
                *env = loop_entry;
                for m in modified {
                    env.remove(&m);
                }
                if let Some(u) = update.as_mut() {
                    changed |= sccp_stmt(u, env);
                }
                break;
            }
            HirStmt::Switch {
                expr,
                cases,
                default,
            } => {
                let pre = env.clone();
                sccp_subst_expr(expr, &pre);
                fold_expr_hir(expr);
                if let Some((v, _)) = eval_hir_expr_with_const_env(expr, &pre) {
                    let mut taken: Option<Vec<HirStmt>> = None;
                    for case in cases.iter_mut() {
                        if case.values.iter().any(|x| *x == v) {
                            taken = Some(std::mem::take(&mut case.body));
                            break;
                        }
                    }
                    let blk = taken.unwrap_or_else(|| std::mem::take(default));
                    *stmt = HirStmt::Block(blk);
                    changed = true;
                    continue;
                }
                let mut acc: Option<ConstEnv> = None;
                for case in cases.iter_mut() {
                    let mut e = pre.clone();
                    changed |= sccp_transform_stmts(&mut case.body, &mut e);
                    acc = Some(match acc {
                        None => e,
                        Some(a) => merge_env(&a, &e),
                    });
                }
                let mut ed = pre.clone();
                changed |= sccp_transform_stmts(default, &mut ed);
                *env = merge_env(acc.as_ref().unwrap_or(&pre), &ed);
                break;
            }
            HirStmt::Return(None)
            | HirStmt::Break
            | HirStmt::Continue
            | HirStmt::Label(_)
            | HirStmt::Goto(_) => break,
        }
    }
    changed
}
