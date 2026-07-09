use crate::pcode::page_map::prot;
use crate::pcode::state::MachineState;
use anyhow::{Context, Result};
use fission_loader::loader::LoadedBinary;

/// Maps all binary sections into the emulator's RAM (space 3) and records
/// guest page protections for the user-mode page map.
///
/// Import patching (IAT/PLT/MMIO) is *not* done here; it is delegated to the
/// `OsEnvironment` implementation so that different OS layers can handle PE,
/// ELF, and bare-metal formats independently.
pub fn map_binary_to_state(state: &mut MachineState, binary: &LoadedBinary) -> Result<()> {
    let mut max_end = 0u64;

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
                    section.name,
                    section.virtual_address,
                    file_size
                );
            }
        }

        let ram = state.ram_space();
        state
            .write_space(ram, section.virtual_address, &data)
            .with_context(|| {
                format!(
                    "Failed to write section {} at 0x{:X} to RAM",
                    section.name, section.virtual_address
                )
            })?;

        // Page protections from section flags (QEMU-style PAGE_READ/WRITE/EXEC shape).
        let mut page_prot = prot::VALID;
        if section.is_readable {
            page_prot |= prot::READ;
        }
        if section.is_writable {
            page_prot |= prot::WRITE;
        }
        if section.is_executable {
            page_prot |= prot::EXEC;
        }
        // Always allow read on mapped image sections if nothing else set.
        if page_prot & (prot::READ | prot::WRITE | prot::EXEC) == 0 {
            page_prot |= prot::READ;
        }

        state.page_map.map_region(
            section.virtual_address,
            section.virtual_size,
            page_prot,
            false,
        );

        let end = section
            .virtual_address
            .saturating_add(section.virtual_size);
        max_end = max_end.max(end);

        tracing::debug!(
            "Mapped section {} → 0x{:X}..0x{:X} ({} bytes, prot=0x{:02X})",
            section.name,
            section.virtual_address,
            end,
            section.virtual_size,
            page_prot
        );
    }

    // Program break starts after the highest mapped image page (heap grows up).
    if max_end > 0 {
        state.page_map.set_brk_base(max_end);
    }

    // Stack region (high canonical user VA) — RW, large enough for simple programs.
    let sp_base = if binary.inner().is_64bit {
        0x0000_7FFF_FF00_0000u64
    } else {
        0x7F00_0000u64
    };
    let stack_size = 0x10_0000u64; // 1 MiB
    state
        .page_map
        .map_region(sp_base - stack_size, stack_size, prot::RW, true);

    Ok(())
}
