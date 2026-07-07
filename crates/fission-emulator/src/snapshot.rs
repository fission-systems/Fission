use crate::core::Emulator;
use crate::pcode::state::MachineState;
use std::time::SystemTime;
use std::path::Path;
use anyhow::{Context, Result};
use serde::{Serialize, Deserialize};

/// A point-in-time snapshot of the emulator state.
#[derive(Serialize, Deserialize)]
pub struct EmulatorSnapshot {
    pub state: MachineState,
    pub pc: u64,
    pub trigger_addr: u64,
    pub created_at: SystemTime,
}

impl EmulatorSnapshot {
    /// Captures the current state of the emulator.
    pub fn capture(emu: &Emulator, trigger_addr: u64) -> Self {
        Self {
            state: emu.state.clone(),
            pc: emu.pc,
            trigger_addr,
            created_at: SystemTime::now(),
        }
    }

    /// Restores this snapshot into the given emulator instance.
    pub fn restore_into(self, emu: &mut Emulator) {
        // hooks are not serialized, so we must preserve them from the current state
        let hooks = std::mem::take(&mut emu.state.hooks);
        emu.state = self.state;
        emu.state.hooks = hooks;
        emu.pc = self.pc;
        tracing::info!("Restored snapshot taken at PC=0x{:X}", self.trigger_addr);
    }

    /// Saves the snapshot to a file on disk.
    pub fn save_to_disk(&self, path: impl AsRef<Path>) -> Result<()> {
        let file = std::fs::File::create(path).context("Failed to create snapshot file")?;
        let writer = std::io::BufWriter::new(file);
        bincode::serialize_into(writer, self).context("Failed to serialize snapshot")?;
        Ok(())
    }

    /// Loads a snapshot from a file on disk.
    pub fn load_from_disk(path: impl AsRef<Path>) -> Result<Self> {
        let file = std::fs::File::open(path).context("Failed to open snapshot file")?;
        let reader = std::io::BufReader::new(file);
        let snapshot = bincode::deserialize_from(reader).context("Failed to deserialize snapshot")?;
        Ok(snapshot)
    }
}
