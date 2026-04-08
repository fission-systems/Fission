use super::arith::{
    canonicalize_condition_expr, canonicalize_flag_intrinsics, canonicalize_integer_expr,
    cleanup_arithmetic_wrappers, collapse_zero_offset_cast, merge_consecutive_shifts,
    normalize_boolean_logic, recognize_hi_lo_extract, recognize_mod_div_power_of_two,
    recognize_magic_number_division, recognize_wide_integer_recombine, simplify_subpiece_chain,
};
use super::bitstream::apply_bitstream_idioms;
use super::cleanup::{
    cast_elision_pass, cleanup_redundant_boundary_labels, collapse_trivial_assign_returns,
    elide_unused_popcount_assigns, eliminate_dead_local_clobber_assigns,
    eliminate_dead_temp_assigns, fuse_single_predecessor_boundaries, inline_single_use_temps,
    promote_guarded_jump_target_tail, prune_unused_dead_local_bindings, prune_unused_temp_bindings,
    remove_unreferenced_leading_labels, simplify_empty_and_constant_ifs,
    simplify_fallthrough_edges,
};
use super::defuse::{constant_folding_pass, defuse_dead_assignment_pass};
use super::phi_recovery::{copy_propagation_pass, join_coalescing_pass};
use super::flag_recovery::apply_flag_recovery_pass;
use super::prologue::remove_callee_save_prologue_epilogue;
use super::aggregate_fields::apply_aggregate_fields_pass;
use super::callsite_type_prop::apply_callsite_type_prop_pass;
use super::dead_store::apply_dead_store_elimination;
use super::iv_recovery::{apply_break_continue_pass, apply_iv_recovery_pass};
use crate::nir::vsa::apply_jump_resolver_pass;
use super::type_infer::apply_type_inference_pass;
use super::use_type_infer::apply_use_driven_type_infer_pass;
use super::ptr_arith::apply_ptr_arith_recovery_pass;
use super::cleanup::single_pred_label_inline;
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
    super::for_loops::apply_for_loop_folding(&mut func.body);
    eliminate_dead_local_clobber_assigns(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
    // Flag recovery: substitute raw x86 EFLAGS variable references in branch
    // conditions with high-level comparison expressions (sf!=of → a<b signed,
    // !zf → a!=b, etc.).  Runs early so that subsequent dead-assignment passes
    // can eliminate now-dead flag-variable assignments.
    if apply_flag_recovery_pass(func) {
        cleanup_stmt_list(&mut func.body, &func.name, 0);
        defuse_dead_assignment_pass(func);
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    // Parity / popcount dead elimination: remove __popcount-based assignments
    // whose result is not consumed anywhere (e.g., dead parity flag variables
    // remaining after flag recovery or simple unused parity computations).
    if elide_unused_popcount_assigns(func) {
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    // Prologue/epilogue elimination: remove callee-saved register save/restore
    // pairs (`*spill = r15` / `r15 = *spill`) from the function body.
    if remove_callee_save_prologue_epilogue(func) {
        cleanup_stmt_list(&mut func.body, &func.name, 0);
        defuse_dead_assignment_pass(func);
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    // Run constant folding after the initial cleanup so that folded constants
    // unlock further simplifications in subsequent passes.
    if constant_folding_pass(&mut func.body) {
        cleanup_stmt_list(&mut func.body, &func.name, 0);
        eliminate_dead_local_clobber_assigns(func);
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    // Function-level def-use dead assignment: removes dead writes to ANY
    // variable (not just trivially-named temps) across the whole body tree.
    defuse_dead_assignment_pass(func);
    // Copy propagation: forward-substitute `x = y` (single-definition copy)
    // to eliminate unnecessary temporaries.
    if copy_propagation_pass(func) {
        // A second cleanup pass to catch newly-exposed dead code.
        defuse_dead_assignment_pass(func);
    }
    // Join-variable coalescing: unify parallel temporaries assigned in both
    // branches of an if-else (SSA out-of-SSA for 2-way joins).
    join_coalescing_pass(func);
    // Type inference: propagate types from typed sub-expressions (Const, Cast,
    // Binary, …) to NirBinding.ty for locals/params that are still Unknown,
    // and re-derive the function return type for `return <var>` patterns.
    apply_type_inference_pass(func);
    // Call-site inter-procedural type propagation: look up callee signatures
    // in the Windows API database and constrain argument / return-value bindings.
    // Runs right after def-driven inference so that the API-derived types
    // become seeds for the subsequent use-driven backward pass.
    if apply_callsite_type_prop_pass(func) {
        apply_type_inference_pass(func);
    }
    // Use-driven backward type propagation: infer pointer/sign types from
    // use-sites (Load/Store/signed comparisons/Return context).  Runs after
    // def-driven inference so its results can seed additional constraints.
    if apply_use_driven_type_infer_pass(func) {
        // A second def-driven pass to pick up any newly-typed variables.
        apply_type_inference_pass(func);
    }
    // Cast elision: remove outer casts that are redundant given the binding's
    // declared type (assignment-context cast: `x = (T)y` where x.ty == T).
    // Runs after type inference so that NirBinding.ty is maximally populated.
    if cast_elision_pass(func) {
        // A light cleanup pass to simplify any newly-exposed dead code.
        defuse_dead_assignment_pass(func);
    }
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
        super::for_loops::apply_for_loop_folding(&mut func.body);
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
            super::for_loops::apply_for_loop_folding(&mut func.body);
            eliminate_dead_local_clobber_assigns(func);
            prune_unused_temp_bindings(func);
            prune_unused_dead_local_bindings(func);
        }
    }
    // Pointer arithmetic recovery: convert IntAdd(ptr, k) → PtrOffset and
    // IntAdd(ptr, idx*stride) → Index after pointer types are established AND
    // after the slot-surfacing pass so the Add(ptr, Mul) pattern remains intact
    // for slot detection.
    if apply_ptr_arith_recovery_pass(func) {
        cleanup_stmt_list(&mut func.body, &func.name, 0);
        defuse_dead_assignment_pass(func);
    }
    // Memory SSA dead store elimination: remove stack-slot stores that are
    // never observed by any subsequent load.  Must run after ptr_arith_recovery
    // so Deref/PtrOffset patterns are normalised, and before aggregate_fields.
    if apply_dead_store_elimination(func) {
        cleanup_stmt_list(&mut func.body, &func.name, 0);
        defuse_dead_assignment_pass(func);
    }
    // Aggregate field layout recovery: collect PtrOffset access offsets on
    // Ptr(Aggregate) variables and annotate the aggregate type with named
    // StructFields.  Must run after ptr_arith_recovery so PtrOffset nodes exist.
    apply_aggregate_fields_pass(func);
    // Single-predecessor label inlining: reduce goto/label pairs by inlining
    // blocks that are targeted by exactly one forward unconditional goto.
    // Runs last so all other structural passes have already had their say.
    if single_pred_label_inline(&mut func.body) {
        cleanup_stmt_list(&mut func.body, &func.name, 0);
        super::for_loops::apply_for_loop_folding(&mut func.body);
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    // Loop IV recovery (SCEV-lite): upgrade While → For for linear induction
    // variables.  Runs after label inlining so the loop body is maximally
    // simplified first.
    if apply_iv_recovery_pass(func) {
        cleanup_stmt_list(&mut func.body, &func.name, 0);
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    // Break/Continue recovery: replace single-predecessor Goto-to-exit-label
    // patterns inside loops with explicit break/continue statements.
    if apply_break_continue_pass(func) {
        cleanup_stmt_list(&mut func.body, &func.name, 0);
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    // Value Set Analysis: use range information to eliminate dead switch
    // cases and constant-condition branches.  Runs last so all structural
    // passes have already simplified the body.
    if apply_jump_resolver_pass(func) {
        cleanup_stmt_list(&mut func.body, &func.name, 0);
        prune_unused_temp_bindings(func);
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
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            if let Some(i) = init {
                normalize_stmt(i);
            }
            if let Some(c) = cond {
                normalize_condition_expr(c);
            }
            if let Some(u) = update {
                normalize_stmt(u);
            }
            for stmt in body {
                normalize_stmt(stmt);
            }
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
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let HirStmt::Block(b) = &mut **i {
                        cleanup_stmt_list(b, func_name, depth + 1);
                    }
                }
                if let Some(u) = update {
                    if let HirStmt::Block(b) = &mut **u {
                        cleanup_stmt_list(b, func_name, depth + 1);
                    }
                }
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
        if simplify_empty_and_constant_ifs(stmts) {
            changed = true;
            last_changed_pass = Some("simplify_empty_and_constant_ifs");
        }
        if simplify_fallthrough_edges(stmts) {
            changed = true;
            last_changed_pass = Some("simplify_fallthrough_edges");
        }
        if fuse_single_predecessor_boundaries(stmts) {
            changed = true;
            last_changed_pass = Some("fuse_single_predecessor_boundaries");
        }
        if promote_guarded_jump_target_tail(stmts) {
            changed = true;
            last_changed_pass = Some("promote_guarded_jump_target_tail");
        }
        if cleanup_redundant_boundary_labels(stmts) {
            changed = true;
            last_changed_pass = Some("cleanup_redundant_boundary_labels");
        }
        if remove_unreferenced_leading_labels(stmts) {
            changed = true;
            last_changed_pass = Some("remove_unreferenced_leading_labels");
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
    // Pre-pass: merge consecutive shifts top-down before child recursion so that
    // Shr(Shr(x, K1), K2) → Shr(x, K1+K2) is visible before any child Shr gets
    // converted to a division by recognize_mod_div_power_of_two.
    if let Some(merged) = merge_consecutive_shifts(expr) {
        *expr = merged;
    }
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
            .or_else(|| recognize_magic_number_division(&current))
            .or_else(|| recognize_hi_lo_extract(&current))
            .or_else(|| simplify_subpiece_chain(&current))
            .or_else(|| merge_consecutive_shifts(&current))
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
