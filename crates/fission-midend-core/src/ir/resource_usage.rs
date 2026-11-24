//! Observed use of the deterministic work limits (see [`ResourceLimits`]).
//!
//! This is telemetry only: nothing here feeds back into a decision. Each limit
//! keeps its own counter where it is enforced; this records how far those
//! counters got, so a limit can be judged against real functions.
//!
//! One [`ResourceUsage`] is shared (behind an `Rc`) by a builder and every
//! trial clone or isolated fork of it, so the peaks cover all the work that
//! was attempted, including work later discarded.

use super::ResourceLimits;
use std::cell::Cell;

/// The highest value a counter reached, and how often it crossed its limit.
#[derive(Debug, Default)]
pub struct PeakMeter {
    peak: Cell<u64>,
    trips: Cell<usize>,
}

impl PeakMeter {
    /// Record that the counter now reads `value` against `limit`.
    ///
    /// Counters advance one unit at a time, so crossing the limit is the step
    /// that reads `limit + 1`; counting only that step records one trip per
    /// crossing instead of one per call made after it. A limit of `u64::MAX`
    /// cannot be crossed.
    pub fn observe(&self, value: u64, limit: u64) {
        if value > self.peak.get() {
            self.peak.set(value);
        }
        if limit.checked_add(1) == Some(value) {
            self.trips.set(self.trips.get() + 1);
        }
    }

    pub fn peak(&self) -> u64 {
        self.peak.get()
    }

    pub fn trips(&self) -> usize {
        self.trips.get()
    }

    /// Run `f` and return, with its result, the highest value observed while
    /// it ran (0 when nothing was observed).
    ///
    /// The meter keeps its cumulative peak: afterwards it reads exactly what
    /// it would have read had `f` been run without the window.
    pub fn peak_within<T>(&self, f: impl FnOnce() -> T) -> (T, u64) {
        struct Restore<'a> {
            meter: &'a PeakMeter,
            before: u64,
        }
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                let during = self.meter.peak.get();
                self.meter.peak.set(self.before.max(during));
            }
        }
        let restore = Restore {
            meter: self,
            before: self.peak.replace(0),
        };
        let result = f();
        let during = self.peak.get();
        drop(restore);
        (result, during)
    }
}

/// One [`PeakMeter`] per limit in [`ResourceLimits`].
#[derive(Debug, Default)]
pub struct ResourceUsage {
    pub structuring_total_work: PeakMeter,
    pub condition_recovery_subcalls: PeakMeter,
    pub sese_region_proof_calls: PeakMeter,
    pub varnode_lowering_work: PeakMeter,
}

impl ResourceUsage {
    /// Every meter's trip count, in field order.
    pub fn trip_counts(&self) -> [usize; 4] {
        [
            self.structuring_total_work.trips(),
            self.condition_recovery_subcalls.trips(),
            self.sese_region_proof_calls.trips(),
            self.varnode_lowering_work.trips(),
        ]
    }

    /// Write the observed peaks and trips into the public stats.
    pub fn apply_to_stats(&self, stats: &mut super::NirBuildStats) {
        stats.resource_structuring_total_work_peak = self.structuring_total_work.peak();
        stats.resource_structuring_total_work_trip_count = self.structuring_total_work.trips();
        stats.resource_condition_recovery_subcalls_peak = self.condition_recovery_subcalls.peak();
        stats.resource_condition_recovery_subcalls_trip_count =
            self.condition_recovery_subcalls.trips();
        stats.resource_sese_region_proof_calls_peak = self.sese_region_proof_calls.peak();
        stats.resource_sese_region_proof_calls_trip_count = self.sese_region_proof_calls.trips();
        stats.resource_varnode_lowering_work_peak = self.varnode_lowering_work.peak();
        stats.resource_varnode_lowering_work_trip_count = self.varnode_lowering_work.trips();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_tracks_the_highest_value_and_trips_count_crossings() {
        let meter = PeakMeter::default();
        for value in 1..=5 {
            meter.observe(value, 3);
        }
        assert_eq!(meter.peak(), 5);
        assert_eq!(
            meter.trips(),
            1,
            "only the step reading limit + 1 is a crossing"
        );
        // a reset counter that climbs past the limit again is a second crossing
        for value in 1..=4 {
            meter.observe(value, 3);
        }
        assert_eq!(meter.trips(), 2);
        assert_eq!(meter.peak(), 5);
    }

    #[test]
    fn peak_within_reports_the_window_and_keeps_the_cumulative_peak() {
        let meter = PeakMeter::default();
        meter.observe(9, 100);
        let ((), during) = meter.peak_within(|| {
            meter.observe(3, 100);
            meter.observe(5, 100);
        });
        assert_eq!(during, 5, "the window's own highest value");
        assert_eq!(meter.peak(), 9, "an earlier, higher peak survives");

        let ((), during) = meter.peak_within(|| meter.observe(12, 100));
        assert_eq!(during, 12);
        assert_eq!(meter.peak(), 12, "a higher peak inside the window is kept");

        let ((), during) = meter.peak_within(|| {});
        assert_eq!(during, 0, "nothing observed");
        assert_eq!(meter.peak(), 12);
    }

    #[test]
    fn a_limit_at_the_integer_ceiling_never_overflows() {
        let meter = PeakMeter::default();
        meter.observe(u64::MAX, u64::MAX);
        assert_eq!(meter.peak(), u64::MAX);
        assert_eq!(meter.trips(), 0);
    }

    #[test]
    fn usage_is_written_to_the_public_stats() {
        let usage = ResourceUsage::default();
        usage.structuring_total_work.observe(7, 5);
        usage.condition_recovery_subcalls.observe(2, 512);
        usage.sese_region_proof_calls.observe(9, 8);
        usage.varnode_lowering_work.observe(1, 20_000);
        let mut stats = super::super::NirBuildStats::default();
        usage.apply_to_stats(&mut stats);
        assert_eq!(stats.resource_structuring_total_work_peak, 7);
        assert_eq!(stats.resource_structuring_total_work_trip_count, 0);
        assert_eq!(stats.resource_condition_recovery_subcalls_peak, 2);
        assert_eq!(stats.resource_sese_region_proof_calls_peak, 9);
        assert_eq!(stats.resource_sese_region_proof_calls_trip_count, 1);
        assert_eq!(stats.resource_varnode_lowering_work_peak, 1);
    }
}
