//! JIT shadow propagation + symbolic gate (concolic) unit tests.

use fission_emulator::core::Emulator;
use fission_emulator::jit::callbacks::{
    jit_shadow_binop, jit_shadow_copy, jit_shadow_load, jit_shadow_store, jit_sym_cbranch_gate,
};
use fission_emulator::pcode::page_map::prot;
use fission_emulator::MachineState;

/// Build a minimal Emulator enough for shadow callouts (hello fixture).
fn mini_emu() -> Emulator {
    use fission_emulator::arch::ArchInfo;
    use fission_emulator::os::LinuxEnv;
    use fission_loader::loader::LoadedBinary;
    use fission_sleigh::runtime::RuntimeSleighFrontend;
    use std::path::PathBuf;

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/linux_x64_hello_sys.elf");
    let binary = LoadedBinary::from_file(&path).expect("load");
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary).expect("elf");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("fe")
        .into_iter()
        .next()
        .expect("sleigh");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch");
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new())).expect("emu");
    emu.apply_linux_image(info).expect("image");
    emu
}

#[test]
fn shadow_load_copy_binop_reaches_cbranch_gate() {
    let mut emu = mini_emu();
    let ram = emu.state.ram_space();
    let uniq = emu.state.unique_space();
    // Map a private page and plant a tainted byte.
    emu.state
        .page_map
        .map_region(0x7000_0000, 0x1000, prot::RW, true);
    emu.state
        .write_space(ram, 0x7000_0000, &[0x2A])
        .expect("write");
    emu.state.set_shadow_memory(ram, 0x7000_0000, 77);

    let emu_ptr = &mut emu as *mut Emulator;

    // LOAD: mem → unique:0x10 (simulating dest varnode)
    jit_shadow_load(emu_ptr, uniq, 0x10, 1, ram, 0x7000_0000);
    assert_eq!(emu.state.get_shadow_memory(uniq, 0x10), Some(77));

    // COPY to unique:0x20
    jit_shadow_copy(emu_ptr, uniq, 0x20, 1, uniq, 0x10);
    assert_eq!(emu.state.get_shadow_memory(uniq, 0x20), Some(77));

    // BINOP (compare-like union) → unique:0x30 condition
    jit_shadow_binop(emu_ptr, uniq, 0x30, 1, uniq, 0x20, 0, 0);
    assert_eq!(emu.state.get_shadow_memory(uniq, 0x30), Some(77));

    // STORE taint back to memory
    jit_shadow_store(emu_ptr, ram, 0x7000_0010, 1, uniq, 0x30);
    assert_eq!(emu.state.get_shadow_memory(ram, 0x7000_0010), Some(77));

    // CBranch gate should stop
    emu.sym_events.clear();
    emu.sym_stop_requested = false;
    let stop = jit_sym_cbranch_gate(emu_ptr, 1, uniq, 0x30, 0x401000, 0x401010);
    assert_eq!(stop, 1);
    assert!(emu.sym_stop_requested);
    assert_eq!(emu.sym_events.len(), 1);
    assert_eq!(emu.sym_events[0].condition_node, Some(77));
}

#[test]
fn host_reg_file_mirrors_register_writes() {
    let mut state = MachineState::new();
    let reg = state.register_space();
    let off = 0x08u64;
    state
        .write_space(reg, off, &0xDEAD_BEEF_CAFE_BABEu64.to_le_bytes())
        .unwrap();
    assert!(state.host_reg_in_range(off, 8));
    let slice = &state.host_reg_file[off as usize..off as usize + 8];
    assert_eq!(
        u64::from_le_bytes(slice.try_into().unwrap()),
        0xDEAD_BEEF_CAFE_BABE
    );
    // Cached read path
    let hits = state.reg_cache_hits;
    let _ = state.read_space(reg, off, 8).unwrap();
    assert!(state.reg_cache_hits >= hits);
}
