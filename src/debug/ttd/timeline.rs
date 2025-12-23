//! Timeline - Manages recorded history and navigation.

use super::snapshot::ExecutionSnapshot;
use super::recorder::TTDRecorder;

/// Timeline navigation result
#[derive(Debug, Clone)]
pub enum SeekResult {
    /// Successfully seeked to the target
    Success(ExecutionSnapshot),
    /// Target is out of bounds
    OutOfBounds { min: u64, max: u64, requested: u64 },
    /// No snapshots available
    Empty,
}

/// Timeline for navigating recorded execution history
#[derive(Debug)]
pub struct Timeline {
    /// The underlying recorder
    recorder: TTDRecorder,
    /// Current position in the timeline (snapshot index)
    current_position: Option<u64>,
    /// Are we in replay mode?
    replay_mode: bool,
}

impl Timeline {
    /// Create a new timeline with a fresh recorder
    pub fn new() -> Self {
        Self {
            recorder: TTDRecorder::new(),
            current_position: None,
            replay_mode: false,
        }
    }
    
    /// Create a timeline from an existing recorder
    pub fn from_recorder(recorder: TTDRecorder) -> Self {
        let position = recorder.latest_snapshot().map(|s| s.step_index);
        Self {
            recorder,
            current_position: position,
            replay_mode: false,
        }
    }
    
    /// Get mutable access to the recorder
    pub fn recorder_mut(&mut self) -> &mut TTDRecorder {
        &mut self.recorder
    }
    
    /// Get read access to the recorder
    pub fn recorder(&self) -> &TTDRecorder {
        &self.recorder
    }
    
    /// Start recording (delegates to recorder)
    pub fn start_recording(&mut self) {
        self.recorder.start_recording();
        self.current_position = None;
        self.replay_mode = false;
    }
    
    /// Stop recording (delegates to recorder)
    pub fn stop_recording(&mut self) {
        self.recorder.stop_recording();
        self.current_position = self.recorder.latest_snapshot().map(|s| s.step_index);
    }
    
    /// Enter replay mode at current position
    pub fn enter_replay_mode(&mut self) {
        if self.recorder.snapshot_count() > 0 {
            self.replay_mode = true;
            if self.current_position.is_none() {
                self.current_position = self.recorder.latest_snapshot().map(|s| s.step_index);
            }
        }
    }
    
    /// Exit replay mode
    pub fn exit_replay_mode(&mut self) {
        self.replay_mode = false;
    }
    
    /// Is in replay mode?
    pub fn is_replay_mode(&self) -> bool {
        self.replay_mode
    }
    
    /// Seek to a specific step index
    pub fn seek_to(&mut self, step_index: u64) -> SeekResult {
        if self.recorder.snapshot_count() == 0 {
            return SeekResult::Empty;
        }
        
        let snapshots = self.recorder.snapshots();
        let min_step = snapshots.first().map(|s| s.step_index).unwrap_or(0);
        let max_step = snapshots.last().map(|s| s.step_index).unwrap_or(0);
        
        if step_index < min_step || step_index > max_step {
            return SeekResult::OutOfBounds {
                min: min_step,
                max: max_step,
                requested: step_index,
            };
        }
        
        if let Some(snapshot) = self.recorder.get_snapshot(step_index) {
            self.current_position = Some(step_index);
            self.replay_mode = true;
            SeekResult::Success(snapshot.clone())
        } else {
            // Step exists in range but not in snapshots (may have been pruned)
            SeekResult::OutOfBounds {
                min: min_step,
                max: max_step,
                requested: step_index,
            }
        }
    }
    
    /// Rewind N steps from current position
    pub fn rewind(&mut self, steps: u64) -> SeekResult {
        match self.current_position {
            Some(pos) => {
                let target = pos.saturating_sub(steps);
                self.seek_to(target)
            }
            None => SeekResult::Empty,
        }
    }
    
    /// Forward N steps from current position
    pub fn forward(&mut self, steps: u64) -> SeekResult {
        match self.current_position {
            Some(pos) => {
                let target = pos.saturating_add(steps);
                self.seek_to(target)
            }
            None => SeekResult::Empty,
        }
    }
    
    /// Go to the first snapshot
    pub fn seek_start(&mut self) -> SeekResult {
        if let Some(first) = self.recorder.snapshots().first() {
            self.seek_to(first.step_index)
        } else {
            SeekResult::Empty
        }
    }
    
    /// Go to the last snapshot
    pub fn seek_end(&mut self) -> SeekResult {
        if let Some(last) = self.recorder.snapshots().last() {
            self.seek_to(last.step_index)
        } else {
            SeekResult::Empty
        }
    }
    
    /// Get current position
    pub fn current_position(&self) -> Option<u64> {
        self.current_position
    }
    
    /// Get current snapshot
    pub fn current_snapshot(&self) -> Option<&ExecutionSnapshot> {
        self.current_position
            .and_then(|pos| self.recorder.get_snapshot(pos))
    }
    
    /// Get the range of available steps
    pub fn step_range(&self) -> Option<(u64, u64)> {
        let snapshots = self.recorder.snapshots();
        if snapshots.is_empty() {
            return None;
        }
        let min = snapshots.first().map(|s| s.step_index).unwrap_or(0);
        let max = snapshots.last().map(|s| s.step_index).unwrap_or(0);
        Some((min, max))
    }
    
    /// Get total number of available snapshots
    pub fn snapshot_count(&self) -> usize {
        self.recorder.snapshot_count()
    }
    
    /// Clear the timeline
    pub fn clear(&mut self) {
        self.recorder.clear();
        self.current_position = None;
        self.replay_mode = false;
    }
}

impl Default for Timeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::debug::types::RegisterState;
    
    #[test]
    fn test_timeline_basic() {
        let mut timeline = Timeline::new();
        timeline.start_recording();
        
        // Record some steps
        for i in 0..5 {
            let mut regs = RegisterState::default();
            regs.rip = 0x401000 + i * 4;
            timeline.recorder_mut().record_step(regs, 1);
        }
        
        timeline.stop_recording();
        
        // Check range
        assert_eq!(timeline.step_range(), Some((0, 4)));
        assert_eq!(timeline.snapshot_count(), 5);
        
        // Seek to middle
        if let SeekResult::Success(snap) = timeline.seek_to(2) {
            assert_eq!(snap.step_index, 2);
            assert_eq!(snap.registers.rip, 0x401008);
        } else {
            panic!("Seek failed");
        }
        
        // Rewind
        if let SeekResult::Success(snap) = timeline.rewind(1) {
            assert_eq!(snap.step_index, 1);
        } else {
            panic!("Rewind failed");
        }
        
        // Forward
        if let SeekResult::Success(snap) = timeline.forward(2) {
            assert_eq!(snap.step_index, 3);
        } else {
            panic!("Forward failed");
        }
    }
}
