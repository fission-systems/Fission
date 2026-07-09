//! Mallocng expand progress after TZCNT relative-branch fix.
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

fn r64(emu: &mut Emulator, a: u64) -> u64 {
    let b = emu
        .state
        .read_space(emu.state.ram_space(), a, 8)
        .unwrap_or_else(|_| vec![0; 8]);
    u64::from_le_bytes(b.try_into().unwrap())
}
fn r32(emu: &mut Emulator, a: u64) -> u32 {
    let b = emu
        .state
        .read_space(emu.state.ram_space(), a, 4)
        .unwrap_or_else(|_| vec![0; 4]);
    u32::from_le_bytes(b.try_into().unwrap())
}

/// After relative BRANCH remap fix, CRT must leave size-class hot PC and
/// establish non-zero freeable and/or bin heads under a modest budget.
#[test]
fn past_sizeclass_freeable_or_bin_progress() {
    let mut emu = make_emu(5_000);
    emu.run().unwrap();
    let pc = emu.pc;
    eprintln!(
        "stop_pc=0x{pc:X} inst={} exit={:?} sys={:?}",
        emu.inst_count, emu.metrics.exit_reason, emu.metrics.syscalls
    );
    // Must not remain glued to the pre-fix livelock site for the whole budget.
    assert_ne!(
        pc, 0x10035A3,
        "still stuck at mallocng size-class livelock 0x10035A3"
    );

    // Scan bins for any non-zero head and freeable on heap metas.
    let mut any_bin = false;
    for i in 0..32u64 {
        let h = r64(&mut emu, 0x1007f68 + i * 8);
        if h != 0 {
            any_bin = true;
            let freeable = r32(&mut emu, h + 0x18);
            eprintln!("bin[{i}]=0x{h:X} freeable={freeable}");
        }
    }
    let bin5 = r64(&mut emu, 0x1007f68 + 5 * 8);
    let freeable5 = if bin5 != 0 {
        r32(&mut emu, bin5 + 0x18)
    } else {
        0
    };
    assert!(
        any_bin || freeable5 > 0,
        "expected bin link or freeable progress; bin5=0x{bin5:X} freeable5={freeable5}"
    );
}
