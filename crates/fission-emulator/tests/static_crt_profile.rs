//! Profile static musl CRT: bounded max_inst + syscall/userop dump.
//!
//! After max_inst is honored inside hard-chain, this finishes quickly and
//! records which syscalls/HLE the CRT hit before the limit.

use std::path::PathBuf;

use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_emulator::MachineState;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

#[test]
fn profile_static_printf_malloc_first_budget() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
    assert!(path.is_file(), "missing {}", path.display());

    let binary = LoadedBinary::from_file(&path).expect("load");
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary).expect("elf");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("fe")
        .into_iter()
        .next()
        .expect("sleigh");
    let lang_id = load_spec.pair.language_id.as_str();
    let arch = ArchInfo::from_language_id(lang_id, Some(&binary)).expect("arch");
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .expect("emu")
        .with_max_inst(Some(512));
    emu.apply_linux_image(info).expect("image");
    // max_inst disables TB hard-chain so the outer loop can enforce the budget.
    emu.run().expect("run");

    // Budget must be respected (no infinite hard-chain hang).
    assert!(
        emu.inst_count <= 512 + 32,
        "max_inst not honored: inst={}",
        emu.inst_count
    );
    assert!(
        emu.inst_count > 5,
        "expected CRT progress, got {}",
        emu.inst_count
    );

    eprintln!(
        "static CRT profile @{}: halt={} exit={:?} fs_base=0x{:X} tidptr=0x{:X}",
        emu.inst_count,
        emu.halt_requested,
        emu.metrics.exit_reason,
        emu.fs_base,
        emu.clear_child_tid
    );
    eprintln!("  syscalls: {:?}", emu.metrics.syscalls);
    eprintln!("  userops: {:?}", emu.metrics.userops);
    eprintln!("  hle_miss: {:?}", emu.metrics.hle_misses);
    eprintln!("  unk_sys: {:?}", emu.metrics.unknown_syscalls);
    eprintln!("  unimpl: {:?}", emu.metrics.top_unimplemented(8));
    eprintln!("  summary: {}", emu.metrics.summary_line());

    // CRT typically hits arch_prctl (158) and/or set_tid_address (218).
    let saw_tls = emu.metrics.syscalls.contains_key(&158)
        || emu.metrics.syscalls.contains_key(&218)
        || emu.fs_base != 0
        || emu.clear_child_tid != 0;
    if !saw_tls {
        eprintln!("note: no TLS syscalls in first 3k (may enter later)");
    }
}
