//! AArch64 architectural state and operations modeled by the emulator.

use crate::core::Emulator;

/// `DCZID_EL0.BS = 4` describes a 64-byte `DC ZVA` block; `DZP = 0` permits
/// the instruction at EL0. This matches the default AArch64 profile used by
/// the reference emulator.
const DEFAULT_DCZID_EL0: u64 = 4;

pub(crate) fn initialize_cpu_state(emu: &mut Emulator) -> anyhow::Result<()> {
    emu.write_register_u64("dczid_el0", DEFAULT_DCZID_EL0)
}

/// Zero the cache-line-sized block selected by `DCZID_EL0.BS`.
pub(crate) fn data_cache_zero(emu: &mut Emulator, address: u64) {
    let dczid = emu
        .read_register_u64("dczid_el0")
        .unwrap_or(DEFAULT_DCZID_EL0);
    let block_size = 4u64 << (dczid & 0xF);
    let base = address & !(block_size - 1);
    let size = block_size as usize;
    let ram = emu.state.ram_space();
    let executable_pages = emu.state.page_map.exec_pages_in_range(base, size);
    let zeros = vec![0; size];

    if let Err(error) = emu.state.write_space(ram, base, &zeros) {
        emu.record_memory_fault("DC_ZVA", ram, base, size, error);
        return;
    }

    for page in executable_pages {
        emu.jit_cache.invalidate_page(page);
    }
    if emu.observe.mem {
        emu.notify_mem(base, size as u32, true, 0);
    }
}
