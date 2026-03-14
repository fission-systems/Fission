use super::*;
use super::arith::{
    canonicalize_condition_expr, canonicalize_integer_expr, cleanup_arithmetic_wrappers,
    collapse_zero_offset_cast, normalize_boolean_logic, recognize_hi_lo_extract,
    recognize_mod_div_power_of_two, recognize_wide_integer_recombine,
};
use super::bitstream::apply_bitstream_idioms;
use super::cleanup::{collapse_trivial_assign_returns, inline_single_use_temps};
use super::slots::{apply_memory_slot_surfacing, normalize_binding_initializers};

pub(super) fn normalize_function_body(body: &mut Vec<HirStmt>) {
    for stmt in body.iter_mut() {
        normalize_stmt(stmt);
    }
    loop {
        let mut changed = false;
        changed |= collapse_trivial_assign_returns(body);
        changed |= inline_single_use_temps(body);
        if !changed {
            break;
        }
        for stmt in body.iter_mut() {
            normalize_stmt(stmt);
        }
    }
}

pub(super) fn normalize_hir_function(func: &mut HirFunction) {
    normalize_binding_initializers(&mut func.locals);
    normalize_function_body(&mut func.body);
    let allow_expensive_passes = !is_large_hir_function(func);
    let mut changed = false;
    if allow_expensive_passes {
        changed |= apply_memory_slot_surfacing(func);
        normalize_binding_initializers(&mut func.locals);
        normalize_function_body(&mut func.body);
        changed |= apply_bitstream_idioms(func);
        if changed {
            normalize_binding_initializers(&mut func.locals);
            normalize_function_body(&mut func.body);
        }
    }
}

fn is_large_hir_function(func: &HirFunction) -> bool {
    count_hir_stmts(&func.body) > 220 || func.locals.len() > 160
}

fn count_hir_stmts(stmts: &[HirStmt]) -> usize {
    fn count_stmt(stmt: &HirStmt) -> usize {
        match stmt {
            HirStmt::Block(stmts)
            | HirStmt::While { body: stmts, .. }
            | HirStmt::DoWhile { body: stmts, .. } => 1 + count_hir_stmts(stmts),
            HirStmt::Switch { cases, default, .. } => {
                1 + cases.iter().map(|case| count_hir_stmts(&case.body)).sum::<usize>()
                    + count_hir_stmts(default)
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => 1 + count_hir_stmts(then_body) + count_hir_stmts(else_body),
            _ => 1,
        }
    }

    stmts.iter().map(count_stmt).sum()
}

pub(super) fn normalize_stmt(stmt: &mut HirStmt) {
    match stmt {
        HirStmt::Assign { rhs, .. } => normalize_expr(rhs),
        HirStmt::Expr(expr) => normalize_expr(expr),
        HirStmt::Block(stmts) => {
            for stmt in stmts {
                normalize_stmt(stmt);
            }
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            normalize_expr(expr);
            for case in cases {
                for stmt in &mut case.body {
                    normalize_stmt(stmt);
                }
            }
            for stmt in default {
                normalize_stmt(stmt);
            }
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            normalize_condition_expr(cond);
            for stmt in then_body {
                normalize_stmt(stmt);
            }
            for stmt in else_body {
                normalize_stmt(stmt);
            }
        }
        HirStmt::While { cond, body } => {
            normalize_condition_expr(cond);
            for stmt in body {
                normalize_stmt(stmt);
            }
        }
        HirStmt::DoWhile { body, cond } => {
            for stmt in body {
                normalize_stmt(stmt);
            }
            normalize_condition_expr(cond);
        }
        HirStmt::Label(_) | HirStmt::Goto(_) => {}
        HirStmt::Return(Some(expr)) => normalize_expr(expr),
        HirStmt::Return(None) | HirStmt::Break | HirStmt::Continue => {}
    }
}

fn normalize_condition_expr(expr: &mut HirExpr) {
    normalize_expr(expr);
    let mut current = expr.clone();
    loop {
        let next = canonicalize_condition_expr(&current);
        match next {
            Some(next_expr) if next_expr != current => {
                current = next_expr;
                normalize_expr(&mut current);
            }
            _ => break,
        }
    }
    *expr = current;
}

pub(super) fn normalize_expr(expr: &mut HirExpr) {
    match expr {
        HirExpr::Cast { expr: inner, .. } => normalize_expr(inner),
        HirExpr::Unary { expr: inner, .. } => normalize_expr(inner),
        HirExpr::Binary { lhs, rhs, .. } => {
            normalize_expr(lhs);
            normalize_expr(rhs);
        }
        HirExpr::Call { args, .. } => {
            for arg in args {
                normalize_expr(arg);
            }
        }
        HirExpr::Load { ptr, .. } | HirExpr::PtrOffset { base: ptr, .. } => normalize_expr(ptr),
        HirExpr::Index { base, index, .. } => {
            normalize_expr(base);
            normalize_expr(index);
        }
        HirExpr::AggregateCopy { src, .. } => normalize_expr(src),
        HirExpr::Var(_) | HirExpr::Const(_, _) => {}
    }

    let mut current = expr.clone();
    loop {
        let next = canonicalize_integer_expr(&current)
            .or_else(|| recognize_mod_div_power_of_two(&current))
            .or_else(|| recognize_hi_lo_extract(&current))
            .or_else(|| recognize_wide_integer_recombine(&current))
            .or_else(|| normalize_boolean_logic(&current))
            .or_else(|| cleanup_arithmetic_wrappers(&current))
            .or_else(|| collapse_zero_offset_cast(&current));
        match next {
            Some(next_expr) if next_expr != current => current = next_expr,
            _ => break,
        }
    }
    *expr = current;
}
