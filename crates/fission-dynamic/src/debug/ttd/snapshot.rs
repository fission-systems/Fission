//! Execution Snapshot - Captures complete state at a point in time.

use super::super::types::RegisterState;
use std::time::Instant;

/// Memory change record for delta compression
#[derive(Debug, Clone)]
pub struct MemoryDelta {
    /// Start address of the change
    pub address: u64,
    /// Original value before the change
    pub old_value: Vec<u8>,
    /// New value after the change
    pub new_value: Vec<u8>,
}

impl MemoryDelta {
    /// Create a new memory delta record
    pub fn new(address: u64, old_value: Vec<u8>, new_value: Vec<u8>) -> Self {
        Self {
            address,
            old_value,
            new_value,
        }
    }

    /// Size of the changed region
    pub fn size(&self) -> usize {
        self.new_value.len()
    }
}

/// Complete execution snapshot at a specific point in time
#[derive(Debug, Clone)]
pub struct ExecutionSnapshot {
    /// Step index in the recording (0-based)
    pub step_index: u64,
    /// Timestamp when this snapshot was taken
    pub timestamp: Instant,
    ///CPU register state at this point
    pub registers: RegisterState,
    /// Memory changes that occurred at this step
    pub memory_deltas: Vec<MemoryDelta>,
    /// Thread ID that was executing
    pub thread_id: u32,
}

impl ExecutionSnapshot {
    /// Create a new snapshot
    pub fn new(step_index: u64, registers: RegisterState, thread_id: u32) -> Self {
        Self {
            step_index,
            timestamp: Instant::now(),
            registers,
            memory_deltas: Vec::new(),
            thread_id,
        }
    }

    /// Add a memory change to this snapshot
    pub fn add_memory_delta(&mut self, delta: MemoryDelta) {
        self.memory_deltas.push(delta);
    }

    /// Get the instruction pointer (RIP) at this snapshot
    pub fn rip(&self) -> u64 {
        self.registers.rip
    }

    /// Estimate memory usage of this snapshot in bytes
    pub fn memory_usage(&self) -> usize {
        std::mem::size_of::<Self>()
            + self
                .memory_deltas
                .iter()
                .map(|d| d.old_value.len() + d.new_value.len())
                .sum::<usize>()
    }
}

/// Snapshot statistics for UI display
#[derive(Debug, Clone, Default)]
pub struct SnapshotStats {
    /// Total number of snapshots
    pub count: u64,
    /// Total memory usage in bytes
    pub memory_bytes: usize,
    /// Average memory deltas per snapshot
    pub avg_deltas_per_snapshot: f64,
}
