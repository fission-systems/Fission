use super::super::arith::{
    canonicalize_condition_expr, canonicalize_flag_intrinsics, canonicalize_integer_expr,
    cleanup_arithmetic_wrappers, collapse_zero_offset_cast, merge_consecutive_shifts,
    normalize_boolean_logic, recognize_hi_lo_extract, recognize_mod_div_power_of_two,
    recognize_magic_number_division, recognize_wide_integer_recombine, simplify_subpiece_chain,
};
use super::super::idioms::{
    apply_bitstream_idioms, apply_branch_prefix_hoist_pass, remove_callee_save_prologue_epilogue,
};
use super::super::cleanup::{
    collapse_redundant_conditional_returns,
    cast_elision_pass, cleanup_redundant_boundary_labels, collapse_trivial_assign_returns,
    elide_unused_popcount_assigns, eliminate_dead_local_clobber_assigns,
    eliminate_dead_temp_assigns, fuse_single_predecessor_boundaries, inline_single_use_temps,
    promote_guarded_jump_target_tail, prune_unused_dead_local_bindings, prune_unused_temp_bindings,
    remove_unreferenced_leading_labels, simplify_empty_and_constant_ifs,
    simplify_fallthrough_edges,
};
use super::super::global_opt::{
    apply_cse_pass, apply_dead_store_elimination, apply_gvn_join_hoist_pass, apply_licm_pass,
    apply_redundant_load_elimination, apply_sccp_pass,
};
use super::super::analysis::defuse::{
    apply_wide_dead_assignment_pass, constant_folding_pass, defuse_dead_assignment_pass,
};
use super::super::recovery::{
    apply_break_continue_pass, apply_flag_recovery_pass, apply_for_loop_folding,
    apply_iv_recovery_pass, copy_propagation_pass, join_coalescing_pass,
};
use super::super::memory::{
    apply_aggregate_fields_pass, apply_memory_slot_surfacing, apply_memory_slot_surfacing_cheap,
    apply_ptr_arith_recovery_pass, normalize_binding_initializers,
};
use super::super::types::{
    apply_callsite_type_prop_pass, apply_entry_param_promotion_pass,
    apply_interproc_callsite_arity_pass, apply_type_inference_pass,
    apply_use_driven_type_infer_pass, apply_variadic_stack_region_pass,
};
use crate::nir::vsa::apply_jump_resolver_pass;
use super::super::cleanup::single_pred_label_inline;
use super::super::wave_stats;
use super::super::*;
use std::time::Instant;
use tracing::{debug, debug_span};

const TYPE_SIGNATURE_FIXED_POINT_MAX_ROUNDS: usize = 6;

fn apply_type_signature_fixed_point(func: &mut HirFunction, diag: bool, perf: bool) {
    let mut interproc_signature_rounds = 0usize;
    for round in 0..TYPE_SIGNATURE_FIXED_POINT_MAX_ROUNDS {
        let (before_stmts, before_locals) = if perf { hir_shape(func) } else { (0, 0) };
        let round_start = if perf { Some(Instant::now()) } else { None };

        let def_changed =
            run_pass_logged(func, "type_inference", perf, apply_type_inference_pass);
        let callsite_changed = run_pass_logged(
            func,
            "callsite_type_prop",
            perf,
            apply_callsite_type_prop_pass,
        );
        let use_changed = run_pass_logged(
            func,
            "use_driven_type_infer",
            perf,
            apply_use_driven_type_infer_pass,
        );
        let round_changed = def_changed || callsite_changed || use_changed;

        if callsite_changed {
            interproc_signature_rounds += 1;
        }

        if diag {
            eprintln!(
                "[DIAG] normalize type-fp: {} round={} def_changed={} callsite_changed={} use_changed={}",
                func.name,
                round + 1,
                def_changed,
                callsite_changed,
                use_changed,
            );
        }

        if let Some(start) = round_start {
            let (after_stmts, after_locals) = hir_shape(func);
            eprintln!(
                "[PERF] normalize type-fp-round: fn={} round={} changed={} elapsed_ms={:.3} stmts={}=>{} locals={}=>{}",
                func.name,
                round + 1,
                round_changed,
                start.elapsed().as_secs_f64() * 1000.0,
                before_stmts,
                after_stmts,
                before_locals,
                after_locals,
            );
        }

        if !round_changed {
            break;
        }
    }

    if interproc_signature_rounds > 0 {
        wave_stats::add_interproc_constraint_rounds(interproc_signature_rounds);
    }
}

