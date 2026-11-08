//! Snapshot recorder primitives.

use crate::{ExecutionSnapshot, MemoryDelta, RegisterState, SnapshotStats};
use fission_core::CONFIG;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

/// Recording status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordingStatus {
    /// Not recording
    Idle,
    /// Currently recording
    Recording,
    /// Recording paused
    Paused,
}

/// TTD Recorder - Records execution snapshots
///
/// Performance: Uses `VecDeque` for O(1) removal from front and `HashMap` for O(1) lookup.
#[derive(Debug)]
pub struct TTDRecorder {
    status: RecordingStatus,
    snapshots: VecDeque<ExecutionSnapshot>,
    snapshot_steps: HashMap<u64, ()>,
    current_step: u64,
    start_time: Option<Instant>,
    max_snapshots: usize,
    prev_registers: Option<RegisterState>,
}

impl TTDRecorder {
    /// Create a new recorder with default settings
    pub fn new() -> Self {
        Self {
            status: RecordingStatus::Idle,
            snapshots: VecDeque::new(),
            snapshot_steps: HashMap::new(),
            current_step: 0,
            start_time: None,
            max_snapshots: CONFIG.debug.max_snapshots,
            prev_registers: None,
        }
    }

    /// Create a new recorder with custom max snapshots
    pub fn with_max_snapshots(max_snapshots: usize) -> Self {
        Self {
            max_snapshots,
            ..Self::new()
        }
    }

    /// Start recording
    pub fn start_recording(&mut self) {
        self.status = RecordingStatus::Recording;
        self.start_time = Some(Instant::now());
        self.current_step = 0;
        self.snapshots.clear();
        self.snapshot_steps.clear();
        self.prev_registers = None;
    }

    /// Stop recording
    pub fn stop_recording(&mut self) {
        self.status = RecordingStatus::Idle;
    }

    /// Pause recording
    pub fn pause_recording(&mut self) {
        if self.status == RecordingStatus::Recording {
            self.status = RecordingStatus::Paused;
        }
    }

    /// Resume recording
    pub fn resume_recording(&mut self) {
        if self.status == RecordingStatus::Paused {
            self.status = RecordingStatus::Recording;
        }
    }

    fn enforce_max_snapshots(&mut self) {
        if self.snapshots.len() >= self.max_snapshots
            && let Some(oldest) = self.snapshots.pop_front()
        {
            self.snapshot_steps.remove(&oldest.step_index);
        }
    }

    /// Make room for a snapshot at `step_index`, or say there is none to make.
    ///
    /// Recording at a step at or before one already held means the run was
    /// rewound and is going on from there: the later steps belong to a timeline
    /// that no longer happened. Dropping them keeps `snapshots` strictly
    /// increasing, which `get_snapshot`'s binary search needs, and keeps
    /// `snapshot_steps` from losing a step that a newer snapshot still holds
    /// when an older duplicate is evicted.
    fn prepare_slot(&mut self, step_index: u64) -> bool {
        if self.max_snapshots == 0 {
            return false;
        }
        while let Some(back) = self.snapshots.back() {
            if back.step_index < step_index {
                break;
            }
            let stale = back.step_index;
            self.snapshots.pop_back();
            self.snapshot_steps.remove(&stale);
        }
        self.enforce_max_snapshots();
        true
    }

    /// Record a step with the current register state.
    ///
    /// `step` is the position on the *guest's* timeline -- its retired
    /// instruction count -- and not the number of snapshots taken so far.
    /// Those were the same field once, which meant a seek to "step 200" found
    /// the two-hundredth snapshot and then told the emulator its instruction
    /// count was 200. rr keeps the two apart by construction: a position there
    /// is a `MarkKey` of trace time and tick count, never a bare integer that
    /// two layers are free to read differently.
    pub fn record_step(
        &mut self,
        step: u64,
        registers: RegisterState,
        thread_id: u32,
    ) -> Option<u64> {
        if self.status != RecordingStatus::Recording {
            return None;
        }

        let step_index = step;
        let snapshot = ExecutionSnapshot::new(step_index, registers.clone(), thread_id);

        if !self.prepare_slot(step_index) {
            return None;
        }
        self.snapshot_steps.insert(step_index, ());
        self.snapshots.push_back(snapshot);
        self.prev_registers = Some(registers);
        self.current_step += 1;

        Some(step_index)
    }

