use super::arith::{
    canonicalize_condition_expr, canonicalize_flag_intrinsics, canonicalize_integer_expr,
    cleanup_arithmetic_wrappers, collapse_zero_offset_cast, normalize_boolean_logic,
    recognize_hi_lo_extract, recognize_mod_div_power_of_two, recognize_wide_integer_recombine,
};
use super::bitstream::apply_bitstream_idioms;
use super::cleanup::{
    collapse_trivial_assign_returns, eliminate_dead_local_clobber_assigns,
    eliminate_dead_temp_assigns, inline_single_use_temps, prune_unused_dead_local_bindings,
    prune_unused_temp_bindings,
};
use super::slots::{
    apply_memory_slot_surfacing, apply_memory_slot_surfacing_cheap, normalize_binding_initializers,
};
use super::*;
use std::time::Instant;

pub(super) fn normalize_function_body(body: &mut Vec<HirStmt>) {
    cleanup_stmt_list(body, "<body>", 0);
}

pub(super) fn normalize_hir_function(func: &mut HirFunction) {
    let diag = normalize_diag_enabled();
    let total_start = Instant::now();
    if diag {
        eprintln!(
            "[DIAG] normalize start: {} stmts={} locals={}",
            func.name,
            count_hir_stmts(&func.body),
            func.locals.len()
        );
    }
    normalize_binding_initializers(&mut func.locals);
    cleanup_stmt_list(&mut func.body, &func.name, 0);
    eliminate_dead_local_clobber_assigns(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
    let allow_expensive_passes = !is_large_hir_function(func);
    let mut changed = false;
    let pass_start = Instant::now();
    changed |= if allow_expensive_passes {
        apply_memory_slot_surfacing(func)
    } else {
        apply_memory_slot_surfacing_cheap(func)
    };
    if diag {
        eprintln!(
            "[DIAG] normalize slots: {} changed={} elapsed={:.3}s mode={}",
            func.name,
            changed,
            pass_start.elapsed().as_secs_f64(),
            if allow_expensive_passes {
                "full"
            } else {
                "cheap"
            }
        );
    }
    if changed {
        normalize_binding_initializers(&mut func.locals);
        cleanup_stmt_list(&mut func.body, &func.name, 0);
        eliminate_dead_local_clobber_assigns(func);
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    if allow_expensive_passes {
        let bitstream_start = Instant::now();
        changed |= apply_bitstream_idioms(func);
        if diag {
            eprintln!(
                "[DIAG] normalize bitstream: {} changed={} elapsed={:.3}s",
                func.name,
                changed,
                bitstream_start.elapsed().as_secs_f64()
            );
        }
        if changed {
            normalize_binding_initializers(&mut func.locals);
            cleanup_stmt_list(&mut func.body, &func.name, 0);
            eliminate_dead_local_clobber_assigns(func);
            prune_unused_temp_bindings(func);
            prune_unused_dead_local_bindings(func);
        }
    }
    if diag {
        eprintln!(
            "[DIAG] normalize done: {} total_elapsed={:.3}s",
            func.name,
            total_start.elapsed().as_secs_f64()
        );
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
                1 + cases
                    .iter()
                    .map(|case| count_hir_stmts(&case.body))
                    .sum::<usize>()
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

fn cleanup_stmt_list(stmts: &mut Vec<HirStmt>, func_name: &str, depth: usize) {
    for stmt in stmts.iter_mut() {
        normalize_stmt(stmt);
        match stmt {
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                cleanup_stmt_list(body, func_name, depth + 1)
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                cleanup_stmt_list(then_body, func_name, depth + 1);
                cleanup_stmt_list(else_body, func_name, depth + 1);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    cleanup_stmt_list(&mut case.body, func_name, depth + 1);
                }
                cleanup_stmt_list(default, func_name, depth + 1);
            }
            HirStmt::Assign { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }

    let diag = normalize_diag_enabled();
    let loop_start = Instant::now();
    let mut iterations = 0usize;
    loop {
        iterations += 1;
        let mut changed = false;
        let mut last_changed_pass = None;
        if collapse_trivial_assign_returns(stmts) {
            changed = true;
            last_changed_pass = Some("collapse_trivial_assign_returns");
        }
        if inline_single_use_temps(stmts) {
            changed = true;
            last_changed_pass = Some("inline_single_use_temps");
        }
        if eliminate_dead_temp_assigns(stmts) {
            changed = true;
            last_changed_pass = Some("eliminate_dead_temp_assigns");
        }
        if !changed {
            break;
        }
        if diag && iterations % 50 == 0 {
            eprintln!(
                "[DIAG] normalize loop: {} depth={} iterations={} elapsed={:.3}s last_changed_pass={}",
                func_name,
                depth,
                iterations,
                loop_start.elapsed().as_secs_f64(),
                last_changed_pass.unwrap_or("<none>")
            );
        }
        for stmt in stmts.iter_mut() {
            normalize_stmt(stmt);
        }
    }
    if diag && (iterations > 1 || loop_start.elapsed().as_millis() > 100) {
        eprintln!(
            "[DIAG] normalize loop done: {} depth={} iterations={} elapsed={:.3}s",
            func_name,
            depth,
            iterations,
            loop_start.elapsed().as_secs_f64()
        );
    }
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
            .or_else(|| canonicalize_flag_intrinsics(&current))
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

fn normalize_diag_enabled() -> bool {
    std::env::var_os("FISSION_PREVIEW_DIAG").is_some()
}
