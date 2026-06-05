//! Canonical normalize stage functions — shared by ActionGroup passes and the pipeline driver.

use super::run::{
    apply_type_signature_fixed_point, body_contains_popcount_call, body_has_loopish_shapes,
    cleanup_func_stmt_list, contains_call_stmts, hir_shape, is_large_hir_function,
    jump_resolver_admission, memory_fact_prefilter_allows_full, run_cleanup_block,
    run_cleanup_family_passes, run_pass_logged, sccp_admission_summary,
};
use super::super::analysis::defuse::{
    apply_wide_dead_assignment_pass, constant_folding_pass, defuse_dead_assignment_pass,
    stabilize_repeated_pure_exprs,
};
use super::super::arith::{
    apply_conditional_move_pass, apply_double_precision_reconstruction_pass,
    apply_float_sign_pass, apply_ignore_nan_pass, apply_or_compare_pass,
    apply_subfloat_flow_pass, apply_three_way_compare_pass,
};
use super::super::cleanup::{
    apply_deindirect_pass, apply_expand_load_pass, apply_subvar_trim_pass, apply_switch_norm_pass,
    canonicalize_minmax_conditional_returns, cast_elision_pass, elide_unused_popcount_assigns,
    eliminate_dead_local_clobber_assigns, inline_loop_condition_trailing_temps,
    normalize_dowhile_decrement_condition, prune_unused_dead_local_bindings,
    prune_unused_temp_bindings, rescue_undeclared_bindings, single_pred_label_inline,
    simplify_empty_and_constant_ifs_recursive,
};
use super::super::global_opt::{
    apply_bit_consume_dead_code_pass, apply_conditional_const_pass, apply_cse_pass,
    apply_dead_store_elimination, apply_gvn_join_hoist_pass, apply_licm_pass,
    apply_nz_mask_simplification_pass, apply_post_assign_value_representative_pass,
    apply_redundant_load_elimination, apply_sccp_pass,
};
use super::super::idioms::{
    apply_branch_prefix_hoist_pass, apply_split_flow_pass, apply_subflow_pruning,
    remove_callee_save_prologue_epilogue, remove_dead_callee_saved_param_loads,
    remove_entry_stack_scaffold_stores,
};
use super::super::memory::{
    apply_aggregate_alias_access_rewrite_pass, apply_aggregate_fields_pass,
    apply_constant_ptr_recovery_pass, apply_memory_heritage, apply_memory_slot_surfacing,
    apply_memory_slot_surfacing_cheap, apply_ptr_arith_recovery_pass, apply_split_datatype_pass,
    apply_union_resolve_pass, apply_zero_index_deref_pass,
};
use super::super::recovery::{
    apply_break_continue_pass, apply_flag_recovery_pass, apply_for_loop_folding,
    apply_iv_recovery_pass, copy_propagation_pass, join_coalescing_pass, apply_variable_merge_pass,
};
use super::super::subvar_flow::apply_subvar_flow_pass;
use super::super::types::{
    apply_entry_param_promotion_pass, apply_interproc_callsite_arity_pass,
    apply_type_inference_pass, apply_variadic_stack_region_pass,
};
use super::super::apply_rule_normalization;
use super::super::wave_stats;
use crate::nir::action_pipeline::PassBudget;
use crate::nir::types::HirFunction;
use crate::nir::vsa::apply_jump_resolver_pass;
use std::time::Instant;
use tracing::debug_span;


