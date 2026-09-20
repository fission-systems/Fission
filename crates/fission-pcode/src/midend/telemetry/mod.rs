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

pub fn last_preview_build_stats() -> Option<PreviewBuildStats> {
    LAST_PREVIEW_BUILD_STATS.with(|slot| slot.borrow().clone())
}

pub fn last_preview_hint_stats() -> Option<PreviewHintStats> {
    LAST_PREVIEW_HINT_STATS.with(|slot| slot.borrow().clone())
}

pub fn last_nir_build_stats() -> Option<NirBuildStats> {
    last_preview_build_stats()
}

pub fn last_nir_hint_stats() -> Option<NirHintStats> {
    last_preview_hint_stats()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_observation_does_not_drain_the_slot() {
        reset_preview_telemetry();
        store_preview_build_stats(PreviewBuildStats {
            rendered_code_len: 7,
            ..PreviewBuildStats::default()
        });

        assert_eq!(last_preview_build_stats().unwrap().rendered_code_len, 7);
        assert_eq!(last_nir_build_stats().unwrap().rendered_code_len, 7);

        reset_preview_telemetry();
    }
}
