use crate::pcode::state::MachineState;
use anyhow::Result;

/// Initializes the Thread Environment Block (TEB) and Process Environment Block (PEB)
/// at conventional linear addresses for a 32-bit or 64-bit Windows process.
pub fn initialize_peb_teb(state: &mut MachineState, is_64bit: bool) -> Result<()> {
    if is_64bit {
        // x64 TEB is typically at GS:[0x30], PEB at GS:[0x60]
        let teb_addr = 0x000000007FFDE000u64;
        let peb_addr = 0x000000007FFDF000u64;

        // TEB
        state.write_space(state.ram_space(), teb_addr + 0x60, &peb_addr.to_le_bytes())?;
        
        // PEB
        state.write_space(state.ram_space(), peb_addr + 0x2, &[1])?; // BeingDebugged = 1 (just to test anti-debug)
        
        tracing::info!("Initialized x64 TEB at 0x{:X}, PEB at 0x{:X}", teb_addr, peb_addr);
    } else {
        // x86 TEB is typically at FS:[0x18], PEB at FS:[0x30]
        let teb_addr = 0x7FFDE000u64;
        let peb_addr = 0x7FFDF000u64;

        // TEB
        state.write_space(state.ram_space(), teb_addr + 0x30, &(peb_addr as u32).to_le_bytes())?;
        
        // PEB
        state.write_space(state.ram_space(), peb_addr + 0x2, &[1])?; // BeingDebugged = 1
        
        tracing::info!("Initialized x86 TEB at 0x{:X}, PEB at 0x{:X}", teb_addr, peb_addr);
    }
    
    Ok(())
}
