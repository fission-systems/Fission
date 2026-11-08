//! Execution snapshot primitives.

use crate::RegisterState;
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

/// Shadow state change record for Taint tracking
#[derive(Debug, Clone)]
pub struct ShadowDelta {
    pub space_id: u64,
    pub address: u64,
    pub old_node: Option<u32>,
    pub new_node: Option<u32>,
}

/// Complete execution snapshot at a specific point in time
#[derive(Debug, Clone)]
pub struct ExecutionSnapshot {
    /// Step index in the recording (0-based)
    pub step_index: u64,
    /// Timestamp when this snapshot was taken
    pub timestamp: Instant,
    /// CPU register state at this point
    pub registers: RegisterState,
    /// Memory changes that occurred at this step
    pub memory_deltas: Vec<MemoryDelta>,
    /// Shadow (Taint) changes that occurred at this step
    pub shadow_deltas: Vec<ShadowDelta>,
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
            shadow_deltas: Vec::new(),
            thread_id,
        }
    }

    /// Add a memory change to this snapshot
    pub fn add_memory_delta(&mut self, delta: MemoryDelta) {
        self.memory_deltas.push(delta);
    }

    /// Add a shadow (taint) change to this snapshot
    pub fn add_shadow_delta(&mut self, delta: ShadowDelta) {
        self.shadow_deltas.push(delta);
    }

    /// Where the machine was executing at this snapshot.
    pub fn pc(&self) -> u64 {
        self.registers.pc
    }

    /// Estimate memory usage of this snapshot in bytes
    pub fn memory_usage(&self) -> usize {
        std::mem::size_of::<Self>()
            // The registers' own heap, the bulk of a snapshot.
            + self.registers.heap_bytes()
            // The delta records' buffers, and the byte vectors inside them.
            + self.memory_deltas.capacity() * std::mem::size_of::<MemoryDelta>()
            + self
                .memory_deltas
                .iter()
                .map(|d| d.old_value.len() + d.new_value.len())
                .sum::<usize>()
            + self.shadow_deltas.capacity() * std::mem::size_of::<ShadowDelta>()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RegisterState;

    fn snapshot(registers: RegisterState) -> ExecutionSnapshot {
        ExecutionSnapshot::new(1, registers, 0)
    }

    #[test]
    fn a_snapshot_with_more_registers_reports_more_memory() {
        // `memory_usage` counted the struct and the memory-delta bytes, so the
        // registers -- the bulk of a snapshot taken every few thousand
        // instructions -- cost nothing no matter how many there were.
        let few = snapshot(RegisterState::at(0x1000).with("RAX", 1));
        let mut many = RegisterState::at(0x1000);
        for i in 0..16 {
            many.set(&format!("R{i}"), i);
        }
        many.set_bytes("XMM0", &[0xAA; 32]);
        let many = snapshot(many);

        assert!(many.memory_usage() > few.memory_usage());
        // And by at least what the wide register's bytes alone occupy.
        assert!(many.memory_usage() >= few.memory_usage() + 32);
    }

    #[test]
    fn delta_buffers_count_beyond_their_value_bytes() {
        // The deltas' own records (the Vec's buffer) are heap too, not only the
        // old/new byte vectors inside them.
        let mut with_deltas = snapshot(RegisterState::at(0x1000));
        for i in 0..8 {
            with_deltas.add_memory_delta(MemoryDelta::new(i, vec![0], vec![1]));
            with_deltas.add_shadow_delta(ShadowDelta {
                space_id: 0,
                address: i,
                old_node: None,
                new_node: Some(1),
            });
        }
        let empty = snapshot(RegisterState::at(0x1000));
        let value_bytes = 8 * 2;
        assert!(
            with_deltas.memory_usage()
                >= empty.memory_usage()
                    + value_bytes
                    + 8 * std::mem::size_of::<MemoryDelta>()
                    + 8 * std::mem::size_of::<ShadowDelta>()
        );
    }

    #[test]
    fn an_empty_snapshot_costs_only_its_own_size() {
        // Positive control: nothing is invented for a snapshot with no heap.
        let empty = snapshot(RegisterState::at(0x1000));
        assert_eq!(
            empty.memory_usage(),
            std::mem::size_of::<ExecutionSnapshot>()
        );
    }
}
