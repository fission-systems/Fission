use super::super::analysis::defuse::{
    apply_wide_dead_assignment_pass, constant_folding_pass, defuse_dead_assignment_pass,
    stabilize_repeated_pure_exprs,
};
use super::super::analysis::preservation::preserved_materialization_names;
use super::super::arith::{
    canonicalize_condition_expr, canonicalize_flag_intrinsics, canonicalize_integer_expr,
    cleanup_arithmetic_wrappers, collapse_zero_offset_cast, merge_consecutive_shifts,
    normalize_boolean_logic, recognize_compiler_runtime_division, recognize_hi_lo_extract,
    recognize_magic_number_division, recognize_mod_div_power_of_two,
    recognize_wide_integer_recombine, simplify_subpiece_chain,
};
use super::super::cleanup::single_pred_label_inline;
use super::super::cleanup::{
    cast_elision_pass, cleanup_redundant_boundary_labels, collapse_redundant_conditional_returns,
    collapse_trivial_assign_returns, elide_unused_popcount_assigns,
    eliminate_dead_local_clobber_assigns, eliminate_dead_temp_assigns,
    fuse_single_predecessor_boundaries, inline_single_use_temps, promote_guarded_jump_target_tail,
    prune_unused_dead_local_bindings, prune_unused_temp_bindings,
    remove_unreferenced_leading_labels, simplify_empty_and_constant_ifs,
    simplify_fallthrough_edges,
};
use super::super::global_opt::{
    apply_cse_pass, apply_dead_store_elimination, apply_gvn_join_hoist_pass, apply_licm_pass,
    apply_post_assign_value_representative_pass, apply_redundant_load_elimination, apply_sccp_pass,
};
use super::super::idioms::{
    apply_bitstream_idioms, apply_branch_prefix_hoist_pass, apply_call_artifact_cleanup_pass,
    apply_security_cookie_pass, remove_callee_save_prologue_epilogue,
    remove_entry_stack_scaffold_stores,
};
use super::super::memory::{
    apply_aggregate_fields_pass, apply_memory_slot_surfacing, apply_memory_slot_surfacing_cheap,
    apply_ptr_arith_recovery_pass, normalize_binding_initializers,
};
use super::super::recovery::{
    apply_break_continue_pass, apply_flag_recovery_pass, apply_for_loop_folding,
    apply_iv_recovery_pass, copy_propagation_pass, join_coalescing_pass,
};
use super::super::types::{
    apply_callsite_type_prop_pass, apply_entry_param_promotion_pass,
    apply_interproc_callsite_arity_pass, apply_type_inference_pass,
    apply_use_driven_type_infer_pass, apply_variadic_stack_region_pass,
};
use super::super::wave_stats;
use super::super::*;
use crate::nir::vsa::{apply_jump_resolver_pass, jump_resolver_candidate_count};
use std::time::Instant;
use tracing::{debug, debug_span};

const TYPE_SIGNATURE_FIXED_POINT_MAX_ROUNDS: usize = 6;
const EARLY_CLEANUP_BLOCK_STMT_LIMIT: usize = 2000;
const EARLY_CLEANUP_BLOCK_BLOCK_LIMIT: usize = 300;

#[derive(Debug, Clone, Copy)]
struct PassBudget {
    stmt_limit: usize,
    block_limit: usize,
    round_limit: usize,
}

impl PassBudget {
    fn allows_body_cleanup(self, stmt_count: usize, block_count: usize) -> bool {
        stmt_count <= self.stmt_limit && block_count <= self.block_limit
    }
}

