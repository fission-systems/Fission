use super::*;
use std::cell::RefCell;

thread_local! {
    static LAST_PREVIEW_BUILD_STATS: RefCell<Option<PreviewBuildStats>> = const { RefCell::new(None) };
    static LAST_PREVIEW_HINT_STATS: RefCell<Option<PreviewHintStats>> = const { RefCell::new(None) };
}

pub(super) fn reset_preview_telemetry() {
    LAST_PREVIEW_BUILD_STATS.with(|slot| {
        *slot.borrow_mut() = None;
    });
    LAST_PREVIEW_HINT_STATS.with(|slot| {
        *slot.borrow_mut() = None;
    });
}

pub(super) fn store_preview_build_stats(stats: PreviewBuildStats) {
    LAST_PREVIEW_BUILD_STATS.with(|slot| {
        *slot.borrow_mut() = Some(stats);
    });
}

pub(super) fn store_preview_hint_stats(stats: PreviewHintStats) {
    LAST_PREVIEW_HINT_STATS.with(|slot| {
        *slot.borrow_mut() = Some(stats);
    });
}

pub fn take_last_preview_build_stats() -> Option<PreviewBuildStats> {
    LAST_PREVIEW_BUILD_STATS.with(|slot| slot.borrow_mut().take())
}

pub fn take_last_preview_hint_stats() -> Option<PreviewHintStats> {
    LAST_PREVIEW_HINT_STATS.with(|slot| slot.borrow_mut().take())
}

pub fn take_last_nir_build_stats() -> Option<NirBuildStats> {
    take_last_preview_build_stats()
}

pub fn take_last_nir_hint_stats() -> Option<NirHintStats> {
    take_last_preview_hint_stats()
}
