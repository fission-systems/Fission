use anyhow::Result;
use crate::core::Emulator;
use crate::pcode::state::MachineState;
use crate::os::env::{HleResult, OsEnvironment};
use fission_loader::loader::LoadedBinary;

/// Bare-metal / firmware execution environment.
///
/// No OS is assumed. Import patching uses MMIO magic addresses.
/// Handlers can be registered for specific address ranges to model
/// memory-mapped peripherals.
pub struct BareMetalEnv {
    /// MMIO regions: (start, end) → handler name for logging
    pub mmio_regions: Vec<(u64, u64, String)>,
}

impl BareMetalEnv {
    pub fn new() -> Self {
        Self { mmio_regions: Vec::new() }
    }

    /// Register an MMIO region for tracing. Reads/writes in this range
    /// will be logged.
    pub fn add_mmio_region(&mut self, start: u64, end: u64, name: impl Into<String>) {
        self.mmio_regions.push((start, end, name.into()));
    }
}

impl Default for BareMetalEnv {
    fn default() -> Self { Self::new() }
}

impl OsEnvironment for BareMetalEnv {
    fn patch_imports(&self, _state: &mut MachineState, _binary: &LoadedBinary) -> Result<()> {
        // Bare-metal firmware typically has no dynamic imports; no-op.
        Ok(())
    }

    fn resolve_stub(&self, _binary: &LoadedBinary, magic_addr: u64) -> Option<String> {
        for (start, end, name) in &self.mmio_regions {
            if magic_addr >= *start && magic_addr < *end {
                return Some(format!("MMIO:{}+0x{:X}", name, magic_addr - start));
            }
        }
        None
    }

    fn dispatch_hle(&self, emu: &mut Emulator, func_name: &str) -> Result<HleResult> {
        tracing::info!("MMIO access: {}", func_name);
        // By default, return 0 for all MMIO reads.
        emu.write_return_val(0)?;
        Ok(HleResult::Continue)
    }
}
