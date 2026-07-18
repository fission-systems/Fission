//! Ghidra-ordered normalize action groups and pipeline driver.

use fission_midend_core::wave_stats;
use super::stages::{
    cleanup_after_constant_ptr_recovery, run_stage_block_structure_1, run_stage_cleanup,
    run_stage_deadcode_dynamic, run_stage_heritage_value_recovery, run_stage_memory_recovery,
    run_stage_merge, run_stage_proto_recovery, run_stage_stackstall, run_stage_type_early,
};
use super::super::memory::apply_constant_ptr_recovery_pass;
use fission_midend_core::action_pipeline::{
    GhidraActionConcept, Pass, PassCtx, PassOutcome, Pipeline, cleanup_pass, fn_pass,
    gated_followup, group,
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
                .pass(stage_pass(
                    "deadcode_dynamic",
                    concept,
                    run_stage_deadcode_dynamic,
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
