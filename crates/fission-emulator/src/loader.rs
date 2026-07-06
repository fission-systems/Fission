use crate::pcode::state::MachineState;
use fission_loader::loader::LoadedBinary;
use anyhow::{Result, Context};

/// Maps the sections from a loaded binary into the emulator's memory space (Space 3 / ram).
pub fn map_binary_to_state(state: &mut MachineState, binary: &LoadedBinary) -> Result<()> {
    for section in binary.sections.iter() {
        if section.virtual_size == 0 {
            continue;
        }

        let mut data = vec![0u8; section.virtual_size as usize];
        let file_size = std::cmp::min(section.virtual_size, section.file_size) as usize;

        if file_size > 0 {
            if let Some(file_data) = binary.view_bytes(section.virtual_address, file_size) {
                data[..file_size].copy_from_slice(file_data);
            } else {
                tracing::warn!(
                    "Failed to map section {} at 0x{:X} with file_size {}. Filling with zeroes.",
                    section.name, section.virtual_address, file_size
                );
            }
        }

        // write to Space 3 (ram)
        state.write_space(3, section.virtual_address, &data)
            .with_context(|| format!("Failed to map section {} at 0x{:X}", section.name, section.virtual_address))?;
        
        tracing::debug!("Mapped section {} at 0x{:X} (size: 0x{:X})", section.name, section.virtual_address, section.virtual_size);
    }
    
    // Synthesize IAT with magic addresses for HLE if it's a PE file
    if binary.format == "PE" {
        let magic_base = 0xFFFFFFF000000000u64;
        let mut iat_entries: Vec<_> = binary.inner().iat_symbols.iter().collect();
        iat_entries.sort_by_key(|&(&addr, _)| addr);
        for (i, (&addr, name)) in iat_entries.into_iter().enumerate() {
            let magic_addr = magic_base + (i as u64 * 8);
            tracing::debug!("Mapping import {} at 0x{:X} -> HLE Trampoline 0x{:X}", name, addr, magic_addr);
            state.write_space(3, addr, &magic_addr.to_le_bytes())?;
        }
    }
    
    Ok(())
}
