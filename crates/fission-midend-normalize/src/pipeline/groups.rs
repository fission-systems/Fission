//! Ghidra-ordered normalize action groups and pipeline driver.

use fission_midend_core::wave_stats;
use super::stages::{
    cleanup_after_branch_prefix_hoist, cleanup_after_conditional_const,
    cleanup_after_constant_ptr_recovery, cleanup_after_entry_param_promotion,
    cleanup_after_gvn_join_hoist, cleanup_after_sccp, cleanup_elim_8, defuse_after_branch_hoist_and_prune,
    defuse_after_cse_and_prune, defuse_after_gvn_and_prune, run_stage_block_structure_1,
    run_stage_cleanup, run_stage_heritage_value_recovery, run_stage_memory_recovery,
    run_stage_merge, run_stage_proto_recovery, run_stage_stackstall, run_stage_type_early,
    sccp_is_admitted, wide_dead_assignment_and_prune,
};
use super::super::analysis::defuse::{constant_folding_pass, defuse_dead_assignment_pass};
use super::super::global_opt::{
    apply_conditional_const_pass, apply_cse_pass, apply_gvn_join_hoist_pass, apply_sccp_pass,
};
use super::super::idioms::{apply_branch_prefix_hoist_pass, remove_dead_callee_saved_param_loads};
use super::super::memory::apply_constant_ptr_recovery_pass;
use super::super::recovery::{copy_propagation_pass, join_coalescing_pass};
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
        .group(group("proto_recovery", concept).pass(stage_pass(
            "proto_recovery",
            concept,
            run_stage_proto_recovery,
        )))
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
        .group(group("stackstall", concept).pass(stage_pass(
            "stackstall",
            concept,
            run_stage_stackstall,
        )))
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
        .group(group("block_structure_1", block).pass(stage_pass(
            "block_structure_1",
            block,
            run_stage_block_structure_1,
        )))
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
