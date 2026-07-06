use crate::pcode::state::MachineState;
use anyhow::Result;
use fission_loader::loader::LoadedBinary;

/// Maps PE sections into the emulator's machine state (Space 3 / RAM).
///
/// IAT patching is now handled by `WindowsEnv::patch_imports` which is called
/// by the Emulator constructor. This function only performs section mapping
/// for legacy compatibility with the PEB/TEB initialization path.
pub fn load_pe(state: &mut MachineState, binary: &LoadedBinary) -> Result<()> {
    tracing::info!("Mapping PE sections into RAM...");
    for sec in &binary.inner().sections {
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
