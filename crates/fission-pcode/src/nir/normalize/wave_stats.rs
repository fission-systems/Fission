//! Counters for normalize-only “quality wave” passes (merged into [`crate::nir::types::NirBuildStats`]).
use crate::nir::types::NirBuildStats;
use std::cell::RefCell;

thread_local! {
    static WAVE: RefCell<NirBuildStats> = RefCell::new(NirBuildStats::default());
}

pub(super) fn reset_normalize_wave_stats() {
    WAVE.with(|w| {
        *w.borrow_mut() = NirBuildStats::default();
    });
}

pub(super) fn take_normalize_wave_stats() -> NirBuildStats {
    WAVE.with(|w| std::mem::take(&mut *w.borrow_mut()))
}

pub(super) fn add_entry_param_promotions(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().entry_param_promotion_spill_rename_count += n);
}

pub(super) fn add_variadic_stack_region_folds(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().variadic_stack_region_fold_count += n);
}

pub(super) fn add_abi_slot_recoveries(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().abi_slot_recovered_count += n);
}

pub(super) fn add_home_slot_promotions(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().home_slot_promoted_count += n);
}

pub(super) fn add_va_start_recoveries(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().va_start_recovered_count += n);
}

pub(super) fn add_call_signature_refinements(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().call_signature_refined_count += n);
}

pub(super) fn add_security_cookie_folds(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().security_cookie_fold_count += n);
}

pub(super) fn add_call_artifact_removals(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().call_artifact_removed_count += n);
}

pub(super) fn add_object_shape_recoveries(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().object_shape_recovered_count += n);
}

pub(super) fn add_object_root_recoveries(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().object_root_recovered_count += n);
}

pub(super) fn add_typed_fact_evidences(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().typed_fact_evidence_count += n);
}

pub(super) fn add_typed_fact_conflicts(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().typed_fact_conflict_count += n);
}

pub(super) fn add_object_root_fact_promotions(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().object_root_fact_promotion_count += n);
}

pub(super) fn add_typed_object_shape_refinements(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().typed_object_shape_refined_count += n);
}

pub(super) fn add_surface_binding_promotions(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().surface_binding_promoted_count += n);
}

pub(super) fn add_surface_fact_promotions(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().surface_fact_promotion_count += n);
}

pub(super) fn add_prototype_summary_refinements(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().prototype_summary_refined_count += n);
}

pub(super) fn add_prototype_summary_rounds(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().prototype_summary_round_count += n);
}

pub(super) fn add_call_effect_summary_refinements(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().call_effect_summary_refined_count += n);
}

pub(super) fn add_wrapper_summary_folds(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().wrapper_summary_fold_count += n);
}

pub(super) fn add_cleanup_budget_skips(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().cleanup_budget_skip_count += n);
}

pub(super) fn add_cleanup_family_binding_init(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().cleanup_family_binding_init_count += n);
}

pub(super) fn add_cleanup_family_stmt_canonical(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().cleanup_family_stmt_canonical_count += n);
}

pub(super) fn add_cleanup_stmt_fold(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().cleanup_stmt_fold_count += n);
}

pub(super) fn add_cleanup_boundary_label(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().cleanup_boundary_label_count += n);
}

pub(super) fn add_cleanup_loopish_rewrite(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().cleanup_loopish_rewrite_count += n);
}

pub(super) fn add_cleanup_family_dead_binding(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().cleanup_family_dead_binding_count += n);
}

pub(super) fn add_interproc_constraint_rounds(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().interproc_signature_constraint_rounds += n);
}

pub(crate) fn add_materialization_stabilized(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().materialization_stabilized_count += n);
}

pub(crate) fn add_pass_rerun_skipped_by_preservation(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().pass_rerun_skipped_by_preservation_count += n);
}

pub(crate) fn add_indirect_target_set_refinements(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().indirect_target_set_refined_count += n);
}

pub(crate) fn add_dispatcher_shape_recoveries(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().dispatcher_shape_recovered_count += n);
}

pub(super) fn add_pass_metric(
    name: &str,
    elapsed_ms: f64,
    changed: bool,
    before_stmts: usize,
    after_stmts: usize,
    before_locals: usize,
    after_locals: usize,
) {
    metrics::counter!("fission.normalize.pass.invocations_total", "pass" => name.to_string())
        .increment(1);
    metrics::histogram!("fission.normalize.pass.duration_ms", "pass" => name.to_string())
        .record(elapsed_ms);
    let changed_metric = if changed { "changed" } else { "unchanged" };
    metrics::counter!(
        "fission.normalize.pass.outcome_total",
        "pass" => name.to_string(),
        "outcome" => changed_metric
    )
    .increment(1);
    WAVE.with(|w| {
        let mut stats = w.borrow_mut();
        let agg = stats.pass_metrics.entry(name.to_string()).or_default();
        agg.total_time_ms += elapsed_ms;
        agg.total_invocations += 1;
        if changed {
            agg.changed_count += 1;
        }
        agg.stmts_reduced += (before_stmts as isize) - (after_stmts as isize);
        agg.locals_reduced += (before_locals as isize) - (after_locals as isize);
    });
}
