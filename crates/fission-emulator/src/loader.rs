use crate::pcode::state::MachineState;
use fission_loader::loader::LoadedBinary;
use anyhow::{Result, Context};

/// Maps all binary sections into the emulator's RAM (Space 3).
///
/// Import patching (IAT/PLT/MMIO) is *not* done here; it is delegated to the
/// `OsEnvironment` implementation so that different OS layers can handle PE,
/// ELF, and bare-metal formats independently.
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
                    "Failed to map section {} at 0x{:X} (file_size={}). Zeroing.",
                    section.name, section.virtual_address, file_size
                );
            }
        }

        state
            .write_space(3, section.virtual_address, &data)
            .with_context(|| format!(
                "Failed to write section {} at 0x{:X} to RAM",
                section.name, section.virtual_address
            ))?;

        tracing::debug!(
            "Mapped section {} → 0x{:X}..0x{:X} ({} bytes)",
            section.name,
            section.virtual_address,
            section.virtual_address + section.virtual_size as u64,
            section.virtual_size
        );
    }

    Ok(())
}
