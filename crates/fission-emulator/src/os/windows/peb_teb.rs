use crate::pcode::page_map::prot;
use crate::pcode::state::MachineState;
use anyhow::Result;

pub const TEB_ADDR_64: u64 = 0x0000_0000_7FFD_E000;
pub const PEB_ADDR_64: u64 = 0x0000_0000_7FFD_F000;
pub const TEB_ADDR_32: u64 = 0x7FFD_E000;
pub const PEB_ADDR_32: u64 = 0x7FFD_F000;

/// Initializes the Thread Environment Block (TEB) and Process Environment Block (PEB)
/// at conventional linear addresses for a 32-bit or 64-bit Windows process.
/// Maps pages and populates TIB fields (ExceptionList, StackBase, StackLimit, Self pointer)
/// and PEB fields (BeingDebugged, ImageBaseAddress).
pub fn setup_peb_teb(
    state: &mut MachineState,
    is_64bit: bool,
    stack_top: u64,
    stack_limit: u64,
    image_base: u64,
    being_debugged: u8,
) -> Result<(u64, u64)> {
    let (teb_addr, peb_addr) = if is_64bit {
        (TEB_ADDR_64, PEB_ADDR_64)
    } else {
        (TEB_ADDR_32, PEB_ADDR_32)
    };

    // PEB/TEB pages
    state
        .page_map
        .map_region(teb_addr & !0xFFF, 0x2000, prot::RW | prot::ANON, true);

    // The Thread Information Block -- the first fields of the TEB, which the
    // CRT reads directly rather than through an API. MinGW/MSVC start-up walks
    // `fs:[0x18]` to the TIB and then reads `StackBase` out of it.
    // Field offsets differ by bitness: ExceptionList, StackBase, StackLimit, Self.
    {
        let ram = state.ram_space();
        let (exception_list, stack_base, stack_limit_off, self_off) = if is_64bit {
            (0x00u64, 0x08u64, 0x10u64, 0x30u64)
        } else {
            (0x00, 0x04, 0x08, 0x18)
        };
        let word = |v: u64| -> Vec<u8> {
            let b = v.to_le_bytes();
            b[..if is_64bit { 8 } else { 4 }].to_vec()
        };
        // No SEH frame yet: list terminator is all-ones.
        state.write_space(ram, teb_addr + exception_list, &word(u64::MAX))?;
        state.write_space(ram, teb_addr + stack_base, &word(stack_top))?;
        state.write_space(ram, teb_addr + stack_limit_off, &word(stack_limit))?;
        state.write_space(ram, teb_addr + self_off, &word(teb_addr))?;
    }

    // Write PEB fields
    let ram = state.ram_space();
    if is_64bit {
        state.write_space(ram, teb_addr + 0x60, &peb_addr.to_le_bytes())?;
        state.write_space(ram, peb_addr + 0x2, &[being_debugged])?;
        state.write_space(ram, peb_addr + 0x10, &image_base.to_le_bytes())?;
        tracing::info!(
            "Initialized x64 TEB at 0x{:X}, PEB at 0x{:X}",
            teb_addr,
            peb_addr
        );
    } else {
        state.write_space(ram, teb_addr + 0x30, &(peb_addr as u32).to_le_bytes())?;
        state.write_space(ram, peb_addr + 0x2, &[being_debugged])?;
        state.write_space(ram, peb_addr + 0x8, &(image_base as u32).to_le_bytes())?;
        tracing::info!(
            "Initialized x86 TEB at 0x{:X}, PEB at 0x{:X}",
            teb_addr,
            peb_addr
        );
    }

    Ok((teb_addr, peb_addr))
}

/// Standalone / compatibility entry point for initializing PEB and TEB.
/// BeingDebugged defaults to 0 (clean sandbox).
pub fn initialize_peb_teb(state: &mut MachineState, is_64bit: bool) -> Result<()> {
    let (stack_top, stack_limit) = if is_64bit {
        (0x0000_7FFF_FFFF_F000u64, 0x0000_7FFF_FF7F_F000u64)
    } else {
        (0xFFFF_E000u64, 0xFF7F_E000u64)
    };
    setup_peb_teb(state, is_64bit, stack_top, stack_limit, 0, 0)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_peb_teb_64bit() {
        let mut state = MachineState::new();
        initialize_peb_teb(&mut state, true).unwrap();

        let ram = state.ram_space();
        // Self pointer at GS:[0x30] (TEB + 0x30)
        let self_bytes = state.read_space(ram, TEB_ADDR_64 + 0x30, 8).unwrap();
        assert_eq!(
            u64::from_le_bytes(self_bytes.try_into().unwrap()),
            TEB_ADDR_64
        );

        // PEB pointer at GS:[0x60] (TEB + 0x60)
        let peb_bytes = state.read_space(ram, TEB_ADDR_64 + 0x60, 8).unwrap();
        assert_eq!(
            u64::from_le_bytes(peb_bytes.try_into().unwrap()),
            PEB_ADDR_64
        );

        // BeingDebugged at PEB + 0x2 must be 0
        let dbg_bytes = state.read_space(ram, PEB_ADDR_64 + 0x2, 1).unwrap();
        assert_eq!(dbg_bytes[0], 0);
    }

    #[test]
    fn test_initialize_peb_teb_32bit() {
        let mut state = MachineState::new();
        initialize_peb_teb(&mut state, false).unwrap();

        let ram = state.ram_space();
        // Self pointer at FS:[0x18] (TEB + 0x18)
        let self_bytes = state.read_space(ram, TEB_ADDR_32 + 0x18, 4).unwrap();
        assert_eq!(
            u32::from_le_bytes(self_bytes.try_into().unwrap()),
            TEB_ADDR_32 as u32
        );

        // PEB pointer at FS:[0x30] (TEB + 0x30)
        let peb_bytes = state.read_space(ram, TEB_ADDR_32 + 0x30, 4).unwrap();
        assert_eq!(
            u32::from_le_bytes(peb_bytes.try_into().unwrap()),
            PEB_ADDR_32 as u32
        );

        // BeingDebugged at PEB + 0x2 must be 0
        let dbg_bytes = state.read_space(ram, PEB_ADDR_32 + 0x2, 1).unwrap();
        assert_eq!(dbg_bytes[0], 0);
    }
}