pub(crate) fn run_stage_proto_recovery(func: &mut HirFunction, diag: bool, perf: bool) {
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
    // Fast prefilter: skip entirely when the body has no __popcount calls,
    // avoiding an expensive full-body DefUseMap build + 8-round scan.
    if body_contains_popcount_call(&func.body) {
        if run_pass_logged(
            func,
            "elide_unused_popcount",
            perf,
            elide_unused_popcount_assigns,
        ) {
            prune_unused_temp_bindings(func);
            prune_unused_dead_local_bindings(func);
        }
    } else {
        wave_stats::add_cleanup_budget_skips(1);
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
    // Run constant folding after the initial cleanup so that folded constants
    // unlock further simplifications in subsequent passes.
    if run_pass_logged(func, "constant_folding", perf, |f| {
        constant_folding_pass(&mut f.body)
    }) {
        run_cleanup_block(func, "cleanup_elim_7", perf, |f| {
            cleanup_func_stmt_list(f);

            eliminate_dead_local_clobber_assigns(f);

            prune_unused_temp_bindings(f);

        });
    }
}

pub(crate) fn run_stage_deadcode_dynamic(func: &mut HirFunction, diag: bool, perf: bool) {
    if run_pass_logged(
        func,
        "constant_ptr_recovery",
        perf,
        apply_constant_ptr_recovery_pass,
    ) {
        run_cleanup_block(func, "cleanup_constant_ptr", perf, |f| {
            cleanup_func_stmt_list(f);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
        });
    }
    if run_pass_logged(func, "conditional_const", perf, apply_conditional_const_pass) {
        run_cleanup_block(func, "cleanup_conditional_const", perf, |f| {
            cleanup_func_stmt_list(f);
            constant_folding_pass(&mut f.body);
            eliminate_dead_local_clobber_assigns(f);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
        });
    }
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
    // Remove dead callee-saved register assignments whose uses were all
    // copy-propagated away (e.g. `rbx = param_3` after every `rbx` use
    // was substituted with `param_3`). Runs unconditionally so it catches
    // cases where copy propagation fired in an earlier pipeline wave.
    run_pass_logged(
        func,
        "remove_dead_callee_param_loads",
        perf,
        remove_dead_callee_saved_param_loads,
    );
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
        run_pass_logged(
            func,
            "remove_dead_callee_param_loads_after_branch_hoist",
            perf,
            remove_dead_callee_saved_param_loads,
        );
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
        run_pass_logged(
            func,
            "remove_dead_callee_param_loads_after_gvn",
            perf,
            remove_dead_callee_saved_param_loads,
        );
    }
}

pub(crate) fn run_stage_type_early(func: &mut HirFunction, diag: bool, perf: bool) {
    apply_type_signature_fixed_point(func, diag, perf);
}

