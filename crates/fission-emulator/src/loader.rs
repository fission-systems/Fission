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
    
    Ok(())
}