fn apply_type_signature_fixed_point(func: &mut HirFunction, diag: bool, perf: bool) {
    let mut interproc_signature_rounds = 0usize;
    for round in 0..TYPE_SIGNATURE_FIXED_POINT_MAX_ROUNDS {
        let (before_stmts, before_locals) = if perf { hir_shape(func) } else { (0, 0) };
        let round_start = if perf { Some(Instant::now()) } else { None };

        let def_changed = run_pass_logged(func, "type_inference", perf, apply_type_inference_pass);
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

fn cleanup_func_stmt_list(func: &mut HirFunction) {
    let preserved_temps = preserved_materialization_names(&func.locals);
    cleanup_stmt_list_with_options_and_preserved(
        &mut func.body,
        &func.name,
        0,
        CleanupStmtOptions {
            include_boundary_labels: true,
            round_limit: 16,
        },
        &preserved_temps,
    );
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
    run_cleanup_family_passes(
        func,
        "init_1",
        perf,
        PassBudget {
            stmt_limit: 600,
            block_limit: 120,
            round_limit: 12,
        },
    );
    // Flag recovery: substitute raw x86 EFLAGS variable references in branch
    // conditions with high-level comparison expressions (sf!=of → a<b signed,
    // !zf → a!=b, etc.).  Runs early so that subsequent dead-assignment passes
    // can eliminate now-dead flag-variable assignments.
    if run_pass_logged(func, "flag_recovery", perf, apply_flag_recovery_pass) {
        run_cleanup_block(func, "cleanup_defuse_4", perf, |f| {
            cleanup_func_stmt_list(f);

            defuse_dead_assignment_pass(f);

            prune_unused_temp_bindings(f);

            prune_unused_dead_local_bindings(f);
        });
    }
    // Parity / popcount dead elimination: remove __popcount-based assignments
    // whose result is not consumed anywhere (e.g., dead parity flag variables
    // remaining after flag recovery or simple unused parity computations).
    if run_pass_logged(
        func,
        "elide_unused_popcount",
        perf,
        elide_unused_popcount_assigns,
    ) {
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    // Prologue/epilogue elimination: remove callee-saved register save/restore
    // pairs (`*spill = r15` / `r15 = *spill`) from the function body.
    if run_pass_logged(
        func,
        "remove_entry_stack_scaffold_stores",
        perf,
        remove_entry_stack_scaffold_stores,
    ) {
        run_cleanup_block(func, "cleanup_defuse_entry_stack_scaffold", perf, |f| {
            cleanup_func_stmt_list(f);

            defuse_dead_assignment_pass(f);

            prune_unused_temp_bindings(f);

            prune_unused_dead_local_bindings(f);
        });
    }
    if run_pass_logged(
        func,
        "remove_callee_save_prologue_epilogue",
        perf,
        remove_callee_save_prologue_epilogue,
    ) {
        run_cleanup_block(func, "cleanup_defuse_5", perf, |f| {
            cleanup_func_stmt_list(f);

            defuse_dead_assignment_pass(f);

            prune_unused_temp_bindings(f);

            prune_unused_dead_local_bindings(f);
        });
    }
    let _ = run_pass_logged(
        func,
        "call_artifact_cleanup",
        perf,
        apply_call_artifact_cleanup_pass,
    );
    let _ = run_pass_logged(func, "security_cookie", perf, apply_security_cookie_pass);
    // Run constant folding after the initial cleanup so that folded constants
    // unlock further simplifications in subsequent passes.
    if run_pass_logged(func, "constant_folding", perf, |f| {
        constant_folding_pass(&mut f.body)
    }) {
        run_cleanup_block(func, "cleanup_elim_7", perf, |f| {
            cleanup_func_stmt_list(f);

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
            cleanup_func_stmt_list(f);

            defuse_dead_assignment_pass(f);

            prune_unused_temp_bindings(f);

            prune_unused_dead_local_bindings(f);
        });
    }
    // SCCP: global sparse constant propagation on structured HIR (lattice merge
    // at joins).  Runs after local constant folding so folded seeds propagate.
    let sccp_admission = sccp_admission_summary(&func.body);
    if !sccp_admission.eligible {
        wave_stats::add_sccp_skipped_by_admission(1);
    }
    if sccp_admission.eligible && run_pass_logged(func, "sccp", perf, apply_sccp_pass) {
        cleanup_func_stmt_list(func);
        if run_pass_logged(func, "constant_folding_after_sccp", perf, |f| {
            constant_folding_pass(&mut f.body)
        }) {
            run_cleanup_block(func, "cleanup_elim_8", perf, |f| {
                cleanup_func_stmt_list(f);

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
        if run_pass_logged(
            func,
            "copy_propagation_after_cse",
            perf,
            copy_propagation_pass,
        ) {
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
        cleanup_func_stmt_list(func);
        if run_pass_logged(
            func,
            "copy_propagation_after_branch_hoist",
            perf,
            copy_propagation_pass,
        ) {
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
        cleanup_func_stmt_list(func);
        if run_pass_logged(
            func,
            "copy_propagation_after_gvn",
            perf,
            copy_propagation_pass,
        ) {
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
            apply_wide_dead_assignment_pass,
        );
    }
    if run_pass_logged(
        func,
        "remove_entry_stack_scaffold_stores_late",
        perf,
        remove_entry_stack_scaffold_stores,
    ) {
        run_cleanup_block(
            func,
            "cleanup_defuse_entry_stack_scaffold_late",
            perf,
            |f| {
                cleanup_func_stmt_list(f);

                defuse_dead_assignment_pass(f);

                prune_unused_temp_bindings(f);

                prune_unused_dead_local_bindings(f);
            },
        );
    }
    let allow_expensive_passes = !is_large_hir_function(func);
    let has_loopish_control = body_has_loopish_shapes(&func.body);
    let memory_fact_prefilter = memory_fact_prefilter_allows_full(func) && !has_loopish_control;
    let slot_changed = if !memory_fact_prefilter {
        wave_stats::add_memory_fact_prefilter_skip(1);
        wave_stats::add_memory_slot_cheap_exit(1);
        false
    } else if allow_expensive_passes {
        run_pass_logged(
            func,
            "memory_slot_surfacing_full",
            perf,
            apply_memory_slot_surfacing,
        )
    } else {
        wave_stats::add_memory_slot_cheap_exit(1);
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
            slot_changed,
            if allow_expensive_passes {
                "full"
            } else {
                "cheap"
            }
        );
    }
    if slot_changed {
        run_cleanup_family_passes(
            func,
            "init_2",
            perf,
            PassBudget {
                stmt_limit: 600,
                block_limit: 120,
                round_limit: 12,
            },
        );
    }
    let bitstream_changed = if allow_expensive_passes {
        run_pass_logged(func, "bitstream_idioms", perf, apply_bitstream_idioms)
    } else {
        false
    };
    if allow_expensive_passes {
        if diag {
            eprintln!(
                "[DIAG] normalize bitstream: {} changed={}",
                func.name, bitstream_changed,
            );
        }
        if bitstream_changed {
            run_cleanup_family_passes(
                func,
                "init_3",
                perf,
                PassBudget {
                    stmt_limit: 600,
                    block_limit: 120,
                    round_limit: 12,
                },
            );
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
            cleanup_func_stmt_list(f);
        });
        run_pass_logged(
            func,
            "defuse_dead_assignment_after_ptr_arith",
            perf,
            apply_wide_dead_assignment_pass,
        );
    }
    if run_pass_logged(
        func,
        "post_assign_value_representative",
        perf,
        apply_post_assign_value_representative_pass,
    ) {
        run_pass_logged(
            func,
            "constant_folding_after_post_assign_value_representative",
            perf,
            |f| constant_folding_pass(&mut f.body),
        );
        run_pass_logged(
            func,
            "defuse_dead_assignment_after_post_assign_value_representative",
            perf,
            apply_wide_dead_assignment_pass,
        );
    }
    // Memory SSA dead store elimination: remove stack-slot stores that are
    // never observed by any subsequent load.  Must run after ptr_arith_recovery
    // so Deref/PtrOffset patterns are normalised, and before aggregate_fields.
    if run_pass_logged(
        func,
        "dead_store_elimination",
        perf,
        apply_dead_store_elimination,
    ) {
        run_cleanup_block(func, "cleanup_standalone_13", perf, |f| {
            cleanup_func_stmt_list(f);
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
            cleanup_func_stmt_list(f);
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
    // Aggregate field layout recovery: collect partitioned access offsets on
    // pointer-like objects and annotate Ptr(Aggregate) bindings with named
    // StructFields.  Re-run pointer arithmetic afterward so newly inferred
    // aggregate pointers expose field offsets before rendering.
    if memory_fact_prefilter {
        if run_pass_logged(func, "aggregate_fields", perf, apply_aggregate_fields_pass) {
            run_pass_logged(
                func,
                "ptr_arith_recovery_after_aggregate_fields",
                perf,
                apply_ptr_arith_recovery_pass,
            );
        }
    } else {
        wave_stats::add_memory_fact_prefilter_skip(1);
        wave_stats::add_aggregate_fields_skipped_by_admission(1);
    }
    // Single-predecessor label inlining: reduce goto/label pairs by inlining
    // blocks that are targeted by exactly one forward unconditional goto.
    // Runs last so all other structural passes have already had their say.
    if run_pass_logged(func, "single_pred_label_inline", perf, |f| {
        single_pred_label_inline(&mut f.body)
    }) {
        cleanup_func_stmt_list(func);
        apply_for_loop_folding(&mut func.body);
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    // Loop IV recovery (SCEV-lite): upgrade While → For for linear induction
    // variables.  Runs after label inlining so the loop body is maximally
    // simplified first.
    if run_pass_logged(func, "iv_recovery", perf, apply_iv_recovery_pass) {
        run_cleanup_block(func, "cleanup_prune_9", perf, |f| {
            cleanup_func_stmt_list(f);

            prune_unused_temp_bindings(f);

            prune_unused_dead_local_bindings(f);
        });
    }
    // Break/Continue recovery: replace single-predecessor Goto-to-exit-label
    // patterns inside loops with explicit break/continue statements.
    if run_pass_logged(
        func,
        "break_continue_recovery",
        perf,
        apply_break_continue_pass,
    ) {
        run_cleanup_block(func, "cleanup_prune_10", perf, |f| {
            cleanup_func_stmt_list(f);

            prune_unused_temp_bindings(f);

            prune_unused_dead_local_bindings(f);
        });
    }
    // Loop Invariant Code Motion: hoist pure loop-invariant assignments out of
    // loop bodies (innermost-first).  Runs after break/continue recovery so the
    // loop structure is finalised.
    if run_pass_logged(func, "licm", perf, apply_licm_pass) {
        run_cleanup_block(func, "cleanup_standalone_15", perf, |f| {
            cleanup_func_stmt_list(f);
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
    let jump_resolver_admission = jump_resolver_admission(&func.body);
    if jump_resolver_admission.eligible {
        if jump_resolver_admission.candidate_scoped {
            wave_stats::add_candidate_scoped_jump_resolver_count(1);
        }
    }
    if jump_resolver_admission.eligible
        && run_pass_logged(func, "jump_resolver", perf, apply_jump_resolver_pass)
    {
        run_cleanup_block(func, "cleanup_prune1_11", perf, |f| {
            cleanup_func_stmt_list(f);

            prune_unused_temp_bindings(f);
        });
    }
    let stabilized_materializations = stabilize_repeated_pure_exprs(func);
    if stabilized_materializations > 0 {
        wave_stats::add_materialization_stabilized(stabilized_materializations);
        run_pass_logged(func, "proof_fidelity_materialization", perf, |_f| true);
    } else {
        wave_stats::add_pass_rerun_skipped_by_preservation(1);
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

fn body_exceeds_early_cleanup_budget(body: &[HirStmt]) -> bool {
    count_hir_stmts(body) > EARLY_CLEANUP_BLOCK_STMT_LIMIT
        || count_hir_blocks(body) > EARLY_CLEANUP_BLOCK_BLOCK_LIMIT
}

#[derive(Debug, Clone, Copy)]
struct JumpResolverAdmission {
    eligible: bool,
    candidate_scoped: bool,
}

#[derive(Debug, Clone, Copy)]
struct SccpAdmissionSummary {
    eligible: bool,
}

fn jump_resolver_admission(body: &[HirStmt]) -> JumpResolverAdmission {
    if !body_exceeds_early_cleanup_budget(body) {
        return JumpResolverAdmission {
            eligible: true,
            candidate_scoped: false,
        };
    }
    let candidate_count = jump_resolver_candidate_count(body);
    JumpResolverAdmission {
        eligible: candidate_count > 0 && candidate_count <= 16,
        candidate_scoped: candidate_count > 0 && candidate_count <= 16,
    }
}

fn sccp_admission_summary(body: &[HirStmt]) -> SccpAdmissionSummary {
    let has_control_seed = body_has_sccp_control_seed(body);
    let has_const_seed = body_has_sccp_const_seed(body);
    SccpAdmissionSummary {
        eligible: has_control_seed && has_const_seed,
    }
}

fn memory_fact_prefilter_allows_full(func: &HirFunction) -> bool {
    func.params
        .iter()
        .chain(func.locals.iter())
        .any(|binding| matches!(binding.ty, NirType::Ptr(_)))
        || body_has_memory_surface_interest(&func.body)
}

fn body_has_sccp_control_seed(body: &[HirStmt]) -> bool {
    body.iter().any(stmt_has_sccp_control_seed)
}

fn stmt_has_sccp_control_seed(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::If { .. } | HirStmt::Switch { .. } => true,
        HirStmt::Block(body) => body_has_sccp_control_seed(body),
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            expr_contains_const(cond) || body_has_sccp_control_seed(body)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_has_sccp_const_seed)
                || cond.as_ref().is_some_and(expr_contains_const)
                || update.as_deref().is_some_and(stmt_has_sccp_const_seed)
                || body_has_sccp_control_seed(body)
        }
        _ => false,
    }
}

fn body_has_sccp_const_seed(body: &[HirStmt]) -> bool {
    body.iter().any(stmt_has_sccp_const_seed)
}

fn stmt_has_sccp_const_seed(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Assign { rhs, .. } | HirStmt::Expr(rhs) | HirStmt::Return(Some(rhs)) => {
            expr_contains_const(rhs)
        }
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_contains_const(cond)
                || body_has_sccp_const_seed(then_body)
                || body_has_sccp_const_seed(else_body)
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_contains_const(expr)
                || cases
                    .iter()
                    .any(|case| body_has_sccp_const_seed(&case.body))
                || body_has_sccp_const_seed(default)
        }
        HirStmt::Block(body) => body_has_sccp_const_seed(body),
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            expr_contains_const(cond) || body_has_sccp_const_seed(body)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref().is_some_and(stmt_has_sccp_const_seed)
                || cond.as_ref().is_some_and(expr_contains_const)
                || update.as_deref().is_some_and(stmt_has_sccp_const_seed)
                || body_has_sccp_const_seed(body)
        }
        HirStmt::VaStart { .. }
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue
        | HirStmt::Label(_)
        | HirStmt::Goto(_) => false,
    }
}

fn body_has_memory_surface_interest(body: &[HirStmt]) -> bool {
    body.iter().any(stmt_has_memory_surface_interest)
}

fn stmt_has_memory_surface_interest(stmt: &HirStmt) -> bool {
    match stmt {
        HirStmt::Assign { lhs, rhs } => {
            lvalue_has_memory_surface_interest(lhs) || expr_has_memory_surface_interest(rhs)
        }
        HirStmt::Expr(expr) | HirStmt::Return(Some(expr)) => expr_has_memory_surface_interest(expr),
        HirStmt::If {
            cond,
            then_body,
            else_body,
        } => {
            expr_has_memory_surface_interest(cond)
                || body_has_memory_surface_interest(then_body)
                || body_has_memory_surface_interest(else_body)
        }
        HirStmt::Switch {
            expr,
            cases,
            default,
        } => {
            expr_has_memory_surface_interest(expr)
                || cases
                    .iter()
                    .any(|case| body_has_memory_surface_interest(&case.body))
                || body_has_memory_surface_interest(default)
        }
        HirStmt::Block(body) => body_has_memory_surface_interest(body),
        HirStmt::While { cond, body } | HirStmt::DoWhile { body, cond } => {
            expr_has_memory_surface_interest(cond) || body_has_memory_surface_interest(body)
        }
        HirStmt::For {
            init,
            cond,
            update,
            body,
        } => {
            init.as_deref()
                .is_some_and(stmt_has_memory_surface_interest)
                || cond.as_ref().is_some_and(expr_has_memory_surface_interest)
                || update
                    .as_deref()
                    .is_some_and(stmt_has_memory_surface_interest)
                || body_has_memory_surface_interest(body)
        }
        HirStmt::VaStart { .. }
        | HirStmt::Return(None)
        | HirStmt::Break
        | HirStmt::Continue
        | HirStmt::Label(_)
        | HirStmt::Goto(_) => false,
    }
}

fn lvalue_has_memory_surface_interest(lhs: &HirLValue) -> bool {
    match lhs {
        HirLValue::Var(_) => false,
        HirLValue::Deref { ptr, .. } => {
            let _ = ptr;
            true
        }
        HirLValue::Index { base, index, .. } => {
            let _ = (base, index);
            true
        }
    }
}

fn expr_has_memory_surface_interest(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Const(_, _) | HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => false,
        HirExpr::Load { ptr, .. } => {
            let _ = ptr;
            true
        }
        HirExpr::PtrOffset { base, .. } => {
            let _ = base;
            true
        }
        HirExpr::Index { base, index, .. } => {
            let _ = (base, index);
            true
        }
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => expr_has_memory_surface_interest(expr),
        HirExpr::Binary { lhs, rhs, .. } => {
            expr_has_memory_surface_interest(lhs) || expr_has_memory_surface_interest(rhs)
        }
        HirExpr::Call { args, .. } => args.iter().any(expr_has_memory_surface_interest),
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_has_memory_surface_interest(cond)
                || expr_has_memory_surface_interest(then_expr)
                || expr_has_memory_surface_interest(else_expr)
        }
    }
}

fn expr_contains_const(expr: &HirExpr) -> bool {
    match expr {
        HirExpr::Const(_, _) => true,
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) => false,
        HirExpr::Cast { expr, .. }
        | HirExpr::Unary { expr, .. }
        | HirExpr::Load { ptr: expr, .. }
        | HirExpr::PtrOffset { base: expr, .. }
        | HirExpr::AggregateCopy { src: expr, .. } => expr_contains_const(expr),
        HirExpr::Binary { lhs, rhs, .. } => expr_contains_const(lhs) || expr_contains_const(rhs),
        HirExpr::Call { args, .. } => args.iter().any(expr_contains_const),
        HirExpr::Index { base, index, .. } => {
            expr_contains_const(base) || expr_contains_const(index)
        }
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_contains_const(cond)
                || expr_contains_const(then_expr)
                || expr_contains_const(else_expr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_func() -> HirFunction {
        HirFunction {
            name: "admission".to_string(),
            params: Vec::new(),
            locals: Vec::new(),
            return_type: NirType::Unknown,
            surface_return_type_name: None,
            body: Vec::new(),
            calling_convention: Default::default(),
            is_64bit: true,
            suppress_entry_register_params: false,
            callee_observed_max_arity: Default::default(),
            callee_summaries: Default::default(),
        }
    }

    #[test]
    fn sccp_admission_rejects_linear_body_without_control_seed() {
        let body = vec![HirStmt::Assign {
            lhs: HirLValue::Var("xVar0".to_string()),
            rhs: HirExpr::Const(
                1,
                NirType::Int {
                    bits: 32,
                    signed: false,
                },
            ),
        }];
        assert!(!sccp_admission_summary(&body).eligible);
    }

    #[test]
    fn sccp_admission_accepts_if_with_const_guard() {
        let body = vec![HirStmt::If {
            cond: HirExpr::Binary {
                op: HirBinaryOp::Eq,
                lhs: Box::new(HirExpr::Var("uVar0".to_string())),
                rhs: Box::new(HirExpr::Const(
                    1,
                    NirType::Int {
                        bits: 32,
                        signed: false,
                    },
                )),
                ty: NirType::Bool,
            },
            then_body: vec![HirStmt::Return(None)],
            else_body: Vec::new(),
        }];
        assert!(sccp_admission_summary(&body).eligible);
    }

    #[test]
    fn memory_fact_prefilter_rejects_non_pointer_function() {
        let mut func = empty_func();
        func.body.push(HirStmt::Return(None));
        assert!(!memory_fact_prefilter_allows_full(&func));
    }

    #[test]
    fn memory_fact_prefilter_accepts_pointer_param() {
        let mut func = empty_func();
        func.params.push(NirBinding {
            name: "param_1".to_string(),
            ty: NirType::Ptr(Box::new(NirType::Unknown)),
            surface_type_name: None,
            origin: Some(NirBindingOrigin::ParamIndex(0)),
            initializer: None,
        });
        assert!(memory_fact_prefilter_allows_full(&func));
    }
}

fn run_cleanup_block<F>(func: &mut HirFunction, pass_name: &str, perf: bool, mut block: F) -> bool
where
    F: FnMut(&mut HirFunction),
{
    if body_exceeds_early_cleanup_budget(&func.body) {
        wave_stats::add_cleanup_budget_skips(1);
        return false;
    }

    run_pass_logged(func, pass_name, perf, |f| {
        let (before_stmts, before_locals) = hir_shape(f);
        block(f);
        let (after_stmts, after_locals) = hir_shape(f);
        before_stmts != after_stmts || before_locals != after_locals
    })
}

fn run_cleanup_family_passes(
    func: &mut HirFunction,
    stage: &str,
    perf: bool,
    budget: PassBudget,
) -> bool {
    let mut changed = false;
    let body_stmt_count = count_hir_stmts(&func.body);
    let body_block_count = count_hir_blocks(&func.body);
    let within_body_budget = budget.allows_body_cleanup(body_stmt_count, body_block_count);

    if has_binding_initializers(&func.locals) {
        wave_stats::add_cleanup_family_binding_init(1);
        changed |= run_pass_logged(func, &format!("cleanup_binding_init_{stage}"), perf, |f| {
            let before = collect_initializer_fingerprints(&f.locals);
            normalize_binding_initializers(&mut f.locals);
            before != collect_initializer_fingerprints(&f.locals)
        });
    } else {
        wave_stats::add_cleanup_budget_skips(1);
    }

    if !func.body.is_empty() {
        wave_stats::add_cleanup_family_stmt_canonical(1);
        if body_has_conditional_return_shapes(&func.body) {
            changed |= run_pass_logged(
                func,
                &format!("cleanup_stmt_fold_conditional_return_{stage}"),
                perf,
                |f| collapse_redundant_conditional_returns_recursive(&mut f.body),
            );
        }
        if within_body_budget && body_needs_stmt_fold_cleanup(&func.body) {
            wave_stats::add_cleanup_stmt_fold(1);
            changed |= run_pass_logged(func, &format!("cleanup_stmt_fold_{stage}"), perf, |f| {
                let before = hir_shape(f);
                cleanup_stmt_list_with_options(
                    &mut f.body,
                    &f.name,
                    0,
                    CleanupStmtOptions {
                        include_boundary_labels: false,
                        round_limit: budget.round_limit,
                    },
                );
                before != hir_shape(f)
            });
        } else if body_needs_stmt_fold_cleanup(&func.body) {
            wave_stats::add_cleanup_budget_skips(1);
        }

        if body_has_boundary_label_shapes(&func.body) {
            wave_stats::add_cleanup_boundary_label(1);
            changed |= run_pass_logged(
                func,
                &format!("cleanup_boundary_label_{stage}"),
                perf,
                |f| cleanup_boundary_labels_recursive(&mut f.body),
            );
        } else {
            wave_stats::add_cleanup_budget_skips(1);
        }

        if within_body_budget && body_has_loopish_shapes(&func.body) {
            wave_stats::add_cleanup_loopish_rewrite(1);
            changed |= run_pass_logged(
                func,
                &format!("cleanup_loopish_rewrite_{stage}"),
                perf,
                |f| apply_for_loop_folding(&mut f.body),
            );
        } else if body_has_loopish_shapes(&func.body) {
            wave_stats::add_cleanup_budget_skips(1);
        }
    } else {
        wave_stats::add_cleanup_budget_skips(1);
    }

    if !func.locals.is_empty() && within_body_budget {
        wave_stats::add_cleanup_family_dead_binding(1);
        changed |= run_pass_logged(func, &format!("cleanup_dead_binding_{stage}"), perf, |f| {
            let before = hir_shape(f);
            eliminate_dead_local_clobber_assigns(f);
            apply_wide_dead_assignment_pass(f);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
            before != hir_shape(f)
        });
    } else if !func.locals.is_empty() {
        wave_stats::add_cleanup_budget_skips(1);
    }

    changed
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

fn has_binding_initializers(bindings: &[NirBinding]) -> bool {
    bindings.iter().any(|binding| binding.initializer.is_some())
}

fn collect_initializer_fingerprints(bindings: &[NirBinding]) -> Vec<(String, Option<String>)> {
    bindings
        .iter()
        .map(|binding| {
            (
                binding.name.clone(),
                binding.initializer.as_ref().map(print_expr),
            )
        })
        .collect()
}

fn body_has_loopish_shapes(stmts: &[HirStmt]) -> bool {
    for stmt in stmts {
        match stmt {
            HirStmt::While { .. } | HirStmt::DoWhile { .. } | HirStmt::For { .. } => return true,
            HirStmt::Block(body) => {
                if body_has_loopish_shapes(body) {
                    return true;
                }
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if body_has_loopish_shapes(then_body) || body_has_loopish_shapes(else_body) {
                    return true;
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                if cases.iter().any(|case| body_has_loopish_shapes(&case.body))
                    || body_has_loopish_shapes(default)
                {
                    return true;
                }
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
    false
}

fn body_has_boundary_label_shapes(stmts: &[HirStmt]) -> bool {
    for stmt in stmts {
        match stmt {
            HirStmt::Label(_) | HirStmt::Goto(_) => return true,
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                if body_has_boundary_label_shapes(body) {
                    return true;
                }
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                if body_has_boundary_label_shapes(then_body)
                    || body_has_boundary_label_shapes(else_body)
                {
                    return true;
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                if cases
                    .iter()
                    .any(|case| body_has_boundary_label_shapes(&case.body))
                    || body_has_boundary_label_shapes(default)
                {
                    return true;
                }
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
    false
}

fn body_has_conditional_return_shapes(stmts: &[HirStmt]) -> bool {
    for stmt in stmts {
        match stmt {
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                let then_ret = then_body
                    .last()
                    .is_some_and(|stmt| matches!(stmt, HirStmt::Return(_)));
                let else_ret = else_body
                    .last()
                    .is_some_and(|stmt| matches!(stmt, HirStmt::Return(_)));
                if (then_ret && else_ret)
                    || body_has_conditional_return_shapes(then_body)
                    || body_has_conditional_return_shapes(else_body)
                {
                    return true;
                }
            }
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                if body_has_conditional_return_shapes(body) {
                    return true;
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                if cases
                    .iter()
                    .any(|case| body_has_conditional_return_shapes(&case.body))
                    || body_has_conditional_return_shapes(default)
                {
                    return true;
                }
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
    false
}

fn body_needs_stmt_fold_cleanup(stmts: &[HirStmt]) -> bool {
    for stmt in stmts {
        match stmt {
            HirStmt::Assign {
                lhs: HirLValue::Var(name),
                ..
            } if looks_like_trivial_temp_name(name) => return true,
            HirStmt::Return(Some(HirExpr::Var(name))) if looks_like_trivial_temp_name(name) => {
                return true;
            }
            HirStmt::If {
                cond,
                then_body,
                else_body,
            } => {
                if matches!(cond, HirExpr::Const(_, _))
                    || then_body.is_empty()
                    || else_body.is_empty()
                    || body_needs_stmt_fold_cleanup(then_body)
                    || body_needs_stmt_fold_cleanup(else_body)
                {
                    return true;
                }
            }
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                if body_needs_stmt_fold_cleanup(body) {
                    return true;
                }
            }
            HirStmt::Switch { cases, default, .. } => {
                if cases
                    .iter()
                    .any(|case| body_needs_stmt_fold_cleanup(&case.body))
                    || body_needs_stmt_fold_cleanup(default)
                {
                    return true;
                }
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
    false
}

fn looks_like_trivial_temp_name(name: &str) -> bool {
    name == "result"
        || name == "retval"
        || name.starts_with("uVar")
        || name.starts_with("iVar")
        || name.starts_with("xVar")
        || name.starts_with("bVar")
}

fn count_hir_blocks(stmts: &[HirStmt]) -> usize {
    fn count_stmt(stmt: &HirStmt) -> usize {
        match stmt {
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => 1 + count_hir_blocks(body),
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => 1 + count_hir_blocks(then_body) + count_hir_blocks(else_body),
            HirStmt::Switch { cases, default, .. } => {
                1 + cases
                    .iter()
                    .map(|case| count_hir_blocks(&case.body))
                    .sum::<usize>()
                    + count_hir_blocks(default)
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => 0,
        }
    }

    stmts.iter().map(count_stmt).sum()
}

pub(crate) fn normalize_stmt(stmt: &mut HirStmt) {
    match stmt {
        HirStmt::Assign { rhs, .. } => normalize_expr(rhs),
        HirStmt::VaStart { va_list, .. } => normalize_expr(va_list),
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

#[derive(Debug, Clone, Copy)]
struct CleanupStmtOptions {
    include_boundary_labels: bool,
    round_limit: usize,
}

fn cleanup_stmt_list(stmts: &mut Vec<HirStmt>, func_name: &str, depth: usize) {
    let preserved_temps = HashSet::new();
    cleanup_stmt_list_with_options_and_preserved(
        stmts,
        func_name,
        depth,
        CleanupStmtOptions {
            include_boundary_labels: true,
            round_limit: 16,
        },
        &preserved_temps,
    );
}

fn cleanup_stmt_list_with_options(
    stmts: &mut Vec<HirStmt>,
    func_name: &str,
    depth: usize,
    options: CleanupStmtOptions,
) {
    let preserved_temps = HashSet::new();
    cleanup_stmt_list_with_options_and_preserved(
        stmts,
        func_name,
        depth,
        options,
        &preserved_temps,
    );
}

fn cleanup_stmt_list_with_options_and_preserved(
    stmts: &mut Vec<HirStmt>,
    func_name: &str,
    depth: usize,
    options: CleanupStmtOptions,
    preserved_temps: &HashSet<String>,
) {
    for stmt in stmts.iter_mut() {
        normalize_stmt(stmt);
        match stmt {
            HirStmt::Block(body) | HirStmt::While { body, .. } | HirStmt::DoWhile { body, .. } => {
                cleanup_stmt_list_with_options_and_preserved(
                    body,
                    func_name,
                    depth + 1,
                    options,
                    preserved_temps,
                )
            }
            HirStmt::For {
                init, update, body, ..
            } => {
                if let Some(i) = init {
                    if let HirStmt::Block(b) = &mut **i {
                        cleanup_stmt_list_with_options_and_preserved(
                            b,
                            func_name,
                            depth + 1,
                            options,
                            preserved_temps,
                        );
                    }
                }
                if let Some(u) = update {
                    if let HirStmt::Block(b) = &mut **u {
                        cleanup_stmt_list_with_options_and_preserved(
                            b,
                            func_name,
                            depth + 1,
                            options,
                            preserved_temps,
                        );
                    }
                }
                cleanup_stmt_list_with_options_and_preserved(
                    body,
                    func_name,
                    depth + 1,
                    options,
                    preserved_temps,
                )
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                cleanup_stmt_list_with_options_and_preserved(
                    then_body,
                    func_name,
                    depth + 1,
                    options,
                    preserved_temps,
                );
                cleanup_stmt_list_with_options_and_preserved(
                    else_body,
                    func_name,
                    depth + 1,
                    options,
                    preserved_temps,
                );
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    cleanup_stmt_list_with_options_and_preserved(
                        &mut case.body,
                        func_name,
                        depth + 1,
                        options,
                        preserved_temps,
                    );
                }
                cleanup_stmt_list_with_options_and_preserved(
                    default,
                    func_name,
                    depth + 1,
                    options,
                    preserved_temps,
                );
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
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
        if depth == 0 && collapse_trivial_assign_returns(stmts, preserved_temps) {
            changed = true;
            last_changed_pass = Some("collapse_trivial_assign_returns");
        }
        if depth == 0 && inline_single_use_temps(stmts, preserved_temps) {
            changed = true;
            last_changed_pass = Some("inline_single_use_temps");
        }
        if depth == 0 && eliminate_dead_temp_assigns(stmts, preserved_temps) {
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
        if options.include_boundary_labels && cleanup_redundant_boundary_labels(stmts) {
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
        if iterations >= options.round_limit {
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

fn cleanup_boundary_labels_recursive(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed =
        cleanup_redundant_boundary_labels(stmts) || remove_unreferenced_leading_labels(stmts);
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                changed |= cleanup_boundary_labels_recursive(body);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= cleanup_boundary_labels_recursive(then_body);
                changed |= cleanup_boundary_labels_recursive(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= cleanup_boundary_labels_recursive(&mut case.body);
                }
                changed |= cleanup_boundary_labels_recursive(default);
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
    changed
}

fn collapse_redundant_conditional_returns_recursive(stmts: &mut Vec<HirStmt>) -> bool {
    let mut changed = collapse_redundant_conditional_returns(stmts);
    for stmt in stmts.iter_mut() {
        match stmt {
            HirStmt::Block(body)
            | HirStmt::While { body, .. }
            | HirStmt::DoWhile { body, .. }
            | HirStmt::For { body, .. } => {
                changed |= collapse_redundant_conditional_returns_recursive(body);
            }
            HirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= collapse_redundant_conditional_returns_recursive(then_body);
                changed |= collapse_redundant_conditional_returns_recursive(else_body);
            }
            HirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |= collapse_redundant_conditional_returns_recursive(&mut case.body);
                }
                changed |= collapse_redundant_conditional_returns_recursive(default);
            }
            HirStmt::Assign { .. }
            | HirStmt::VaStart { .. }
            | HirStmt::Expr(_)
            | HirStmt::Label(_)
            | HirStmt::Goto(_)
            | HirStmt::Return(_)
            | HirStmt::Break
            | HirStmt::Continue => {}
        }
    }
    changed
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
        HirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            normalize_expr(cond);
            normalize_expr(then_expr);
            normalize_expr(else_expr);
        }
        HirExpr::Var(_) | HirExpr::AddressOfGlobal(_) | HirExpr::Const(_, _) => {}
    }

    let mut current = expr.clone();
    loop {
        let next = canonicalize_integer_expr(&current)
            .or_else(|| recognize_compiler_runtime_division(&current))
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
