//! Ghidra-ordered normalize action groups and pipeline driver.

use fission_midend_core::wave_stats;
use super::stages::{
    body_contains_popcount_call_admitted, cleanup_after_branch_prefix_hoist,
    cleanup_after_callee_save_prologue_epilogue, cleanup_after_conditional_const,
    cleanup_after_constant_folding_init, cleanup_after_constant_ptr_recovery,
    cleanup_after_entry_param_promotion, cleanup_after_entry_stack_scaffold,
    cleanup_after_entry_stack_scaffold_late, cleanup_after_flag_recovery,
    cleanup_after_gvn_join_hoist, cleanup_after_loop_condition_temps,
    cleanup_after_sccp, cleanup_after_single_pred_label_inline, cleanup_after_split_flow,
    cleanup_after_subvar_flow, cleanup_after_subvar_trim, cleanup_elim_8, cleanup_prune_9,
    cleanup_prune_10, cleanup_standalone_15, constant_folding_body_pass,
    defuse_after_branch_hoist_and_prune, defuse_after_cse_and_prune, defuse_after_gvn_and_prune,
    defuse_after_licm_and_prune, prune_after_elide_popcount, run_stage_cast_elision,
    run_stage_cleanup, run_stage_heritage_value_recovery, run_stage_memory_recovery,
    run_stage_merge, run_stage_proto_recovery_head, run_stage_type_early, sccp_is_admitted,
    wide_dead_assignment_and_prune,
};
use super::super::analysis::defuse::{
    apply_wide_dead_assignment_pass, constant_folding_pass, defuse_dead_assignment_pass,
};
use super::super::cleanup::{
    apply_deindirect_pass, apply_expand_load_pass, apply_subvar_trim_pass,
    apply_switch_norm_pass, cast_elision_pass, elide_unused_popcount_assigns,
    inline_loop_condition_trailing_temps, normalize_dowhile_decrement_condition,
    single_pred_label_inline,
};
use super::super::arith::apply_conditional_move_pass;
use super::super::global_opt::{
    apply_bit_consume_dead_code_pass, apply_conditional_const_pass, apply_cse_pass,
    apply_gvn_join_hoist_pass, apply_licm_pass, apply_nz_mask_simplification_pass,
    apply_sccp_pass,
};
use super::super::idioms::{
    apply_branch_prefix_hoist_pass, apply_split_flow_pass, apply_subflow_pruning,
    remove_callee_save_prologue_epilogue, remove_dead_callee_saved_param_loads,
    remove_entry_stack_scaffold_stores,
};
use super::super::subvar_flow::apply_subvar_flow_pass;
use super::super::memory::{apply_constant_ptr_recovery_pass, apply_ptr_arith_recovery_pass};
use super::super::recovery::{
    apply_break_continue_pass, apply_flag_recovery_pass, apply_iv_recovery_pass,
    copy_propagation_pass, join_coalescing_pass,
};
use super::super::types::apply_entry_param_promotion_pass;
use fission_midend_core::action_pipeline::{
    GhidraActionConcept, Pass, PassCtx, PassOutcome, Pipeline, admission_gated, cleanup_pass,
    fn_pass, gated_followup, group,
};
use fission_midend_core::ir::HirFunction;
use std::time::Instant;

struct CanonicalStagePass {
    stage: &'static str,
    concept: GhidraActionConcept,
    run: fn(&mut HirFunction, bool, bool),
}

impl Pass for CanonicalStagePass {
    fn name(&self) -> &'static str {
        self.stage
    }

    fn concept(&self) -> GhidraActionConcept {
        self.concept
    }

    fn run(&self, ctx: &mut PassCtx<'_>) -> PassOutcome {
        (self.run)(ctx.func, ctx.diag, ctx.perf);
        PassOutcome::Unchanged
    }
}

fn stage_pass(
    stage: &'static str,
    concept: GhidraActionConcept,
    run: fn(&mut HirFunction, bool, bool),
) -> Box<dyn Pass> {
    Box::new(CanonicalStagePass {
        stage,
        concept,
        run,
    })
}

