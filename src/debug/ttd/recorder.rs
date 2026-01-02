//! TTD Recorder - Records execution step by step.

use std::collections::HashMap;
use std::time::Instant;
use super::snapshot::{ExecutionSnapshot, MemoryDelta, SnapshotStats};
use super::super::types::RegisterState;
use crate::config::CONFIG;

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
/// Performance: Uses both a Vec for ordering and a HashMap for O(1) lookup by step index.
#[derive(Debug)]
pub struct TTDRecorder {
    /// Current recording status
    status: RecordingStatus,
    /// All recorded snapshots in order
    snapshots: Vec<ExecutionSnapshot>,
    /// Index for O(1) lookup by step index
    snapshot_index: HashMap<u64, usize>,
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
            snapshots: Vec::new(),
            snapshot_index: HashMap::new(),
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
        self.snapshot_index.clear();
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
    
    /// Record a step with the current register state
    pub fn record_step(&mut self, registers: RegisterState, thread_id: u32) -> Option<u64> {
        if self.status != RecordingStatus::Recording {
            return None;
        }
        
        let step_index = self.current_step;
        let snapshot = ExecutionSnapshot::new(step_index, registers.clone(), thread_id);
        
        // Enforce max snapshots limit (remove oldest)
        if self.snapshots.len() >= self.max_snapshots {
            if let Some(oldest) = self.snapshots.first() {
                self.snapshot_index.remove(&oldest.step_index);
            }
            self.snapshots.remove(0);
            // Rebuild index after removal (indices shift)
            self.rebuild_index();
        }
        
        let idx = self.snapshots.len();
        self.snapshot_index.insert(step_index, idx);
        self.snapshots.push(snapshot);
        self.prev_registers = Some(registers);
        self.current_step += 1;
        
        Some(step_index)
    }
    
    /// Rebuild the snapshot index after removals
    fn rebuild_index(&mut self) {
        self.snapshot_index.clear();
        for (idx, snapshot) in self.snapshots.iter().enumerate() {
            self.snapshot_index.insert(snapshot.step_index, idx);
        }
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
        
        // Enforce max snapshots limit
        if self.snapshots.len() >= self.max_snapshots {
            if let Some(oldest) = self.snapshots.first() {
                self.snapshot_index.remove(&oldest.step_index);
            }
            self.snapshots.remove(0);
            self.rebuild_index();
        }
        
        let idx = self.snapshots.len();
        self.snapshot_index.insert(step_index, idx);
        self.snapshots.push(snapshot);
        self.prev_registers = Some(registers);
        self.current_step += 1;
        
        Some(step_index)
    }
    
    /// Get a snapshot by step index using O(1) HashMap lookup
    pub fn get_snapshot(&self, step_index: u64) -> Option<&ExecutionSnapshot> {
        self.snapshot_index
            .get(&step_index)
            .and_then(|&idx| self.snapshots.get(idx))
    }
    
    /// Get the latest snapshot
    pub fn latest_snapshot(&self) -> Option<&ExecutionSnapshot> {
        self.snapshots.last()
    }
    
    /// Get all snapshots
    pub fn snapshots(&self) -> &[ExecutionSnapshot] {
        &self.snapshots
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
            self.snapshots.iter().map(|s| s.memory_deltas.len()).sum::<usize>() as f64 / count as f64
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
        self.snapshot_index.clear();
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
