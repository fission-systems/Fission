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

pub(super) fn add_interproc_constraint_rounds(n: usize) {
    if n == 0 {
        return;
    }
    WAVE.with(|w| w.borrow_mut().interproc_signature_constraint_rounds += n);
}
