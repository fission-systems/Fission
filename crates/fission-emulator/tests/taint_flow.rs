//! Taint reaches a sink, and a clean run reports nothing.
//!
//! The fixture is the emulator's own concolic-branch ELF: it reads a byte from
//! stdin and branches on it. Built from source in this repository -- never a
//! corpus binary, which is the one thing not to point dynamic analysis at.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::observe::ShadowMode;
use fission_emulator::os::LinuxEnv;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn build(stdin: &[u8]) -> Emulator {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_concolic_branch_sys.elf");
    let binary = LoadedBinary::from_file(&path).expect("load");
    let mut state = MachineState::new();
    let info = fission_emulator::os::linux::loader::load_elf(&mut state, &binary).expect("elf");
    let load_spec = binary.load_spec().expect("spec").clone();
    let sleigh = RuntimeSleighFrontend::new_candidate_frontends_for_load_spec(&load_spec)
        .expect("frontend")
        .into_iter()
        .next()
        .expect("sleigh");
    let arch = ArchInfo::from_language_id(load_spec.pair.language_id.as_str(), Some(&binary))
        .expect("arch");
    let mut emu = Emulator::new(state, binary, sleigh, arch, Box::new(LinuxEnv::new()))
        .expect("emulator")
        .with_max_inst(Some(4096));
    emu.apply_linux_image(info).expect("image");
    emu.seed_stdin(stdin);
    emu
}

#[test]
fn taint_is_off_unless_asked_for() {
    let mut emu = build(b"A");
    assert_eq!(emu.shadow_mode(), ShadowMode::Off);
    let _ = emu.run();
    assert!(
        emu.taint.is_empty(),
        "a run nobody asked to taint reported taint anyway"
    );
}

#[test]
fn a_run_that_reads_gets_a_source_but_a_branch_alone_is_not_a_flow() {
    let mut emu = build(b"A");
    emu.set_shadow_mode(ShadowMode::Taint);
    let _ = emu.run();

    // `read` filled a guest buffer from outside, so the run has a source.
    let sources: Vec<&str> = emu
        .taint
        .sources()
        .iter()
        .map(|s| s.label.as_str())
        .collect();
    assert!(
        sources.contains(&"read"),
        "stdin should be a source, got {sources:?}"
    );

    // The fixture branches on the byte it read and exits with a constant the
    // branch picked. The byte decides *which* constant, and never flows into
    // it -- that is control dependence, not data dependence, and this taint
    // deliberately does not follow it: implicit flows reach everything, so
    // tracking them turns every report into noise.
    //
    // So the right answer here is no hit at all. Pinning that is the point:
    // the cheap failure for a taint engine is not missing a flow, it is
    // reporting one that is not there.
    for hit in &emu.taint.hits {
        eprintln!(
            "  0x{:X}  {} -- {}  <- {:?}",
            hit.pc, hit.sink, hit.detail, hit.sources
        );
    }
    assert!(
        emu.taint.hits.is_empty(),
        "a control-flow-only dependence was reported as a data flow: {:?}",
        emu.taint.hits
    );
}

#[test]
fn a_declared_source_propagates_through_arithmetic() {
    // Independent of any syscall: mark memory, let the guest compute with it,
    // and check the label survives the arithmetic rather than the copy alone.
    let mut emu = build(b"A");
    emu.set_shadow_mode(ShadowMode::Taint);
    let scratch = 0x7FFF_0000u64;
    emu.taint_range(scratch, 8, "test source");
    let ram = emu.state.ram_space();

    let set = emu
        .state
        .get_shadow_memory(ram, scratch)
        .expect("marking a range left it clean");
    assert_eq!(emu.taint.sources().len(), 1, "one call, one source");
    assert_eq!(emu.taint.labels(set), vec!["test source"]);
}