pub(crate) fn run_stage_stackstall(func: &mut HirFunction, diag: bool, perf: bool) {
    run_pass_logged(func, "nz_mask_simplification", perf, apply_nz_mask_simplification_pass);
    // Subflow / bitmask pruning: optimize redundant bit-widths and bitmasks (subflow.cc).
    run_pass_logged(func, "subflow_pruning_early", perf, apply_subflow_pruning);
    // Global subvariable flow analyzer: propagate active bitmasks globally to declare narrow subvariables.
    if run_pass_logged(func, "subvar_flow_pass", perf, apply_subvar_flow_pass) {
        run_cleanup_block(func, "cleanup_subvar_flow", perf, |f| {
            cleanup_func_stmt_list(f);
            defuse_dead_assignment_pass(f);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
        });
    }
    // SplitFlow: identify and split artificially joined local variables.
    if run_pass_logged(func, "split_flow_pass", perf, apply_split_flow_pass) {
        run_cleanup_block(func, "cleanup_split_flow", perf, |f| {
            cleanup_func_stmt_list(f);
            defuse_dead_assignment_pass(f);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
        });
    }
    // Cast elision: remove outer casts that are redundant given the binding's
    // declared type (assignment-context cast: `x = (T)y` where x.ty == T).
    // Runs after type inference so that NirBinding.ty is maximally populated.
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
    // Sub-word data flow cast trimming: eliminate redundant casts of sub-word data flow variables.
    if run_pass_logged(func, "subvar_trim", perf, apply_subvar_trim_pass) {
        cleanup_func_stmt_list(func);
        run_pass_logged(
            func,
            "defuse_dead_assignment_after_subvar_trim",
            perf,
            apply_wide_dead_assignment_pass,
        );
    }
    // ExpandLoad: collapse Cast<narrow>(Load<wide>(ptr)) → Load<narrow>(ptr) for natural
    // LSB truncations, and widen AND-comparison constants when the Load type is wider.
    // Mirrors Ghidra's RuleExpandLoad. Runs after cast_elision so type annotations are fresh.
    if run_pass_logged(func, "expand_load", perf, apply_expand_load_pass) {
        run_pass_logged(
            func,
            "defuse_dead_assignment_after_expand_load",
            perf,
            apply_wide_dead_assignment_pass,
        );
    }
    // Bit-level consumed-mask dead-code pass: eliminate dead OR-constant branches
    // and redundant ZEXT operations by backward-propagating consumed bit masks.
    // Mirrors Ghidra's ActionDeadCode (consumed-mask propagation) at the HIR level.
    // Runs after subvar_flow (narrow variables confirmed) and cast_elision.
    if run_pass_logged(
        func,
        "bit_consume_dead_code",
        perf,
        apply_bit_consume_dead_code_pass,
    ) {
        run_pass_logged(
            func,
            "defuse_dead_assignment_after_bit_consume",
            perf,
            apply_wide_dead_assignment_pass,
        );
        run_cleanup_block(func, "cleanup_bit_consume", perf, |f| {
            cleanup_func_stmt_list(f);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
        });
    }
    if run_pass_logged(
        func,
        "conditional_move",
        perf,
        apply_conditional_move_pass,
    ) {
        run_cleanup_block(func, "cleanup_conditional_move", perf, |f| {
            cleanup_func_stmt_list(f);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
        });
    }
    if run_pass_logged(
        func,
        "switch_norm",
        perf,
        apply_switch_norm_pass,
    ) {
        run_cleanup_block(func, "cleanup_switch_norm", perf, |f| {
            cleanup_func_stmt_list(f);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
        });
    }
    if run_pass_logged(
        func,
        "deindirect",
        perf,
        apply_deindirect_pass,
    ) {
        run_cleanup_block(func, "cleanup_deindirect", perf, |f| {
            cleanup_func_stmt_list(f);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
        });
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
}

pub(crate) fn run_stage_heritage_value_recovery(func: &mut HirFunction, diag: bool, perf: bool) {
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
    let heritage_changed = run_pass_logged(
        func,
        "memory_heritage",
        perf,
        apply_memory_heritage,
    );
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

pub(crate) fn run_stage_memory_recovery(func: &mut HirFunction, diag: bool, perf: bool) {
    let has_loopish_control = body_has_loopish_shapes(&func.body);
    let memory_fact_prefilter =
        memory_fact_prefilter_allows_full(func) && !has_loopish_control;
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
    if run_pass_logged(
        func,
        "or_compare",
        perf,
        apply_or_compare_pass,
    ) {
        run_cleanup_block(func, "cleanup_or_compare", perf, |f| {
            cleanup_func_stmt_list(f);
            constant_folding_pass(&mut f.body);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
        });
    }
    if run_pass_logged(
        func,
        "float_sign",
        perf,
        apply_float_sign_pass,
    ) {
        run_cleanup_block(func, "cleanup_float_sign", perf, |f| {
            cleanup_func_stmt_list(f);
            constant_folding_pass(&mut f.body);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
        });
    }
    if run_pass_logged(
        func,
        "ignore_nan",
        perf,
        apply_ignore_nan_pass,
    ) {
        run_cleanup_block(func, "cleanup_ignore_nan", perf, |f| {
            cleanup_func_stmt_list(f);
            constant_folding_pass(&mut f.body);
            prune_unused_temp_bindings(f);
            prune_unused_dead_local_bindings(f);
        });
    }
    if run_pass_logged(
        func,
        "subfloat_flow",
        perf,
        apply_subfloat_flow_pass,
    ) {
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

pub(crate) fn run_stage_merge(func: &mut HirFunction, diag: bool, perf: bool) {
    for round in 0..4 {
        let (before_stmts, before_locals) = if perf { hir_shape(func) } else { (0, 0) };
        let round_start = if perf { Some(Instant::now()) } else { None };

        // Pass 1: Run type propagation/inference passes to convergence
        apply_type_signature_fixed_point(func, diag, perf);

        // Pass 2: Run variable merge pass
        let merge_changed = run_pass_logged(func, "variable_merge", perf, apply_variable_merge_pass);

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

pub(crate) fn run_stage_block_structure_1(func: &mut HirFunction, diag: bool, perf: bool) {
    if run_pass_logged(func, "single_pred_label_inline", perf, |f| {
        single_pred_label_inline(&mut f.body)
    }) {
        cleanup_func_stmt_list(func);
        apply_for_loop_folding(&mut func.body);
        prune_unused_temp_bindings(func);
        prune_unused_dead_local_bindings(func);
    }
    run_pass_logged(func, "dowhile_decrement_condition_norm", perf, |f| {
        normalize_dowhile_decrement_condition(&mut f.body)
    });
    if run_pass_logged(func, "loop_condition_trailing_temp_inline", perf, |f| {
        inline_loop_condition_trailing_temps(f)
    }) {
        run_pass_logged(
            func,
            "ptr_arith_recovery_after_loop_condition_temps",
            perf,
            apply_ptr_arith_recovery_pass,
        );
        run_cleanup_block(func, "cleanup_loop_condition_temps", perf, |f| {
            cleanup_func_stmt_list(f);

            prune_unused_temp_bindings(f);

            prune_unused_dead_local_bindings(f);
        });
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
}

pub(crate) fn run_stage_cleanup(func: &mut HirFunction, diag: bool, perf: bool) {
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
    run_pass_logged(func, "type_inference_final", perf, apply_type_inference_pass);
    run_pass_logged(func, "rescue_undeclared_bindings", perf, rescue_undeclared_bindings);
}
