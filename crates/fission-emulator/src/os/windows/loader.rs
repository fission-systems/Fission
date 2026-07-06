use crate::pcode::state::MachineState;
use anyhow::Result;
use fission_loader::loader::LoadedBinary;

/// A simple PE loader that maps sections into the emulator's machine state
pub fn load_pe(state: &mut MachineState, binary: &LoadedBinary) -> Result<()> {
    tracing::info!("Mapping PE file into memory...");
    
    let inner = binary.inner();
    
    // 1. Map sections into RAM
    for sec in &inner.sections {
        tracing::debug!("Mapping section {} at 0x{:X} (size: 0x{:X})", sec.name, sec.virtual_address, sec.virtual_size);
        let sec_data = binary.view_bytes(sec.virtual_address, sec.virtual_size as usize).unwrap_or(&[]);
        state.write_space(3, sec.virtual_address, sec_data)?;
    }
    
    // 2. Synthesize IAT with magic addresses for HLE
    // Magic base for IAT: 0xFFFFFFF000000000
    // (Actual imports are usually identified via pe-specific logic, but we can intercept at this magic range)
    let magic_base = 0xFFFFFFF000000000u64;
    for (i, (&addr, name)) in inner.iat_symbols.iter().enumerate() {
        let magic_addr = magic_base + (i as u64 * 8);
        tracing::debug!("Mapping import {} at 0x{:X} -> HLE Trampoline 0x{:X}", name, addr, magic_addr);
        state.write_space(3, addr, &magic_addr.to_le_bytes())?;
    }
    
    Ok(())
}
