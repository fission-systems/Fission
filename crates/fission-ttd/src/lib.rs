//! Time-travel debugging primitives shared across dynamic backends.

// CI runs `cargo clippy ... -D warnings`; keep rustc warnings strict via other crates / rustc defaults.
#![allow(clippy::all)]

mod recorder;
mod snapshot;
mod types;

pub use recorder::{RecordingStatus, TTDRecorder};
pub use snapshot::{ExecutionSnapshot, MemoryDelta, SnapshotStats};
pub use types::RegisterState;

/// Object-safe timeline interface for consumers that should not depend on a
/// concrete backend implementation.
pub trait TimelineDriver: Send {
    fn start_recording(&mut self);
    fn stop_recording(&mut self);
    fn enter_replay_mode(&mut self);
    fn is_recording(&self) -> bool;
    fn stats(&self) -> SnapshotStats;
    fn step_range(&self) -> Option<(u64, u64)>;
    fn current_position(&self) -> Option<u64>;
    fn current_snapshot_owned(&self) -> Option<ExecutionSnapshot>;
    fn record_step(&mut self, registers: RegisterState, thread_id: u32);
    fn seek_to(&mut self, step_index: u64);
    fn rewind(&mut self, steps: u64);
    fn forward(&mut self, steps: u64);
}
