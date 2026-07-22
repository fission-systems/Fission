//! Canonical normalize stage functions — shared by ActionGroup passes and the pipeline driver.

use super::super::analysis::defuse::{
    apply_wide_dead_assignment_pass, constant_folding_pass, defuse_dead_assignment_pass,
    stabilize_repeated_pure_exprs,
};
use super::super::apply_rule_normalization;
use super::super::arith::{
    apply_conditional_move_pass, apply_double_precision_reconstruction_pass, apply_float_sign_pass,
    apply_ignore_nan_pass, apply_or_compare_pass, apply_subfloat_flow_pass,
    apply_three_way_compare_pass,
};
use super::super::cleanup::{
    apply_byte_sum_index_trunc, apply_deindirect_pass, apply_expand_load_pass,
    apply_subvar_trim_pass, apply_switch_norm_pass, canonicalize_minmax_conditional_returns,
    cast_elision_pass, elide_unused_popcount_assigns, eliminate_dead_local_clobber_assigns,
    eliminate_dead_temp_assigns, eliminate_redundant_var_assigns,
    hoist_param_alias_copies_before_first_use, inline_loop_condition_trailing_temps,
    normalize_dowhile_decrement_condition, prune_unused_dead_local_bindings,
    prune_unused_temp_bindings, rescue_undeclared_bindings,
    simplify_empty_and_constant_ifs_recursive, single_pred_label_inline,
};
use super::super::global_opt::{
    apply_bit_consume_dead_code_pass, apply_dead_store_elimination, apply_gvn_join_hoist_pass,
    apply_licm_pass, apply_nz_mask_simplification_pass,
    apply_post_assign_value_representative_pass, apply_redundant_load_elimination,
};
use super::super::idioms::{
    apply_branch_prefix_hoist_pass, apply_split_flow_pass, apply_subflow_pruning,
    remove_callee_save_prologue_epilogue, remove_dead_callee_saved_param_loads,
    remove_entry_stack_scaffold_stores,
};
use super::super::memory::{
    apply_aggregate_alias_access_rewrite_pass, apply_aggregate_fields_pass, apply_memory_heritage,
    apply_memory_slot_surfacing, apply_memory_slot_surfacing_cheap, apply_ptr_arith_recovery_pass,
    apply_split_datatype_pass, apply_union_resolve_pass, apply_zero_index_deref_pass,
};
use super::super::recovery::{
    apply_break_continue_pass, apply_dead_flag_cleanup_pass, apply_flag_recovery_pass,
    apply_for_loop_folding, apply_iv_recovery_pass, apply_variable_merge_pass,
    copy_propagation_pass, join_coalescing_pass,
};
use super::super::subvar_flow::apply_subvar_flow_pass;
use super::super::types::{apply_interproc_callsite_arity_pass, apply_variadic_stack_region_pass};
use super::run::{
    apply_type_signature_fixed_point, body_contains_popcount_call, body_has_loopish_shapes,
    cleanup_func_stmt_list, contains_call_stmts, hir_shape, is_large_hir_function,
    jump_resolver_admission, memory_fact_prefilter_allows_full, run_cleanup_block,
    run_cleanup_family_passes, run_pass_logged, sccp_admission_summary,
};
use fission_midend_core::action_pipeline::PassBudget;
use fission_midend_core::ir::DirFunction;
use fission_midend_core::vsa::apply_jump_resolver_pass;
use fission_midend_core::wave_stats;
use std::time::Instant;
use tracing::debug_span;

/// Head of `proto_recovery`: the `run_cleanup_family_passes` mini-orchestrator
/// (dynamic per-stage telemetry names, nested budget checks, body-shape
/// gating). Kept as one `stage_pass` step rather than forced through
/// `fn_pass`/`GatedFollowupPass` -- those primitives don't carry `diag`/`perf`
/// through to a callee, and this function's *own* internal `run_pass_logged`
/// calls need them. The remaining proto_recovery chains (flag_recovery
/// onward) are declarative `ActionGroup` passes registered after this one in
/// `groups.rs`, so decomposing this head further is a separate, focused slice
/// rather than blocking the rest of the stage.
pub fn run_stage_proto_recovery_head(func: &mut DirFunction, _diag: bool, perf: bool) {
    let _hir_normalize = debug_span!("hir_normalize", fn_name = %func.name).entered();
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
}

