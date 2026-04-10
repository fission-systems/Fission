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

pub(super) fn add_interproc_constraint_rounds(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().interproc_signature_constraint_rounds += n);
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