pub fn build_normalize_pipeline() -> Pipeline {
    let concept = GhidraActionConcept::Normalize;
    let proto = GhidraActionConcept::PrototypeTypes;
    let heritage = GhidraActionConcept::HeritageValueRecovery;
    let block = GhidraActionConcept::BlockGraphStructuring;

    Pipeline::new("normalize")
        .group(
            group("proto_recovery", concept)
                // `run_cleanup_family_passes` mini-orchestrator: dynamic
                // per-stage telemetry names + nested budget checks, kept as
                // one stage_pass step (needs diag/perf forwarding that
                // fn_pass doesn't carry). See stages.rs doc comment.
                .pass(stage_pass(
                    "proto_recovery",
                    concept,
                    run_stage_proto_recovery_head,
                ))
                // Flag recovery: substitute raw x86 EFLAGS variable
                // references in branch conditions with high-level
                // comparison expressions. Runs early so subsequent
                // dead-assignment passes can eliminate now-dead flag
                // variable assignments.
                .pass(gated_followup(
                    fn_pass("flag_recovery", concept, apply_flag_recovery_pass),
                    vec![cleanup_pass(
                        "cleanup_defuse_4",
                        concept,
                        cleanup_after_flag_recovery,
                    )],
                ))
                // Parity/popcount dead elimination: remove __popcount-based
                // assignments whose result is unconsumed. Admission-gated on
                // body_contains_popcount_call to skip the expensive
                // full-body DefUseMap scan for the common no-popcount case.
                .pass(admission_gated(
                    body_contains_popcount_call_admitted,
                    None,
                    gated_followup(
                        fn_pass(
                            "elide_unused_popcount",
                            concept,
                            elide_unused_popcount_assigns,
                        ),
                        vec![fn_pass(
                            "prune_after_elide_popcount",
                            concept,
                            prune_after_elide_popcount,
                        )],
                    ),
                ))
                // Prologue/epilogue elimination: remove callee-saved
                // register save/restore pairs from the function body.
                .pass(gated_followup(
                    fn_pass(
                        "remove_entry_stack_scaffold_stores",
                        concept,
                        remove_entry_stack_scaffold_stores,
                    ),
                    vec![cleanup_pass(
                        "cleanup_defuse_entry_stack_scaffold",
                        concept,
                        cleanup_after_entry_stack_scaffold,
                    )],
                ))
                .pass(gated_followup(
                    fn_pass(
                        "remove_callee_save_prologue_epilogue",
                        concept,
                        remove_callee_save_prologue_epilogue,
                    ),
                    vec![cleanup_pass(
                        "cleanup_defuse_5",
                        concept,
                        cleanup_after_callee_save_prologue_epilogue,
                    )],
                ))
                // Constant folding after the initial cleanup so folded
                // constants unlock further simplification in later passes.
                .pass(gated_followup(
                    fn_pass("constant_folding", concept, constant_folding_body_pass),
                    vec![cleanup_pass(
                        "cleanup_elim_7",
                        concept,
                        cleanup_after_constant_folding_init,
                    )],
                )),
        )
        .group(
            group("deadcode_dynamic", concept)
                // Migrated off the imperative `if run_pass_logged(..) { .. }`
                // idiom onto declarative ActionGroup passes (PROJECT.md M3,
                // first slice). `apply_constant_ptr_recovery_pass` reports
                // Changed; the budget-gated cleanup block only then runs.
                .pass(gated_followup(
                    fn_pass(
                        "constant_ptr_recovery",
                        concept,
                        apply_constant_ptr_recovery_pass,
                    ),
                    vec![cleanup_pass(
                        "cleanup_constant_ptr",
                        concept,
                        cleanup_after_constant_ptr_recovery,
                    )],
                ))
                .pass(gated_followup(
                    fn_pass(
                        "conditional_const",
                        concept,
                        apply_conditional_const_pass,
                    ),
                    vec![cleanup_pass(
                        "cleanup_conditional_const",
                        concept,
                        cleanup_after_conditional_const,
                    )],
                ))
                .pass(gated_followup(
                    fn_pass(
                        "entry_param_promotion",
                        concept,
                        apply_entry_param_promotion_pass,
                    ),
                    vec![cleanup_pass(
                        "cleanup_defuse_6",
                        concept,
                        cleanup_after_entry_param_promotion,
                    )],
                ))
                // SCCP: global sparse constant propagation on structured HIR
                // (lattice merge at joins). Runs after local constant folding
                // so folded seeds propagate. Admission-gated on
                // sccp_admission_summary; the previously-unlogged
                // `cleanup_func_stmt_list` after a Changed sccp run is now
                // the one new telemetry entry this slice adds
                // (`cleanup_sccp`) — NIR/HIR output is unaffected.
                .pass(admission_gated(
                    sccp_is_admitted,
                    None,
                    gated_followup(
                        fn_pass("sccp", concept, apply_sccp_pass),
                        vec![
                            fn_pass("cleanup_sccp", concept, cleanup_after_sccp),
                            gated_followup(
                                fn_pass(
                                    "constant_folding_after_sccp",
                                    concept,
                                    |f| constant_folding_pass(&mut f.body),
                                ),
                                vec![cleanup_pass("cleanup_elim_8", concept, cleanup_elim_8)],
                            ),
                            fn_pass(
                                "wide_dead_assignment",
                                concept,
                                wide_dead_assignment_and_prune,
                            ),
                        ],
                    ),
                ))
                // Local CSE: within each linear block, replace identical
                // pure sub-expressions with the variable that first computed
                // them. Runs right after constant folding so folded
                // constants are included in the expression map.
                .pass(gated_followup(
                    fn_pass("cse", concept, apply_cse_pass),
                    vec![
                        gated_followup(
                            fn_pass(
                                "copy_propagation_after_cse",
                                concept,
                                copy_propagation_pass,
                            ),
                            vec![fn_pass(
                                "defuse_dead_assignment_after_cse_copy",
                                concept,
                                defuse_dead_assignment_pass,
                            )],
                        ),
                        fn_pass(
                            "defuse_dead_assignment_after_cse",
                            concept,
                            defuse_after_cse_and_prune,
                        ),
                    ],
                ))
                // Function-level def-use dead assignment: removes dead
                // writes to ANY variable (not just trivially-named temps)
                // across the whole body tree. Runs unconditionally.
                .pass(fn_pass(
                    "defuse_dead_assignment",
                    concept,
                    defuse_dead_assignment_pass,
                ))
                // Copy propagation: forward-substitute `x = y` (single-
                // definition copy) to eliminate unnecessary temporaries.
                .pass(gated_followup(
                    fn_pass("copy_propagation", concept, copy_propagation_pass),
                    vec![fn_pass(
                        "defuse_dead_assignment_after_copy",
                        concept,
                        defuse_dead_assignment_pass,
                    )],
                ))
                // Remove dead callee-saved register assignments whose uses
                // were all copy-propagated away. Runs unconditionally so it
                // catches cases where copy propagation fired in an earlier
                // pipeline wave.
                .pass(fn_pass(
                    "remove_dead_callee_param_loads",
                    concept,
                    remove_dead_callee_saved_param_loads,
                ))
                // Join-variable coalescing: unify parallel temporaries
                // assigned in both branches of an if-else (SSA out-of-SSA
                // for 2-way joins).
                .pass(fn_pass(
                    "join_coalescing",
                    concept,
                    join_coalescing_pass,
                ))
                // If-else common pure-prefix hoisting: move identical
                // leading assignments out of both branches (partial
                // redundancy elimination for branches).
                .pass(gated_followup(
                    fn_pass(
                        "branch_prefix_hoist",
                        concept,
                        apply_branch_prefix_hoist_pass,
                    ),
                    vec![
                        fn_pass(
                            "cleanup_branch_prefix_hoist",
                            concept,
                            cleanup_after_branch_prefix_hoist,
                        ),
                        gated_followup(
                            fn_pass(
                                "copy_propagation_after_branch_hoist",
                                concept,
                                copy_propagation_pass,
                            ),
                            vec![fn_pass(
                                "defuse_dead_assignment_after_branch_hoist_copy",
                                concept,
                                defuse_dead_assignment_pass,
                            )],
                        ),
                        fn_pass(
                            "defuse_dead_assignment_after_branch_hoist",
                            concept,
                            defuse_after_branch_hoist_and_prune,
                        ),
                        fn_pass(
                            "remove_dead_callee_param_loads_after_branch_hoist",
                            concept,
                            remove_dead_callee_saved_param_loads,
                        ),
                    ],
                ))
                // GVN-lite at 2-way joins: duplicate pure RHS, different LHS
                // → hoist temp.
                .pass(gated_followup(
                    fn_pass(
                        "gvn_join_hoist",
                        concept,
                        apply_gvn_join_hoist_pass,
                    ),
                    vec![
                        fn_pass(
                            "cleanup_gvn_join_hoist",
                            concept,
                            cleanup_after_gvn_join_hoist,
                        ),
                        gated_followup(
                            fn_pass(
                                "copy_propagation_after_gvn",
                                concept,
                                copy_propagation_pass,
                            ),
                            vec![fn_pass(
                                "defuse_dead_assignment_after_gvn_copy",
                                concept,
                                defuse_dead_assignment_pass,
                            )],
                        ),
                        fn_pass(
                            "defuse_dead_assignment_after_gvn",
                            concept,
                            defuse_after_gvn_and_prune,
                        ),
                        fn_pass(
                            "remove_dead_callee_param_loads_after_gvn",
                            concept,
                            remove_dead_callee_saved_param_loads,
                        ),
                    ],
                )),
        )
        .group(group("type_early", proto).pass(stage_pass(
            "type_early",
            proto,
            run_stage_type_early,
        )))
        .group(
            group("stackstall", concept)
                .pass(fn_pass(
                    "nz_mask_simplification",
                    concept,
                    apply_nz_mask_simplification_pass,
                ))
                // Subflow / bitmask pruning: optimize redundant bit-widths
                // and bitmasks (subflow.cc).
                .pass(fn_pass(
                    "subflow_pruning_early",
                    concept,
                    apply_subflow_pruning,
                ))
                // Global subvariable flow analyzer: propagate active
                // bitmasks globally to declare narrow subvariables.
                .pass(gated_followup(
                    fn_pass("subvar_flow_pass", concept, apply_subvar_flow_pass),
                    vec![cleanup_pass(
                        "cleanup_subvar_flow",
                        concept,
                        cleanup_after_subvar_flow,
                    )],
                ))
                // SplitFlow: identify and split artificially joined local
                // variables.
                .pass(gated_followup(
                    fn_pass("split_flow_pass", concept, apply_split_flow_pass),
                    vec![cleanup_pass(
                        "cleanup_split_flow",
                        concept,
                        cleanup_after_split_flow,
                    )],
                ))
                // Cast elision: kept as one stage_pass step (needs
                // diag/perf forwarded to its own internal
                // apply_type_signature_fixed_point call). See stages.rs.
                .pass(stage_pass(
                    "cast_elision",
                    concept,
                    run_stage_cast_elision,
                ))
                // Sub-word data flow cast trimming: eliminate redundant
                // casts of sub-word data flow variables.
                .pass(gated_followup(
                    fn_pass("subvar_trim", concept, apply_subvar_trim_pass),
                    vec![
                        fn_pass(
                            "cleanup_subvar_trim_stmt_list",
                            concept,
                            cleanup_after_subvar_trim,
                        ),
                        fn_pass(
                            "defuse_dead_assignment_after_subvar_trim",
                            concept,
                            apply_wide_dead_assignment_pass,
                        ),
                    ],
                ))
                // ExpandLoad: collapse Cast<narrow>(Load<wide>(ptr)) →
                // Load<narrow>(ptr) for natural LSB truncations, and widen
                // AND-comparison constants when the Load type is wider.
                .pass(gated_followup(
                    fn_pass("expand_load", concept, apply_expand_load_pass),
                    vec![fn_pass(
                        "defuse_dead_assignment_after_expand_load",
                        concept,
                        apply_wide_dead_assignment_pass,
                    )],
                ))
                // Bit-level consumed-mask dead-code pass: eliminate dead
                // OR-constant branches and redundant ZEXT operations by
                // backward-propagating consumed bit masks.
                .pass(gated_followup(
                    fn_pass(
                        "bit_consume_dead_code",
                        concept,
                        apply_bit_consume_dead_code_pass,
                    ),
                    vec![
                        fn_pass(
                            "defuse_dead_assignment_after_bit_consume",
                            concept,
                            apply_wide_dead_assignment_pass,
                        ),
                        cleanup_pass(
                            "cleanup_bit_consume",
                            concept,
                            cleanup_after_constant_ptr_recovery,
                        ),
                    ],
                ))
                .pass(gated_followup(
                    fn_pass(
                        "conditional_move",
                        concept,
                        apply_conditional_move_pass,
                    ),
                    vec![cleanup_pass(
                        "cleanup_conditional_move",
                        concept,
                        cleanup_after_constant_ptr_recovery,
                    )],
                ))
                .pass(gated_followup(
                    fn_pass("switch_norm", concept, apply_switch_norm_pass),
                    vec![cleanup_pass(
                        "cleanup_switch_norm",
                        concept,
                        cleanup_after_constant_ptr_recovery,
                    )],
                ))
                .pass(gated_followup(
                    fn_pass("deindirect", concept, apply_deindirect_pass),
                    vec![cleanup_pass(
                        "cleanup_deindirect",
                        concept,
                        cleanup_after_constant_ptr_recovery,
                    )],
                ))
                .pass(gated_followup(
                    fn_pass(
                        "remove_entry_stack_scaffold_stores_late",
                        concept,
                        remove_entry_stack_scaffold_stores,
                    ),
                    vec![cleanup_pass(
                        "cleanup_defuse_entry_stack_scaffold_late",
                        concept,
                        cleanup_after_entry_stack_scaffold_late,
                    )],
                )),
        )
        .group(group("heritage_value_recovery", heritage).pass(stage_pass(
            "heritage_value_recovery",
            heritage,
            run_stage_heritage_value_recovery,
        )))
        .group(group("memory_recovery", concept).pass(stage_pass(
            "memory_recovery",
            concept,
            run_stage_memory_recovery,
        )))
        .group(group("merge", proto).pass(stage_pass("merge", proto, run_stage_merge)))
        .group(
            group("block_structure_1", block)
                .pass(gated_followup(
                    fn_pass(
                        "single_pred_label_inline",
                        block,
                        |f| single_pred_label_inline(&mut f.body),
                    ),
                    vec![fn_pass(
                        "cleanup_single_pred_label_inline",
                        block,
                        cleanup_after_single_pred_label_inline,
                    )],
                ))
                .pass(fn_pass(
                    "dowhile_decrement_condition_norm",
                    block,
                    |f| normalize_dowhile_decrement_condition(&mut f.body),
                ))
                // Runs after label inlining so the loop body is maximally
                // simplified before trailing-temp inlining looks at loop
                // conditions.
                .pass(gated_followup(
                    fn_pass(
                        "loop_condition_trailing_temp_inline",
                        block,
                        inline_loop_condition_trailing_temps,
                    ),
                    vec![
                        fn_pass(
                            "ptr_arith_recovery_after_loop_condition_temps",
                            block,
                            apply_ptr_arith_recovery_pass,
                        ),
                        cleanup_pass(
                            "cleanup_loop_condition_temps",
                            block,
                            cleanup_after_loop_condition_temps,
                        ),
                    ],
                ))
                // Loop IV recovery (SCEV-lite): upgrade While → For for
                // linear induction variables.
                .pass(gated_followup(
                    fn_pass("iv_recovery", block, apply_iv_recovery_pass),
                    vec![cleanup_pass("cleanup_prune_9", block, cleanup_prune_9)],
                ))
                // Break/Continue recovery: replace single-predecessor
                // Goto-to-exit-label patterns inside loops with explicit
                // break/continue statements.
                .pass(gated_followup(
                    fn_pass(
                        "break_continue_recovery",
                        block,
                        apply_break_continue_pass,
                    ),
                    vec![cleanup_pass("cleanup_prune_10", block, cleanup_prune_10)],
                ))
                // Loop Invariant Code Motion: hoist pure loop-invariant
                // assignments out of loop bodies (innermost-first). Runs
                // after break/continue recovery so the loop structure is
                // finalised.
                .pass(gated_followup(
                    fn_pass("licm", block, apply_licm_pass),
                    vec![
                        cleanup_pass("cleanup_standalone_15", block, cleanup_standalone_15),
                        fn_pass(
                            "defuse_dead_assignment_after_licm",
                            block,
                            defuse_after_licm_and_prune,
                        ),
                    ],
                )),
        )
        .group(group("cleanup", concept).pass(stage_pass("cleanup", concept, run_stage_cleanup)))
}

pub fn run_normalize_pipeline(func: &mut HirFunction, diag: bool, perf: bool) {
    let total_start = Instant::now();
    wave_stats::reset_normalize_wave_stats();

    if diag {
        eprintln!(
            "[DIAG] normalize start: {} stmts={} locals={}",
            func.name,
            fission_midend_core::action_pipeline::count_hir_stmts(&func.body),
            func.locals.len()
        );
    }

    let pipeline = build_normalize_pipeline();
    let mut ctx = PassCtx {
        func,
        perf,
        diag,
        stats: None,
        decomp_facts: None,
    };
    pipeline.run(&mut ctx);

    if perf {
        let (final_stmts, final_locals) = fission_midend_core::action_pipeline::hir_shape(func);
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

#[cfg(test)]
mod tests {
    use super::*;
// prelude via parent

    #[test]
    fn normalize_pipeline_has_ghidra_ordered_groups() {
        let pipeline = build_normalize_pipeline();
        let names = pipeline.group_names();
        assert_eq!(
            names,
            vec![
                "proto_recovery",
                "deadcode_dynamic",
                "type_early",
                "stackstall",
                "heritage_value_recovery",
                "memory_recovery",
                "merge",
                "block_structure_1",
                "cleanup",
            ]
        );
    }
}
