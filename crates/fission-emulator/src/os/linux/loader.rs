use crate::pcode::state::MachineState;
use anyhow::Result;
use fission_loader::loader::LoadedBinary;

/// Maps ELF PT_LOAD segments into the emulator's machine state (Space 3 / RAM).
///
/// For static ELFs, each program header with `p_type = PT_LOAD` defines a region
/// that must be mapped at its virtual address before execution begins.
/// If the binary lacks program headers (only sections), fall back to section mapping.
pub fn load_elf(state: &mut MachineState, binary: &LoadedBinary) -> Result<()> {
    tracing::info!("Mapping ELF sections into RAM...");

    for sec in &binary.inner().sections {
        if sec.virtual_address == 0 || sec.virtual_size == 0 {
            tracing::debug!(
                "Skipping non-loaded section {} (va=0x{:X})",
                sec.name, sec.virtual_address
            );
            continue;
        }
        tracing::debug!(
            "Mapping section {} at 0x{:X} (size: 0x{:X})",
            sec.name, sec.virtual_address, sec.virtual_size
        );
        let sec_data = binary
            .view_bytes(sec.virtual_address, sec.virtual_size as usize)
            .unwrap_or(&[]);
        state.write_space(3, sec.virtual_address, sec_data)?;
    }

    Ok(())
}
