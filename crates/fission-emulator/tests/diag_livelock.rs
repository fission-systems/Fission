//! Diagnostics for static musl CRT / mallocng livelock investigation.
//!
//! ## Findings (2026-07-10)
//!
//! Hot PC `0x10035A3` is **not** TLS or a spinlock: it is musl **mallocng
//! size-class arithmetic** (`tzcnt`/`shl` on the expand path after empty bins).
//!
//! Root mapping bug (fixed in `page_map::map_region`): unaligned `.bss` start
//! (`0x1007D00`, size `0x9E0`) dropped the tail page `0x1008000`, so
//! `brk_cur` / size-class counters / malloc lock lived in unmapped RAM;
//! `jit_write_space` silently ignored faults (`let _ = write_space(...)`).
//!
//! Remaining open: after the map fix, expand still stops at `0x10035A3` with
//! empty bins and a huge page-count field — next dig is mask/init integrity
//! on the alloc_meta path (not missing syscall numbers).

use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_emulator::MachineState;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;
use std::path::PathBuf;

fn make_emu(max_inst: u64) -> Emulator {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
    let binary = LoadedBinary::from_file(&path).unwrap();
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary).unwrap();
    let load_spec = binary.load_spec().unwrap().clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let arch =
        ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary)).unwrap();
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .unwrap()
        .with_max_inst(Some(max_inst));
    emu.apply_linux_image(info).unwrap();
    emu
}

/// Regression: ELF `.bss` tail page must be mapped so mallocng globals work.
#[test]
fn static_elf_bss_tail_page_mapped() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
    let binary = LoadedBinary::from_file(&path).unwrap();
    let mut state = MachineState::new();
    let _info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary).unwrap();
    // .bss = 0x1007D00 size 0x9E0 → ends 0x10086E0; must cover page 0x1008000.
    assert!(
        state.page_map.is_mapped(0x1007F20),
        "init-flag page"
    );
    assert!(
        state.page_map.is_mapped(0x10082B0),
        "brk_cur / mallocng tail page must be mapped"
    );
    assert!(
        state.page_map.is_mapped(0x10080E8),
        "size-class usage counters"
    );
    // Writable check under page-fault enforcement after emu setup.
    let mut emu = make_emu(1);
    let ram = emu.state.ram_space();
    emu.state
        .write_space(ram, 0x10082B0, &0xAAu64.to_le_bytes())
        .expect("write brk_cur");
    let b = emu.state.read_space(ram, 0x10082B0, 8).unwrap();
    assert_eq!(u64::from_le_bytes(b.try_into().unwrap()), 0xAA);
}

#[test]
fn dump_state_at_malloc_hot() {
    let mut emu = make_emu(2000);
    emu.run().unwrap();
    eprintln!(
        "pc=0x{:X} inst={} exit={:?} sys={:?}",
        emu.pc, emu.inst_count, emu.metrics.exit_reason, emu.metrics.syscalls
    );
    // Soft assertion: early CRT still has no unknown syscalls.
    assert!(
        emu.metrics.unknown_syscalls.is_empty(),
        "unexpected unknown syscalls: {:?}",
        emu.metrics.unknown_syscalls
    );
}

/// After InstNext fix: alloc_meta must set init flag and keep mask sane.
#[test]
fn alloc_meta_init_and_mask_sane_after_first_meta() {
    let mut emu = make_emu(950);
    emu.run().unwrap();
    let ram = emu.state.ram_space();
    let init = {
        let b = emu.state.read_space(ram, 0x1007F20, 4).unwrap();
        u32::from_le_bytes(b.try_into().unwrap())
    };
    let mask = {
        let b = emu.state.read_space(ram, 0x1007F40, 8).unwrap();
        u64::from_le_bytes(b.try_into().unwrap())
    };
    let secret = {
        let b = emu.state.read_space(ram, 0x1007F18, 8).unwrap();
        u64::from_le_bytes(b.try_into().unwrap())
    };
    assert_eq!(init, 1, "alloc_meta init_done must stick (RIP-relative mov imm)");
    assert_ne!(secret, 0, "secret from AT_RANDOM");
    // mask is page count remaining — must not be the bogus (2<<56)-1 pattern
    assert!(
        mask < 0x1000,
        "mask=0x{mask:X} looks like InstNext-corrupted page count"
    );
}
