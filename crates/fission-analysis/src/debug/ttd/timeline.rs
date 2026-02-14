//! Timeline - Manages recorded history and navigation.

use super::recorder::TTDRecorder;
use super::snapshot::{ExecutionSnapshot, SnapshotStats};
#[cfg(target_os = "linux")]
use crate::debug::rr::RRDebugger;

/// Timeline navigation result
#[derive(Debug, Clone)]
pub enum SeekResult {
    Success(ExecutionSnapshot),
    OutOfBounds { min: u64, max: u64, requested: u64 },
    Empty,
}

#[derive(Debug)]
pub enum Backend {
    Internal(TTDRecorder),
    #[cfg(target_os = "linux")]
    RR(RRDebugger),
}

impl Default for Backend {
    fn default() -> Self {
        Self::Internal(TTDRecorder::new())
    }
}

/// Timeline for navigating recorded execution history
#[derive(Debug, Default)]
pub struct Timeline {
    backend: Backend,
    current_position: Option<u64>,
    replay_mode: bool,
    current_snapshot: Option<Box<ExecutionSnapshot>>,
}

impl Timeline {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(target_os = "linux")]
    pub fn new_rr(rr: RRDebugger) -> Self {
        Self {
            backend: Backend::RR(rr),
            current_position: None,
            replay_mode: true,
            current_snapshot: None,
        }
    }

    pub fn from_recorder(recorder: TTDRecorder) -> Self {
        let position = recorder.latest_snapshot().map(|s| s.step_index);
        Self {
            backend: Backend::Internal(recorder),
            current_position: position,
            replay_mode: false,
            current_snapshot: None,
        }
    }

    pub fn start_recording(&mut self) {
        match &mut self.backend {
            Backend::Internal(rec) => {
                rec.start_recording();
                self.current_position = None;
                self.replay_mode = false;
            }
            #[cfg(target_os = "linux")]
            Backend::RR(_) => {}
        }
    }

    pub fn stop_recording(&mut self) {
        match &mut self.backend {
            Backend::Internal(rec) => {
                rec.stop_recording();
                self.current_position = rec.latest_snapshot().map(|s| s.step_index);
            }
            #[cfg(target_os = "linux")]
            Backend::RR(_) => {}
        }
    }

    pub fn is_recording(&self) -> bool {
        match &self.backend {
            Backend::Internal(rec) => rec.is_recording(),
            #[cfg(target_os = "linux")]
            Backend::RR(_) => false,
        }
    }

    pub fn stats(&self) -> SnapshotStats {
        match &self.backend {
            Backend::Internal(rec) => rec.stats(),
            #[cfg(target_os = "linux")]
            Backend::RR(_) => SnapshotStats::default(),
        }
    }

    pub fn duration(&self) -> Option<std::time::Duration> {
        match &self.backend {
            Backend::Internal(rec) => rec.duration(),
            #[cfg(target_os = "linux")]
            Backend::RR(_) => None,
        }
    }

    pub fn latest_snapshot(&self) -> Option<&ExecutionSnapshot> {
        match &self.backend {
            Backend::Internal(rec) => rec.latest_snapshot(),
            #[cfg(target_os = "linux")]
            Backend::RR(_) => self.current_snapshot.as_deref(),
        }
    }

    pub fn enter_replay_mode(&mut self) {
        match &self.backend {
            Backend::Internal(rec) if rec.snapshot_count() > 0 => {
                self.replay_mode = true;
                if self.current_position.is_none() {
                    self.current_position = rec.latest_snapshot().map(|s| s.step_index);
                }
            }
            #[cfg(target_os = "linux")]
            Backend::RR(_) => {
                self.replay_mode = true;
            }
            _ => {}
        }
    }

    pub fn exit_replay_mode(&mut self) {
        self.replay_mode = false;
    }

    pub fn is_replay_mode(&self) -> bool {
        self.replay_mode
    }

    pub fn record_step_internal(
        &mut self,
        registers: crate::debug::types::RegisterState,
        thread_id: u32,
    ) {
        match &mut self.backend {
            Backend::Internal(rec) => {
                rec.record_step(registers, thread_id);
            }
            #[cfg(target_os = "linux")]
            Backend::RR(_) => {}
        }
    }

    pub fn seek_to(&mut self, step_index: u64) -> SeekResult {
        match &mut self.backend {
            Backend::Internal(rec) => {
                if rec.snapshot_count() == 0 {
                    return SeekResult::Empty;
                }

                let snapshots = rec.snapshots();
                let min_step = snapshots.first().map(|s| s.step_index).unwrap_or(0);
                let max_step = snapshots.last().map(|s| s.step_index).unwrap_or(0);

                if step_index < min_step || step_index > max_step {
                    return SeekResult::OutOfBounds {
                        min: min_step,
                        max: max_step,
                        requested: step_index,
                    };
                }

                if let Some(snapshot) = rec.get_snapshot(step_index) {
                    self.current_position = Some(step_index);
                    self.replay_mode = true;
                    // We don't cache internal snapshots, they are memoized in recorder
                    SeekResult::Success(snapshot.clone())
                } else {
                    SeekResult::OutOfBounds {
                        min: min_step,
                        max: max_step,
                        requested: step_index,
                    }
                }
            }
            #[cfg(target_os = "linux")]
            Backend::RR(rr) => match rr.seek_to(step_index) {
                Ok(snap) => {
                    self.current_position = Some(step_index);
                    self.current_snapshot = Some(Box::new(snap.clone()));
                    self.replay_mode = true;
                    SeekResult::Success(snap)
                }
                Err(_) => SeekResult::Empty,
            },
        }
    }

    pub fn rewind(&mut self, steps: u64) -> SeekResult {
        #[cfg(target_os = "linux")]
        if let Backend::RR(rr) = &mut self.backend {
            if steps == 1 {
                return match rr.reverse_step() {
                    Ok(snap) => {
                        self.current_position = Some(snap.step_index);
                        self.current_snapshot = Some(Box::new(snap.clone()));
                        SeekResult::Success(snap)
                    }
                    Err(_) => SeekResult::Empty,
                };
            }
        }

        if let Some(pos) = self.current_position {
            let target = pos.saturating_sub(steps);
            self.seek_to(target)
        } else {
            SeekResult::Empty
        }
    }

    pub fn forward(&mut self, steps: u64) -> SeekResult {
        #[cfg(target_os = "linux")]
        if let Backend::RR(rr) = &mut self.backend {
            if steps == 1 {
                return match rr.forward_step() {
                    Ok(snap) => {
                        self.current_position = Some(snap.step_index);
                        self.current_snapshot = Some(Box::new(snap.clone()));
                        SeekResult::Success(snap)
                    }
                    Err(_) => SeekResult::Empty,
                };
            }
        }

        if let Some(pos) = self.current_position {
            let target = pos.saturating_add(steps);
            self.seek_to(target)
        } else {
            SeekResult::Empty
        }
    }

    pub fn seek_start(&mut self) -> SeekResult {
        let target = match &self.backend {
            Backend::Internal(rec) => rec.snapshots().first().map(|s| s.step_index),
            #[cfg(target_os = "linux")]
            Backend::RR(_) => Some(0),
        };

        if let Some(t) = target {
            self.seek_to(t)
        } else {
            SeekResult::Empty
        }
    }

    pub fn seek_end(&mut self) -> SeekResult {
        let target = match &self.backend {
            Backend::Internal(rec) => rec.snapshots().last().map(|s| s.step_index),
            #[cfg(target_os = "linux")]
            Backend::RR(rr) => Some(rr.step_count() as u64),
        };

        if let Some(t) = target {
            self.seek_to(t)
        } else {
            SeekResult::Empty
        }
    }

    pub fn current_position(&self) -> Option<u64> {
        self.current_position
    }

    pub fn current_snapshot(&self) -> Option<&ExecutionSnapshot> {
        match &self.backend {
            Backend::Internal(rec) => self.current_position.and_then(|pos| rec.get_snapshot(pos)),
            #[cfg(target_os = "linux")]
            Backend::RR(_) => self.current_snapshot.as_deref(),
        }
    }

    pub fn step_range(&self) -> Option<(u64, u64)> {
        match &self.backend {
            Backend::Internal(rec) => {
                let snapshots = rec.snapshots();
                if snapshots.is_empty() {
                    return None;
                }
                let min = snapshots.first().map(|s| s.step_index).unwrap_or(0);
                let max = snapshots.last().map(|s| s.step_index).unwrap_or(0);
                Some((min, max))
            }
            #[cfg(target_os = "linux")]
            Backend::RR(rr) => rr.timeline_range(),
        }
    }

    pub fn snapshot_count(&self) -> usize {
        match &self.backend {
            Backend::Internal(rec) => rec.snapshot_count(),
            #[cfg(target_os = "linux")]
            Backend::RR(rr) => rr.step_count(),
        }
    }

    pub fn clear(&mut self) {
        match &mut self.backend {
            Backend::Internal(rec) => rec.clear(),
            #[cfg(target_os = "linux")]
            Backend::RR(_) => {}
        }
        self.current_position = None;
        self.replay_mode = false;
        self.current_snapshot = None;
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
            timeline.record_step_internal(regs, 1);
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
