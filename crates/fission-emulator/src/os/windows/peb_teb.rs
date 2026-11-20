use crate::pcode::page_map::prot;
use crate::pcode::state::MachineState;
use anyhow::Result;

/// A 64-bit TEB is 0x1838 bytes, so it takes two pages: its TLS slots start at
/// `+0x1480`. At `0x7FFDE000` they ran into the PEB's page.
pub const TEB_ADDR_64: u64 = 0x0000_0000_7FFD_D000;
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

    // TEB pages up to and including the PEB's.
    let teb_page = teb_addr & !0xFFF;
    state.page_map.map_region(
        teb_page,
        (peb_addr & !0xFFF) + 0x1000 - teb_page,
        prot::RW | prot::ANON,
        true,
    );

    // The Thread Information Block -- the first fields of the TEB, which the
    // CRT reads directly rather than through an API. MinGW/MSVC start-up walks
    // `fs:[0x18]` to the TIB and then reads `StackBase` out of it.
    // Field offsets differ by bitness: ExceptionList, StackBase, StackLimit, Self.
    {
        let ram = state.ram_space();
        let (exception_list, stack_base, stack_limit_off, self_off, client_id) = if is_64bit {
            (0x00u64, 0x08u64, 0x10u64, 0x30u64, 0x40u64)
        } else {
            (0x00, 0x04, 0x08, 0x18, 0x20)
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
        // ClientId: the process and thread ids `GetCurrentProcessId` and
        // `GetCurrentThreadId` answer with.
        let pointer = if is_64bit { 8 } else { 4 };
        state.write_space(ram, teb_addr + client_id, &word(1000))?;
        state.write_space(
            ram,
            teb_addr + client_id + pointer,
            &word(crate::os::windows::threads::ROOT_TID),
        )?;
    }

    setup_shared_user_data(state)?;

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

/// `KUSER_SHARED_DATA`: one read-only page the kernel maps at the same address
/// into every process, 32- and 64-bit alike. Programs read the clock from it
/// directly, with no API call -- the Go runtime's `nanotime` reads
/// `InterruptTime` at `0x7FFE0008` before `main`, and an unmapped page stops it
/// there.
pub const SHARED_USER_DATA: u64 = 0x7FFE_0000;

/// Field offsets, from the layout every Windows version since NT 4 keeps.
const SUD_TICK_COUNT_MULTIPLIER: u64 = 0x004;
const SUD_INTERRUPT_TIME: u64 = 0x008;
const SUD_SYSTEM_TIME: u64 = 0x014;
const SUD_NT_BUILD_NUMBER: u64 = 0x260;
const SUD_NT_PRODUCT_TYPE: u64 = 0x264;
const SUD_PRODUCT_TYPE_IS_VALID: u64 = 0x268;
const SUD_NT_MAJOR_VERSION: u64 = 0x26C;
const SUD_NT_MINOR_VERSION: u64 = 0x270;
const SUD_TICK_COUNT: u64 = 0x320;

/// `TickCountQuad * TickCountMultiplier >> 24` is milliseconds; this value is
/// the one Windows uses, 15.625 ms per tick.
const TICK_COUNT_MULTIPLIER: u32 = 0x0FA0_0000;

/// Map the page and fill the fields the process may read: the version it was
/// told it runs on and the clock at `tick_ms` milliseconds since start.
pub fn setup_shared_user_data(state: &mut MachineState) -> Result<()> {
    state.page_map.map_region(
        SHARED_USER_DATA,
        0x1000,
        prot::VALID | prot::READ | prot::ANON,
        true,
    );
    let ram = state.ram_space();
    let page = SHARED_USER_DATA;
    with_kernel_write(state, |state| {
        state.write_space(
            ram,
            page + SUD_TICK_COUNT_MULTIPLIER,
            &TICK_COUNT_MULTIPLIER.to_le_bytes(),
        )?;
        // Windows 10 22H2 workstation, which is what the rest of the HLE
        // answers as.
        state.write_space(ram, page + SUD_NT_BUILD_NUMBER, &19045u32.to_le_bytes())?;
        state.write_space(ram, page + SUD_NT_PRODUCT_TYPE, &1u32.to_le_bytes())?;
        state.write_space(ram, page + SUD_PRODUCT_TYPE_IS_VALID, &[1])?;
        state.write_space(ram, page + SUD_NT_MAJOR_VERSION, &10u32.to_le_bytes())?;
        state.write_space(ram, page + SUD_NT_MINOR_VERSION, &0u32.to_le_bytes())?;
        Ok(())
    })?;
    write_shared_user_time(state, 0)
}

/// Bring the page's clock to `tick_ms` after process start -- the clock
/// `GetTickCount` and `GetSystemTimeAsFileTime` answer from, so a program
/// comparing them sees one time. `InterruptTime` and `TickCount` count from
/// boot, as those APIs do; `SystemTime` is the wall clock.
pub fn write_shared_user_time(state: &mut MachineState, tick_ms: u64) -> Result<()> {
    if !state.page_map.is_mapped(SHARED_USER_DATA) {
        return Ok(());
    }
    // `KSYSTEM_TIME` is `LowPart, High1Time, High2Time`; a reader retries
    // until the two high words agree, so both carry the high half.
    let ksystem_time = |value: u64| -> [u8; 12] {
        let mut bytes = [0u8; 12];
        bytes[0..4].copy_from_slice(&(value as u32).to_le_bytes());
        bytes[4..8].copy_from_slice(&((value >> 32) as u32).to_le_bytes());
        bytes[8..12].copy_from_slice(&((value >> 32) as u32).to_le_bytes());
        bytes
    };
    let uptime_ms = crate::os::windows::hle::UPTIME_AT_START_MS.wrapping_add(tick_ms);
    let interrupt_time = uptime_ms.wrapping_mul(10_000);
    let system_time =
        crate::os::windows::hle::FIXED_FILETIME_BASE.wrapping_add(tick_ms.wrapping_mul(10_000));
    let tick_count = uptime_ms.wrapping_mul(1 << 24) / u64::from(TICK_COUNT_MULTIPLIER);
    let ram = state.ram_space();
    let page = SHARED_USER_DATA;
    with_kernel_write(state, |state| {
        state.write_space(
            ram,
            page + SUD_INTERRUPT_TIME,
            &ksystem_time(interrupt_time),
        )?;
        state.write_space(ram, page + SUD_SYSTEM_TIME, &ksystem_time(system_time))?;
        state.write_space(ram, page + SUD_TICK_COUNT, &ksystem_time(tick_count))?;
        // The legacy 32-bit tick count at offset 0.
        state.write_space(ram, page, &(tick_count as u32).to_le_bytes())?;
        Ok(())
    })
}

/// The page is read-only to the guest, not to the kernel that keeps it.
fn with_kernel_write(
    state: &mut MachineState,
    write: impl FnOnce(&mut MachineState) -> Result<()>,
) -> Result<()> {
    let enforce = std::mem::replace(&mut state.enforce_page_faults, false);
    let result = write(state);
    state.enforce_page_faults = enforce;
    result
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