/// Budget-gated (`CleanupPass`, matching the original `run_cleanup_block`).
pub(super) fn cleanup_after_flag_recovery(func: &mut DirFunction) {
    cleanup_func_stmt_list(func);
    defuse_dead_assignment_pass(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
}

/// Admission gate for the `elide_unused_popcount` chain: skips (with
/// `wave_stats::add_cleanup_budget_skips` telemetry) when the body has no
/// `__popcount` calls at all, avoiding an expensive full-body DefUseMap
/// build + 8-round scan for the common case.
pub(super) fn body_contains_popcount_call_admitted(func: &DirFunction) -> bool {
    let admitted = body_contains_popcount_call(&func.body);
    if !admitted {
        wave_stats::add_cleanup_budget_skips(1);
    }
    admitted
}

pub(super) fn prune_after_elide_popcount(func: &mut DirFunction) -> bool {
    let before = hir_shape(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
    hir_shape(func) != before
}

/// Budget-gated (`CleanupPass`, matching the original `run_cleanup_block`).
pub(super) fn cleanup_after_entry_stack_scaffold(func: &mut DirFunction) {
    cleanup_func_stmt_list(func);
    defuse_dead_assignment_pass(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
}

/// Budget-gated (`CleanupPass`, matching the original `run_cleanup_block`).
pub(super) fn cleanup_after_callee_save_prologue_epilogue(func: &mut DirFunction) {
    cleanup_func_stmt_list(func);
    defuse_dead_assignment_pass(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
}

pub(super) fn constant_folding_body_pass(func: &mut DirFunction) -> bool {
    constant_folding_pass(&mut func.body)
}

/// Budget-gated (`CleanupPass`, matching the original `run_cleanup_block`).
pub(super) fn cleanup_after_constant_folding_init(func: &mut DirFunction) {
    cleanup_func_stmt_list(func);
    eliminate_dead_local_clobber_assigns(func);
    prune_unused_temp_bindings(func);
}

/// Runs the `constant_ptr_recovery` → `cleanup_constant_ptr` chain that used
/// to open the `deadcode_dynamic` stage (now fully migrated to declarative
/// `ActionGroup` passes registered directly in `groups.rs`, see
/// `GatedFollowupPass` + `CleanupPass`); kept here only as the shared
/// cleanup block body so both the pass registration and any direct callers
/// see one definition.
pub(super) fn cleanup_after_constant_ptr_recovery(func: &mut DirFunction) {
    cleanup_func_stmt_list(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
}

/// Cleanup block for the `conditional_const` chain, same migration pattern
/// as [`cleanup_after_constant_ptr_recovery`].
pub(super) fn cleanup_after_conditional_const(func: &mut DirFunction) {
    cleanup_func_stmt_list(func);
    constant_folding_pass(&mut func.body);
    eliminate_dead_local_clobber_assigns(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
}

/// Cleanup block for the `entry_param_promotion` chain, same migration
/// pattern as [`cleanup_after_constant_ptr_recovery`].
pub(super) fn cleanup_after_entry_param_promotion(func: &mut DirFunction) {
    cleanup_func_stmt_list(func);
    defuse_dead_assignment_pass(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
}

pub(super) fn sccp_is_admitted(func: &DirFunction) -> bool {
    let admission = sccp_admission_summary(&func.body);
    if !admission.eligible {
        wave_stats::add_sccp_skipped_by_admission(1);
    }
    admission.eligible
}

/// Runs after `sccp` reports Changed. Previously a bare, unlogged,
/// *unconditional* `cleanup_func_stmt_list(func)` call (no early-cleanup
/// budget gate) — registered via `fn_pass`, not `cleanup_pass`, so this
/// migration does not newly admission-gate a call that always ran before.
/// The one new telemetry entry this slice adds (`cleanup_sccp`) replaces a
/// call that had no metric name at all; NIR/HIR output is unaffected.
pub(super) fn cleanup_after_sccp(func: &mut DirFunction) -> bool {
    let before = hir_shape(func);
    cleanup_func_stmt_list(func);
    hir_shape(func) != before
}

/// Cleanup block for the nested `constant_folding_after_sccp` chain.
pub(super) fn cleanup_elim_8(func: &mut DirFunction) {
    cleanup_func_stmt_list(func);
    eliminate_dead_local_clobber_assigns(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
}

/// Unconditional SCCP-chain tail: `wide_dead_assignment` plus the two bare
/// prune calls that followed it, bundled under `wide_dead_assignment`'s
/// existing telemetry name (its own Changed result is what the original
/// `run_pass_logged` reported; the prune calls were never logged).
pub(super) fn wide_dead_assignment_and_prune(func: &mut DirFunction) -> bool {
    let changed = apply_wide_dead_assignment_pass(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
    changed
}

/// Tail of the `cse` chain: `defuse_dead_assignment_after_cse` (already
/// individually logged in the original) plus the two bare, unlogged prune
/// calls that followed it, bundled under the same telemetry name.
pub(super) fn defuse_after_cse_and_prune(func: &mut DirFunction) -> bool {
    let changed = defuse_dead_assignment_pass(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
    changed
}

/// Cleanup for the `branch_prefix_hoist` chain: bare, unlogged
/// `cleanup_func_stmt_list` that used to open the `if` body.
pub(super) fn cleanup_after_branch_prefix_hoist(func: &mut DirFunction) -> bool {
    let before = hir_shape(func);
    cleanup_func_stmt_list(func);
    hir_shape(func) != before
}

/// Tail of the `branch_prefix_hoist` chain: `defuse_dead_assignment_after_branch_hoist`
/// (already individually logged) plus the two bare prune calls that followed it.
pub(super) fn defuse_after_branch_hoist_and_prune(func: &mut DirFunction) -> bool {
    let changed = defuse_dead_assignment_pass(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
    changed
}

/// Cleanup for the `gvn_join_hoist` chain: bare, unlogged
/// `cleanup_func_stmt_list` that used to open the `if` body.
pub(super) fn cleanup_after_gvn_join_hoist(func: &mut DirFunction) -> bool {
    let before = hir_shape(func);
    cleanup_func_stmt_list(func);
    hir_shape(func) != before
}

/// Tail of the `gvn_join_hoist` chain: `defuse_dead_assignment_after_gvn`
/// (already individually logged) plus the two bare prune calls that followed it.
pub(super) fn defuse_after_gvn_and_prune(func: &mut DirFunction) -> bool {
    let changed = defuse_dead_assignment_pass(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
    changed
}


pub fn run_stage_type_early(func: &mut DirFunction, diag: bool, perf: bool) {
    apply_type_signature_fixed_point(func, diag, perf);
}

pub(super) fn cleanup_after_subvar_flow(func: &mut DirFunction) {
    cleanup_func_stmt_list(func);
    defuse_dead_assignment_pass(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
}

pub(super) fn cleanup_after_split_flow(func: &mut DirFunction) {
    cleanup_func_stmt_list(func);
    defuse_dead_assignment_pass(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
}

/// `cast_elision` chain: kept as one `stage_pass` step (like
/// `run_stage_proto_recovery_head`) because it conditionally calls
/// `apply_type_signature_fixed_point(func, diag, perf)`, which needs
/// diag/perf forwarded to its own internal `run_pass_logged` calls --
/// `fn_pass` doesn't carry those through.
pub fn run_stage_cast_elision(func: &mut DirFunction, diag: bool, perf: bool) {
    if run_pass_logged(func, "cast_elision", perf, cast_elision_pass) {
        cleanup_func_stmt_list(func);
        if !contains_call_stmts(&func.body) {
            apply_type_signature_fixed_point(func, diag, perf);
            run_pass_logged(
                func,
                "cast_elision_after_type_refine",
                perf,
                cast_elision_pass,
            );
        }
        // A light cleanup pass to simplify any newly-exposed dead code.
        run_pass_logged(
            func,
            "defuse_dead_assignment_after_cast_elision",
            perf,
            apply_wide_dead_assignment_pass,
        );
    }
}

pub(super) fn cleanup_after_subvar_trim(func: &mut DirFunction) -> bool {
    let before = hir_shape(func);
    cleanup_func_stmt_list(func);
    hir_shape(func) != before
}

pub(super) fn cleanup_after_entry_stack_scaffold_late(func: &mut DirFunction) {
    cleanup_func_stmt_list(func);
    defuse_dead_assignment_pass(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
}

pub fn run_stage_heritage_value_recovery(func: &mut DirFunction, diag: bool, perf: bool) {
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
    let heritage_changed = run_pass_logged(func, "memory_heritage", perf, apply_memory_heritage);
    if heritage_changed {
        run_cleanup_family_passes(
            func,
            "heritage_cleanup",
            perf,
            PassBudget {
                stmt_limit: 600,
                block_limit: 120,
                round_limit: 12,
            },
        );
    }
}

pub fn run_stage_memory_recovery(func: &mut DirFunction, diag: bool, perf: bool) {
    let has_loopish_control = body_has_loopish_shapes(&func.body);
    let memory_fact_prefilter = memory_fact_prefilter_allows_full(func) && !has_loopish_control;
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
    if run_pass_logged(
        func,
        "double_precision_reconstruction",
        perf,
        apply_double_precision_reconstruction_pass,
    ) {
        run_cleanup_block(func, "cleanup_double_precision", perf, |f| {
            cleanup_func_stmt_list(f);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
        });
    }
    if run_pass_logged(
        func,
        "three_way_compare",
        perf,
        apply_three_way_compare_pass,
    ) {
        run_cleanup_block(func, "cleanup_three_way_compare", perf, |f| {
            cleanup_func_stmt_list(f);
            constant_folding_pass(&mut f.body);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
        });
    }
    if run_pass_logged(func, "or_compare", perf, apply_or_compare_pass) {
        run_cleanup_block(func, "cleanup_or_compare", perf, |f| {
            cleanup_func_stmt_list(f);
            constant_folding_pass(&mut f.body);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
        });
    }
    if run_pass_logged(func, "float_sign", perf, apply_float_sign_pass) {
        run_cleanup_block(func, "cleanup_float_sign", perf, |f| {
            cleanup_func_stmt_list(f);
            constant_folding_pass(&mut f.body);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
        });
    }
    if run_pass_logged(func, "ignore_nan", perf, apply_ignore_nan_pass) {
        run_cleanup_block(func, "cleanup_ignore_nan", perf, |f| {
            cleanup_func_stmt_list(f);
            constant_folding_pass(&mut f.body);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
        });
    }
    if run_pass_logged(func, "subfloat_flow", perf, apply_subfloat_flow_pass) {
        run_cleanup_block(func, "cleanup_subfloat_flow", perf, |f| {
            cleanup_func_stmt_list(f);
            constant_folding_pass(&mut f.body);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
        });
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
        if run_pass_logged(
            func,
            "aggregate_alias_access_rewrite",
            perf,
            apply_aggregate_alias_access_rewrite_pass,
        ) {
            run_pass_logged(
                func,
                "defuse_dead_assignment_after_aggregate_alias_access",
                perf,
                defuse_dead_assignment_pass,
            );
            prune_unused_temp_bindings(func);
        }
        if run_pass_logged(func, "aggregate_fields", perf, apply_aggregate_fields_pass) {
            let _ = run_pass_logged(func, "union_resolve", perf, apply_union_resolve_pass);
            run_pass_logged(
                func,
                "ptr_arith_recovery_after_aggregate_fields",
                perf,
                apply_ptr_arith_recovery_pass,
            );
            if run_pass_logged(func, "split_datatype", perf, apply_split_datatype_pass) {
                run_cleanup_family_passes(
                    func,
                    "split_datatype_cleanup",
                    perf,
                    PassBudget {
                        stmt_limit: 600,
                        block_limit: 120,
                        round_limit: 12,
                    },
                );
            }
        }
    } else {
        wave_stats::add_memory_fact_prefilter_skip(1);
        wave_stats::add_aggregate_fields_skipped_by_admission(1);
    }
    run_pass_logged(
        func,
        "zero_index_deref_after_aggregate_fields",
        perf,
        apply_zero_index_deref_pass,
    );
}

pub fn run_stage_merge(func: &mut DirFunction, diag: bool, perf: bool) {
    for round in 0..4 {
        let (before_stmts, before_locals) = if perf { hir_shape(func) } else { (0, 0) };
        let round_start = if perf { Some(Instant::now()) } else { None };

        // Pass 1: Run type propagation/inference passes to convergence
        apply_type_signature_fixed_point(func, diag, perf);

        // Pass 2: Run variable merge pass
        let merge_changed =
            run_pass_logged(func, "variable_merge", perf, apply_variable_merge_pass);

        if diag {
            eprintln!(
                "[DIAG] merge-type-loop: {} round={} merge_changed={}",
                func.name,
                round + 1,
                merge_changed,
            );
        }

        if let Some(start) = round_start {
            let (after_stmts, after_locals) = hir_shape(func);
            eprintln!(
                "[PERF] merge-type-loop-round: fn={} round={} changed={} elapsed_ms={:.3} stmts={}=>{} locals={}=>{}",
                func.name,
                round + 1,
                merge_changed,
                start.elapsed().as_secs_f64() * 1000.0,
                before_stmts,
                after_stmts,
                before_locals,
                after_locals,
            );
        }

        if !merge_changed {
            break;
        }
    }
}

/// Tail of the `single_pred_label_inline` chain: bare, unlogged
/// `cleanup_func_stmt_list` + `apply_for_loop_folding` + the two bare prune
/// calls that followed it in the original (never individually gated or
/// logged, so this stays an unwrapped `fn_pass`, not a budget-gated
/// `cleanup_pass`).
pub(super) fn cleanup_after_single_pred_label_inline(func: &mut DirFunction) -> bool {
    let before = hir_shape(func);
    cleanup_func_stmt_list(func);
    apply_for_loop_folding(&mut func.body);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
    hir_shape(func) != before
}

/// `run_cleanup_block`-wrapped tail of the `loop_condition_trailing_temp_inline`
/// chain (budget-gated, matching the original).
pub(super) fn cleanup_after_loop_condition_temps(func: &mut DirFunction) {
    cleanup_func_stmt_list(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
}

/// `run_cleanup_block`-wrapped tail of the `iv_recovery` chain.
pub(super) fn cleanup_prune_9(func: &mut DirFunction) {
    cleanup_func_stmt_list(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
}

/// `run_cleanup_block`-wrapped tail of the `break_continue_recovery` chain.
pub(super) fn cleanup_prune_10(func: &mut DirFunction) {
    cleanup_func_stmt_list(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
}

/// `run_cleanup_block`-wrapped head of the `licm` chain tail (just the
/// statement-list cleanup; the rest of the original tail is
/// `defuse_after_licm_and_prune` below).
pub(super) fn cleanup_standalone_15(func: &mut DirFunction) {
    cleanup_func_stmt_list(func);
}

/// Tail of the `licm` chain: `defuse_dead_assignment_after_licm` (already
/// individually logged in the original) plus the two bare, unlogged prune
/// calls that followed it, bundled under the same telemetry name.
pub(super) fn defuse_after_licm_and_prune(func: &mut DirFunction) -> bool {
    let changed = defuse_dead_assignment_pass(func);
    prune_unused_temp_bindings(func);
    prune_unused_dead_local_bindings(func);
    changed
}

pub fn run_stage_cleanup(func: &mut DirFunction, diag: bool, perf: bool) {
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
    let rule_changed = run_pass_logged(func, "rule_normalization", perf, apply_rule_normalization);
    if rule_changed {
        run_cleanup_block(func, "cleanup_rule_normalization", perf, |f| {
            cleanup_func_stmt_list(f);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
        });
    }
    let stabilized_materializations = stabilize_repeated_pure_exprs(func);
    if stabilized_materializations > 0 {
        wave_stats::add_materialization_stabilized(stabilized_materializations);
        run_pass_logged(func, "proof_fidelity_materialization", perf, |_f| true);
    } else {
        wave_stats::add_pass_rerun_skipped_by_preservation(1);
    }
    run_pass_logged(func, "cleanup_empty_ifs_final", perf, |f| {
        simplify_empty_and_constant_ifs_recursive(&mut f.body)
    });
    run_pass_logged(func, "canonicalize_minmax_final", perf, |f| {
        canonicalize_minmax_conditional_returns(&mut f.body)
    });
    run_pass_logged(func, "prune_unused_bindings_final", perf, |f| {
        let before = hir_shape(f);
        prune_unused_temp_bindings(f);
        remove_dead_callee_saved_param_loads(f);
        prune_unused_dead_local_bindings(f);
        before != hir_shape(f)
    });
    // Subflow / bitmask pruning: optimize redundant bit-widths and bitmasks (subflow.cc).
    run_pass_logged(func, "subflow_pruning_final", perf, apply_subflow_pruning);
    // Recover `movzx` low-byte after byte+byte sum identity copies (RC4 keystream index).
    // Runs after subflow so we can re-introduce a necessary `& 0xff` that earlier waves dropped.
    run_pass_logged(
        func,
        "byte_sum_index_trunc",
        perf,
        apply_byte_sum_index_trunc,
    );
    apply_type_signature_fixed_point(func, diag, perf);
    // Residual CF/OF/SF/ZF/PF stores after late materialize/arith waves — drop if unused.
    run_pass_logged(
        func,
        "dead_flag_cleanup_final",
        perf,
        apply_dead_flag_cleanup_pass,
    );
    run_pass_logged(func, "hoist_param_alias_copies", perf, |f| {
        hoist_param_alias_copies_before_first_use(&mut f.body)
    });
    run_pass_logged(
        func,
        "rescue_undeclared_bindings",
        perf,
        rescue_undeclared_bindings,
    );
    // Final sweep: pure `x = x` may be reintroduced by late cast/materialize
    // rewrites after the main cleanup fixed-point loop.
    run_pass_logged(func, "eliminate_redundant_var_assigns_final", perf, |f| {
        eliminate_redundant_var_assigns(&mut f.body)
    });
    // CDQ/IDIV wide-piece remainder: `t = (hi<<32)|lo; x = t % d` → signed `lo % d`.
    run_pass_logged(func, "collapse_cdq_signed_mod", perf, |f| {
        crate::arith::collapse_cdq_signed_mod_in_stmts(&mut f.body)
    });
    // Drop temps left only by the pre-collapse wide dividend assign.
    run_pass_logged(func, "eliminate_dead_temp_after_cdq", perf, |f| {
        eliminate_dead_temp_assigns(&mut f.body, &crate::HashSet::default())
    });
    // Collapse may leave pure temps with zero uses; final prune already ran
    // before CDQ, so re-prune trivial unused bindings (e.g. residual xVar).
    run_pass_logged(func, "prune_unused_temp_after_cdq", perf, |f| {
        prune_unused_temp_bindings(f) | prune_unused_dead_local_bindings(f)
    });
}