pub(crate) fn normalize_function_body(body: &mut Vec<HirStmt>) {
    cleanup_stmt_list(body, "<body>", 0);
}

pub(crate) fn normalize_hir_function(func: &mut HirFunction) {
    wave_stats::reset_normalize_wave_stats();
    let diag = normalize_diag_enabled();
    let perf = normalize_perf_enabled();
    let total_start = Instant::now();
    let _hir_normalize = debug_span!("hir_normalize", fn_name = %func.name).entered();
    if diag {
        eprintln!(
            "[DIAG] normalize start: {} stmts={} locals={}",
            func.name,
            count_hir_stmts(&func.body),
            func.locals.len()
        );
    }
    run_cleanup_block(func, "cleanup_init_1", perf, |f| {

        normalize_binding_initializers(&mut f.locals);

        cleanup_stmt_list(&mut f.body, &f.name, 0);

        apply_for_loop_folding(&mut f.body);

        eliminate_dead_local_clobber_assigns(f);

        prune_unused_temp_bindings(f);

        prune_unused_dead_local_bindings(f);

    });
    // Flag recovery: substitute raw x86 EFLAGS variable references in branch
    // conditions with high-level comparison expressions (sf!=of → a<b signed,
    // !zf → a!=b, etc.).  Runs early so that subsequent dead-assignment passes
    // can eliminate now-dead flag-variable assignments.
    if run_pass_logged(func, "flag_recovery", perf, apply_flag_recovery_pass) {
        run_cleanup_block(func, "cleanup_defuse_4", perf, |f| {

            cleanup_stmt_list(&mut f.body, &f.name, 0);

            defuse_dead_assignment_pass(f);

            prune_unused_temp_bindings(f);

            prune_unused_dead_local_bindings(f);

        });
    }
    // Parity / popcount dead elimination: remove __popcount-based assignments
    // whose result is not consumed anywhere (e.g., dead parity flag variables
    // remaining after flag recovery or simple unused parity computations).
    if run_pass_logged(func, "elide_unused_popcount", perf, elide_unused_popcount_assigns) {
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    // Prologue/epilogue elimination: remove callee-saved register save/restore
    // pairs (`*spill = r15` / `r15 = *spill`) from the function body.
    if run_pass_logged(
        func,
        "remove_callee_save_prologue_epilogue",
        perf,
        remove_callee_save_prologue_epilogue,
    ) {
        run_cleanup_block(func, "cleanup_defuse_5", perf, |f| {

            cleanup_stmt_list(&mut f.body, &f.name, 0);

            defuse_dead_assignment_pass(f);

            prune_unused_temp_bindings(f);

            prune_unused_dead_local_bindings(f);

        });
    }
    // Run constant folding after the initial cleanup so that folded constants
    // unlock further simplifications in subsequent passes.
    if run_pass_logged(func, "constant_folding", perf, |f| constant_folding_pass(&mut f.body)) {
        run_cleanup_block(func, "cleanup_elim_7", perf, |f| {

            cleanup_stmt_list(&mut f.body, &f.name, 0);

            eliminate_dead_local_clobber_assigns(f);

            prune_unused_temp_bindings(f);

            prune_unused_dead_local_bindings(f);

        });
    }
    // ABI-aware entry spill → param_k promotion (HIR, after early cleanup).
    if run_pass_logged(
        func,
        "entry_param_promotion",
        perf,
        apply_entry_param_promotion_pass,
    ) {
        run_cleanup_block(func, "cleanup_defuse_6", perf, |f| {

            cleanup_stmt_list(&mut f.body, &f.name, 0);

            defuse_dead_assignment_pass(f);

            prune_unused_temp_bindings(f);

            prune_unused_dead_local_bindings(f);

        });
    }
    // SCCP: global sparse constant propagation on structured HIR (lattice merge
    // at joins).  Runs after local constant folding so folded seeds propagate.
    if run_pass_logged(func, "sccp", perf, apply_sccp_pass) {
        cleanup_stmt_list(&mut func.body, &func.name, 0);
        if run_pass_logged(func, "constant_folding_after_sccp", perf, |f| {
            constant_folding_pass(&mut f.body)
        }) {
            run_cleanup_block(func, "cleanup_elim_8", perf, |f| {

                cleanup_stmt_list(&mut f.body, &f.name, 0);

                eliminate_dead_local_clobber_assigns(f);

                prune_unused_temp_bindings(f);

                prune_unused_dead_local_bindings(f);

            });
        }
        run_pass_logged(
            func,
            "wide_dead_assignment",
            perf,
            apply_wide_dead_assignment_pass,
        );
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    // Local CSE: within each linear block, replace identical pure sub-expressions
    // with the variable that first computed them.  Runs right after constant
    // folding so that folded constants are included in the expression map.
    if run_pass_logged(func, "cse", perf, apply_cse_pass) {
        if run_pass_logged(func, "copy_propagation_after_cse", perf, copy_propagation_pass) {
            run_pass_logged(
                func,
                "defuse_dead_assignment_after_cse_copy",
                perf,
                defuse_dead_assignment_pass,
            );
        }
        run_pass_logged(
            func,
            "defuse_dead_assignment_after_cse",
            perf,
            defuse_dead_assignment_pass,
        );
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    // Function-level def-use dead assignment: removes dead writes to ANY
    // variable (not just trivially-named temps) across the whole body tree.
    run_pass_logged(
        func,
        "defuse_dead_assignment",
        perf,
        defuse_dead_assignment_pass,
    );
    // Copy propagation: forward-substitute `x = y` (single-definition copy)
    // to eliminate unnecessary temporaries.
    if run_pass_logged(func, "copy_propagation", perf, copy_propagation_pass) {
        // A second cleanup pass to catch newly-exposed dead code.
        run_pass_logged(
            func,
            "defuse_dead_assignment_after_copy",
            perf,
            defuse_dead_assignment_pass,
        );
    }
    // Join-variable coalescing: unify parallel temporaries assigned in both
    // branches of an if-else (SSA out-of-SSA for 2-way joins).
    run_pass_logged(func, "join_coalescing", perf, join_coalescing_pass);
    // If-else common pure-prefix hoisting: move identical leading assignments
    // out of both branches (partial redundancy elimination for branches).
    if run_pass_logged(
        func,
        "branch_prefix_hoist",
        perf,
        apply_branch_prefix_hoist_pass,
    ) {
        cleanup_stmt_list(&mut func.body, &func.name, 0);
        if run_pass_logged(func, "copy_propagation_after_branch_hoist", perf, copy_propagation_pass)
        {
            run_pass_logged(
                func,
                "defuse_dead_assignment_after_branch_hoist_copy",
                perf,
                defuse_dead_assignment_pass,
            );
        }
        run_pass_logged(
            func,
            "defuse_dead_assignment_after_branch_hoist",
            perf,
            defuse_dead_assignment_pass,
        );
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    // GVN-lite at 2-way joins: duplicate pure RHS, different LHS → hoist temp.
    if run_pass_logged(func, "gvn_join_hoist", perf, apply_gvn_join_hoist_pass) {
        cleanup_stmt_list(&mut func.body, &func.name, 0);
        if run_pass_logged(func, "copy_propagation_after_gvn", perf, copy_propagation_pass) {
            run_pass_logged(
                func,
                "defuse_dead_assignment_after_gvn_copy",
                perf,
                defuse_dead_assignment_pass,
            );
        }
        run_pass_logged(
            func,
            "defuse_dead_assignment_after_gvn",
            perf,
            defuse_dead_assignment_pass,
        );
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    // Module B: run def-driven, callsite-signature, and use-driven inference
    // to convergence (bounded). This avoids one-shot ordering sensitivity.
    apply_type_signature_fixed_point(func, diag, perf);
    // Cast elision: remove outer casts that are redundant given the binding's
    // declared type (assignment-context cast: `x = (T)y` where x.ty == T).
    // Runs after type inference so that NirBinding.ty is maximally populated.
    if run_pass_logged(func, "cast_elision", perf, cast_elision_pass) {
        // A light cleanup pass to simplify any newly-exposed dead code.
        run_pass_logged(
            func,
            "defuse_dead_assignment_after_cast_elision",
            perf,
            defuse_dead_assignment_pass,
        );
    }
    let allow_expensive_passes = !is_large_hir_function(func);
    let mut changed = false;
    changed |= if allow_expensive_passes {
        run_pass_logged(
            func,
            "memory_slot_surfacing_full",
            perf,
            apply_memory_slot_surfacing,
        )
    } else {
        run_pass_logged(
            func,
            "memory_slot_surfacing_cheap",
            perf,
            apply_memory_slot_surfacing_cheap,
        )
    };
    if diag {
        eprintln!(
            "[DIAG] normalize slots: {} changed={} mode={}",
            func.name,
            changed,
            if allow_expensive_passes {
                "full"
            } else {
                "cheap"
            }
        );
    }
    if changed {
        run_cleanup_block(func, "cleanup_init_2", perf, |f| {

            normalize_binding_initializers(&mut f.locals);

            cleanup_stmt_list(&mut f.body, &f.name, 0);

            apply_for_loop_folding(&mut f.body);

            eliminate_dead_local_clobber_assigns(f);

            prune_unused_temp_bindings(f);

            prune_unused_dead_local_bindings(f);

        });
    }
    if allow_expensive_passes {
        changed |= run_pass_logged(func, "bitstream_idioms", perf, apply_bitstream_idioms);
        if diag {
            eprintln!(
                "[DIAG] normalize bitstream: {} changed={}",
                func.name,
                changed,
            );
        }
        if changed {
            run_cleanup_block(func, "cleanup_init_3", perf, |f| {

                normalize_binding_initializers(&mut f.locals);

                cleanup_stmt_list(&mut f.body, &f.name, 0);

                apply_for_loop_folding(&mut f.body);

                eliminate_dead_local_clobber_assigns(f);

                prune_unused_temp_bindings(f);

                prune_unused_dead_local_bindings(f);

            });
        }
    }
    // Pointer arithmetic recovery: convert IntAdd(ptr, k) → PtrOffset and
    // IntAdd(ptr, idx*stride) → Index after pointer types are established AND
    // after the slot-surfacing pass so the Add(ptr, Mul) pattern remains intact
    // for slot detection.
    if run_pass_logged(
        func,
        "ptr_arith_recovery",
        perf,
        apply_ptr_arith_recovery_pass,
    ) {
        run_cleanup_block(func, "cleanup_standalone_12", perf, |f| {

            cleanup_stmt_list(&mut f.body, &f.name, 0);

        });
run_pass_logged(
            func,
            "defuse_dead_assignment_after_ptr_arith",
            perf,
            defuse_dead_assignment_pass,
        );
    }
    // Memory SSA dead store elimination: remove stack-slot stores that are
    // never observed by any subsequent load.  Must run after ptr_arith_recovery
    // so Deref/PtrOffset patterns are normalised, and before aggregate_fields.
    if run_pass_logged(func, "dead_store_elimination", perf, apply_dead_store_elimination) {
        run_cleanup_block(func, "cleanup_standalone_13", perf, |f| {

            cleanup_stmt_list(&mut f.body, &f.name, 0);

        });
run_pass_logged(
            func,
            "defuse_dead_assignment_after_dead_store",
            perf,
            defuse_dead_assignment_pass,
        );
    }
    // Redundant load elimination: reuse the result of an earlier stack-slot load
    // when no intervening store (complements dead-store removal and local CSE).
    if run_pass_logged(
        func,
        "redundant_load_elimination",
        perf,
        apply_redundant_load_elimination,
    ) {
        run_cleanup_block(func, "cleanup_standalone_14", perf, |f| {

            cleanup_stmt_list(&mut f.body, &f.name, 0);

        });
run_pass_logged(
            func,
            "defuse_dead_assignment_after_redundant_load",
            perf,
            defuse_dead_assignment_pass,
        );
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    // Windows x64 stack-tail / variadic region lattice hook (stats; optional future folds).
    let _ = run_pass_logged(
        func,
        "variadic_stack_region",
        perf,
        apply_variadic_stack_region_pass,
    );
    // Aggregate field layout recovery: collect PtrOffset access offsets on
    // Ptr(Aggregate) variables and annotate the aggregate type with named
    // StructFields.  Must run after ptr_arith_recovery so PtrOffset nodes exist.
    run_pass_logged(func, "aggregate_fields", perf, apply_aggregate_fields_pass);
    // Single-predecessor label inlining: reduce goto/label pairs by inlining
    // blocks that are targeted by exactly one forward unconditional goto.
    // Runs last so all other structural passes have already had their say.
    if run_pass_logged(func, "single_pred_label_inline", perf, |f| {
        single_pred_label_inline(&mut f.body)
    }) {
        cleanup_stmt_list(&mut func.body, &func.name, 0);
        apply_for_loop_folding(&mut func.body);
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    // Loop IV recovery (SCEV-lite): upgrade While → For for linear induction
    // variables.  Runs after label inlining so the loop body is maximally
    // simplified first.
    if run_pass_logged(func, "iv_recovery", perf, apply_iv_recovery_pass) {
        run_cleanup_block(func, "cleanup_prune_9", perf, |f| {

            cleanup_stmt_list(&mut f.body, &f.name, 0);

            prune_unused_temp_bindings(f);

            prune_unused_dead_local_bindings(f);

        });
    }
    // Break/Continue recovery: replace single-predecessor Goto-to-exit-label
    // patterns inside loops with explicit break/continue statements.
    if run_pass_logged(func, "break_continue_recovery", perf, apply_break_continue_pass) {
        run_cleanup_block(func, "cleanup_prune_10", perf, |f| {

            cleanup_stmt_list(&mut f.body, &f.name, 0);

            prune_unused_temp_bindings(f);

            prune_unused_dead_local_bindings(f);

        });
    }
    // Loop Invariant Code Motion: hoist pure loop-invariant assignments out of
    // loop bodies (innermost-first).  Runs after break/continue recovery so the
    // loop structure is finalised.
    if run_pass_logged(func, "licm", perf, apply_licm_pass) {
        run_cleanup_block(func, "cleanup_standalone_15", perf, |f| {

            cleanup_stmt_list(&mut f.body, &f.name, 0);

        });
run_pass_logged(
            func,
            "defuse_dead_assignment_after_licm",
            perf,
            defuse_dead_assignment_pass,
        );
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    // Call-site arity lower bounds per callee symbol (intra-proc merge on `HirFunction`).
    let _ = run_pass_logged(
        func,
        "interproc_callsite_arity",
        perf,
        apply_interproc_callsite_arity_pass,
    );
    // Value Set Analysis: use range information to eliminate dead switch
    // cases and constant-condition branches.  Runs last so all structural
    // passes have already simplified the body.
    if run_pass_logged(func, "jump_resolver", perf, apply_jump_resolver_pass) {
        run_cleanup_block(func, "cleanup_prune1_11", perf, |f| {

            cleanup_stmt_list(&mut f.body, &f.name, 0);

            prune_unused_temp_bindings(f);

        });
    }
    if perf {
        let (final_stmts, final_locals) = hir_shape(func);
        eprintln!(
            "[PERF] normalize total: fn={} elapsed_ms={:.3} final_stmts={} final_locals={}",
            func.name,
            total_start.elapsed().as_secs_f64() * 1000.0,
            final_stmts,
            final_locals,
        );
    }
    if diag {
        eprintln!(
            "[DIAG] normalize done: {} total_elapsed={:.3}s",
            func.name,
            total_start.elapsed().as_secs_f64()
        );
    }
}

pub(crate) fn is_large_hir_function(func: &HirFunction) -> bool {
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

fn hir_shape(func: &HirFunction) -> (usize, usize) {
    (count_hir_stmts(&func.body), func.locals.len())
}

fn run_cleanup_block<F>(func: &mut HirFunction, pass_name: &str, perf: bool, mut block: F) -> bool
where
    F: FnMut(&mut HirFunction),
{
    run_pass_logged(func, pass_name, perf, |f| {
        let (before_stmts, before_locals) = hir_shape(f);
        block(f);
        let (after_stmts, after_locals) = hir_shape(f);
        before_stmts != after_stmts || before_locals != after_locals
    })
}

fn run_pass_logged<F>(func: &mut HirFunction, pass_name: &str, perf: bool, pass_fn: F) -> bool
where
    F: FnOnce(&mut HirFunction) -> bool,
{
    let _span = debug_span!("normalize_pass", fn_name = %func.name, pass = pass_name).entered();

    let (before_stmts, before_locals) = hir_shape(func);
    let start = Instant::now();
    let changed = pass_fn(func);
    let (after_stmts, after_locals) = hir_shape(func);
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

    super::super::wave_stats::add_pass_metric(
        pass_name,
        elapsed_ms,
        changed,
        before_stmts,
        after_stmts,
        before_locals,
        after_locals,
    );

    debug!(
        changed,
        elapsed_ms,
        stmts_reduced = (before_stmts as isize - after_stmts as isize),
        locals_reduced = (before_locals as isize - after_locals as isize),
        "pass completed"
    );

    if perf {
        eprintln!(
            "[PERF] normalize pass: fn={} pass={} changed={} elapsed_ms={:.3} stmts={}=>{} locals={}=>{}",
            func.name,
            pass_name,
            changed,
            elapsed_ms,
            before_stmts,
            after_stmts,
            before_locals,
            after_locals,
        );
    }
    changed
}

pub(crate) fn normalize_stmt(stmt: &mut HirStmt) {
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
        if collapse_redundant_conditional_returns(stmts) {
            changed = true;
            last_changed_pass = Some("collapse_redundant_conditional_returns");
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

pub(crate) fn normalize_expr(expr: &mut HirExpr) {
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

fn normalize_perf_enabled() -> bool {
    std::env::var_os("FISSION_PREVIEW_PERF").is_some()
}
