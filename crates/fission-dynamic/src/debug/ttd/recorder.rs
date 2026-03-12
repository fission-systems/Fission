//! TTD Recorder - Records execution step by step.

use super::super::types::RegisterState;
use super::snapshot::{ExecutionSnapshot, MemoryDelta, SnapshotStats};
use crate::config::CONFIG;
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
/// Performance: Uses VecDeque for O(1) removal from front and HashMap for O(1) lookup by step index.
/// The HashMap stores step_index directly (not Vec index) to avoid rebuilding on removal.
#[derive(Debug)]
pub struct TTDRecorder {
    /// Current recording status
    status: RecordingStatus,
    /// All recorded snapshots in order (VecDeque for O(1) front removal)
    snapshots: VecDeque<ExecutionSnapshot>,
    /// Index for O(1) lookup: step_index -> snapshot reference via step_index (not vec index)
    /// We store step_index as the key and use the snapshots VecDeque for actual data
    snapshot_steps: HashMap<u64, ()>,
    /// Current step index
    current_step: u64,
    /// Recording start time
    start_time: Option<Instant>,
    /// Maximum number of snapshots to keep (memory limit)
    max_snapshots: usize,
    /// Previous register state for delta detection
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

    /// Helper: Enforce max snapshots limit by removing oldest - O(1) operation
    fn enforce_max_snapshots(&mut self) {
        if self.snapshots.len() >= self.max_snapshots {
            if let Some(oldest) = self.snapshots.pop_front() {
                self.snapshot_steps.remove(&oldest.step_index);
            }
        }
    }

    /// Record a step with the current register state
    pub fn record_step(&mut self, registers: RegisterState, thread_id: u32) -> Option<u64> {
        if self.status != RecordingStatus::Recording {
            return None;
        }

        let step_index = self.current_step;
        let snapshot = ExecutionSnapshot::new(step_index, registers.clone(), thread_id);

        // O(1) removal from front of VecDeque
        self.enforce_max_snapshots();

        self.snapshot_steps.insert(step_index, ());
        self.snapshots.push_back(snapshot);
        self.prev_registers = Some(registers);
        self.current_step += 1;

        Some(step_index)
    }

    /// Record a step with memory changes
    pub fn record_step_with_memory(
        &mut self,
        registers: RegisterState,
        thread_id: u32,
        memory_deltas: Vec<MemoryDelta>,
    ) -> Option<u64> {
        if self.status != RecordingStatus::Recording {
            return None;
        }

        let step_index = self.current_step;
        let mut snapshot = ExecutionSnapshot::new(step_index, registers.clone(), thread_id);

        for delta in memory_deltas {
            snapshot.add_memory_delta(delta);
        }

        // O(1) removal from front of VecDeque
        self.enforce_max_snapshots();

        self.snapshot_steps.insert(step_index, ());
        self.snapshots.push_back(snapshot);
        self.prev_registers = Some(registers);
        self.current_step += 1;

        Some(step_index)
    }

    /// Get a snapshot by step index using O(1) HashMap check + binary search
    ///
    /// Since snapshots are stored in order by step_index, we can use binary search
    /// after verifying the step exists in our index.
    pub fn get_snapshot(&self, step_index: u64) -> Option<&ExecutionSnapshot> {
        // O(1) check if step exists
        if !self.snapshot_steps.contains_key(&step_index) {
            return None;
        }

        // Binary search for the snapshot (snapshots are in order)
        self.snapshots
            .binary_search_by_key(&step_index, |s| s.step_index)
            .ok()
            .and_then(|idx| self.snapshots.get(idx))
    }

    /// Get the latest snapshot
    pub fn latest_snapshot(&self) -> Option<&ExecutionSnapshot> {
        self.snapshots.back()
    }

    /// Get all snapshots as a slice
    ///
    /// Note: VecDeque uses make_contiguous internally for slicing operations.
    /// For iteration, prefer using the iterator directly: `recorder.snapshots_iter()`
    pub fn snapshots(&self) -> Vec<&ExecutionSnapshot> {
        self.snapshots.iter().collect()
    }

    /// Get an iterator over all snapshots (more efficient than snapshots() for iteration)
    pub fn snapshots_iter(&self) -> impl Iterator<Item = &ExecutionSnapshot> {
        self.snapshots.iter()
    }

    /// Get current recording status
    pub fn status(&self) -> RecordingStatus {
        self.status
    }

    /// Is currently recording?
    pub fn is_recording(&self) -> bool {
        self.status == RecordingStatus::Recording
    }

    /// Get current step count
    pub fn step_count(&self) -> u64 {
        self.current_step
    }

    /// Get snapshot count (may be less than step_count due to max limit)
    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    /// Get recording duration
    pub fn duration(&self) -> Option<std::time::Duration> {
        self.start_time.map(|t| t.elapsed())
    }

    /// Get statistics about the recording
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

    /// Clear all recordings
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