    /// Record a step with memory changes
    pub fn record_step_with_memory(
        &mut self,
        step: u64,
        registers: RegisterState,
        thread_id: u32,
        memory_deltas: Vec<MemoryDelta>,
        shadow_deltas: Vec<crate::ShadowDelta>,
    ) -> Option<u64> {
        if self.status != RecordingStatus::Recording {
            return None;
        }

        let step_index = step;
        let mut snapshot = ExecutionSnapshot::new(step_index, registers.clone(), thread_id);
        for delta in memory_deltas {
            snapshot.add_memory_delta(delta);
        }
        for delta in shadow_deltas {
            snapshot.add_shadow_delta(delta);
        }

        if !self.prepare_slot(step_index) {
            return None;
        }
        self.snapshot_steps.insert(step_index, ());
        self.snapshots.push_back(snapshot);
        self.prev_registers = Some(registers);
        self.current_step += 1;

        Some(step_index)
    }

    /// Get a snapshot by step index
    pub fn get_snapshot(&self, step_index: u64) -> Option<&ExecutionSnapshot> {
        if !self.snapshot_steps.contains_key(&step_index) {
            return None;
        }

        self.snapshots
            .binary_search_by_key(&step_index, |s| s.step_index)
            .ok()
            .and_then(|idx| self.snapshots.get(idx))
    }

    /// The latest snapshot taken at or before `step`.
    pub fn snapshot_at_or_before(&self, step: u64) -> Option<&ExecutionSnapshot> {
        let after = self.snapshots.partition_point(|s| s.step_index <= step);
        after.checked_sub(1).and_then(|idx| self.snapshots.get(idx))
    }

    /// The earliest snapshot taken at or after `step`.
    pub fn snapshot_at_or_after(&self, step: u64) -> Option<&ExecutionSnapshot> {
        let idx = self.snapshots.partition_point(|s| s.step_index < step);
        self.snapshots.get(idx)
    }

    /// Get the latest snapshot
    pub fn latest_snapshot(&self) -> Option<&ExecutionSnapshot> {
        self.snapshots.back()
    }

    /// Get the first snapshot
    pub fn first_snapshot(&self) -> Option<&ExecutionSnapshot> {
        self.snapshots.front()
    }

    /// Get the step range (min_step, max_step) covered by recorded snapshots
    pub fn step_range(&self) -> Option<(u64, u64)> {
        let first = self.snapshots.front()?.step_index;
        let last = self.snapshots.back()?.step_index;
        Some((first, last))
    }

    /// Get all snapshots as borrowed references
    pub fn snapshots(&self) -> Vec<&ExecutionSnapshot> {
        self.snapshots.iter().collect()
    }

    /// Get an iterator over all snapshots
    pub fn snapshots_iter(&self) -> impl Iterator<Item = &ExecutionSnapshot> {
        self.snapshots.iter()
    }

    pub fn status(&self) -> RecordingStatus {
        self.status
    }

    pub fn is_recording(&self) -> bool {
        self.status == RecordingStatus::Recording
    }

    pub fn step_count(&self) -> u64 {
        self.current_step
    }

    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    pub fn duration(&self) -> Option<std::time::Duration> {
        self.start_time.map(|t| t.elapsed())
    }

    pub fn stats(&self) -> SnapshotStats {
        let count = self.snapshots.len() as u64;
        let memory_bytes: usize = self.snapshots.iter().map(|s| s.memory_usage()).sum();
        let avg_deltas = if count > 0 {
            self.snapshots
                .iter()
                .map(|s| s.memory_deltas.len())
                .sum::<usize>() as f64
                / count as f64
        } else {
            0.0
        };

        SnapshotStats {
            count,
            memory_bytes,
            avg_deltas_per_snapshot: avg_deltas,
        }
    }

    pub fn clear(&mut self) {
        self.snapshots.clear();
        self.snapshot_steps.clear();
        self.current_step = 0;
        self.start_time = None;
        self.prev_registers = None;
        self.status = RecordingStatus::Idle;
    }
}

impl Default for TTDRecorder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recording() -> TTDRecorder {
        let mut rec = TTDRecorder::new();
        rec.start_recording();
        rec
    }

    fn record(rec: &mut TTDRecorder, step: u64) {
        rec.record_step(step, RegisterState::at(0x1000 + step), 0);
    }

    #[test]
    fn recording_in_order_keeps_every_snapshot_reachable() {
        // Positive control for the test below: ordinary forward recording must
        // keep everything, so truncating on re-record cannot pass by dropping
        // history it should have kept.
        let mut rec = recording();
        for step in [0, 10, 20] {
            record(&mut rec, step);
        }
        assert_eq!(rec.snapshot_count(), 3);
        for step in [0, 10, 20] {
            assert!(rec.get_snapshot(step).is_some(), "step {step} lost");
        }
    }

    #[test]
    fn recording_again_after_a_rewind_replaces_the_later_history() {
        // Time travel back to step 10 and run on: the old steps 10 and 20 are
        // a timeline that no longer happened. Appending a second step 10 left
        // the list unsorted, which `get_snapshot`'s binary search requires.
        let mut rec = recording();
        for step in [0, 10, 20] {
            record(&mut rec, step);
        }
        record(&mut rec, 10);
        record(&mut rec, 15);

        let steps: Vec<u64> = rec.snapshots_iter().map(|s| s.step_index).collect();
        assert_eq!(steps, vec![0, 10, 15], "history is not strictly increasing");
        assert!(rec.get_snapshot(20).is_none(), "an abandoned step survived");
        for step in [0, 10, 15] {
            assert!(rec.get_snapshot(step).is_some(), "step {step} unreachable");
        }
    }

    #[test]
    fn eviction_cannot_make_a_duplicate_step_unreachable() {
        let mut rec = TTDRecorder::with_max_snapshots(2);
        rec.start_recording();
        record(&mut rec, 5);
        record(&mut rec, 5);
        record(&mut rec, 6);
        assert!(rec.get_snapshot(5).is_some() || rec.get_snapshot(6).is_some());
        let steps: Vec<u64> = rec.snapshots_iter().map(|s| s.step_index).collect();
        for step in steps {
            assert!(rec.get_snapshot(step).is_some(), "step {step} unreachable");
        }
    }

    #[test]
    fn a_zero_snapshot_budget_keeps_nothing() {
        let mut rec = TTDRecorder::with_max_snapshots(0);
        rec.start_recording();
        record(&mut rec, 1);
        assert_eq!(rec.snapshot_count(), 0);
    }

    #[test]
    fn snapshot_lookup_at_or_before_and_after_a_step() {
        let mut rec = recording();
        for step in [0, 4, 9, 15] {
            record(&mut rec, step);
        }
        assert_eq!(rec.snapshot_at_or_before(14).map(|s| s.step_index), Some(9));
        assert_eq!(rec.snapshot_at_or_before(9).map(|s| s.step_index), Some(9));
        assert_eq!(rec.snapshot_at_or_after(10).map(|s| s.step_index), Some(15));
        assert_eq!(rec.snapshot_at_or_after(15).map(|s| s.step_index), Some(15));
        assert!(rec.snapshot_at_or_after(16).is_none());
    }
}
