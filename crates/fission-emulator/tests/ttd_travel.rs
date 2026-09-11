//! Does time travel arrive where it says?
//!
//! The TTD layer records snapshots and `ttd_seek` restores one. What nothing
//! checked is the only property that makes either worth having: seeking to
//! step N has to put the machine in the state it was in at step N. The
//! existing test asserts that `ttd_seek` returns `Ok` and that the instruction
//! count lands near the target -- both of which a `seek` that restored nothing
//! at all would also satisfy, and its body is skipped entirely when no
//! snapshot was recorded.
//!
//! So this compares against the machine itself: run one emulator to step N and
//! read its state; run another past N, seek back to N, and read the same
//! state. They are the same run, so they must agree.
//!
//! Both are interpreted, because `max_inst` stops the interpreter at the
//! instruction and the JIT at the end of its block -- a difference that would
//! show up here as a disagreement about something other than time travel.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::Emulator;
use fission_emulator::os::LinuxEnv;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

const GP: [&str; 16] = [
    "RAX", "RBX", "RCX", "RDX", "RSI", "RDI", "RBP", "RSP", "R8", "R9", "R10", "R11", "R12", "R13",
    "R14", "R15",
];

fn build(max_inst: u64, ttd_interval: u64) -> Emulator {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
    build_at(&path, max_inst, ttd_interval)
}

/// An aarch64 ELF, from the dev corpus: the crate carries no aarch64 fixture,
/// and this architecture is the whole point of the test below.
fn build_aarch64(max_inst: u64, ttd_interval: u64) -> Option<Emulator> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../fission-benchmark/corpus/dev/binaries/c/control_flow_gcc-aarch64_O0");
    path.is_file()
        .then(|| build_at(&path, max_inst, ttd_interval))
}

fn build_at(path: &std::path::Path, max_inst: u64, ttd_interval: u64) -> Emulator {
    let binary = LoadedBinary::from_file(path).expect("load");
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
        .with_max_inst(Some(max_inst));
    if ttd_interval > 0 {
        emu = emu.with_ttd(ttd_interval);
    }
    emu.force_interpreter = true;
    emu.apply_linux_image(info).expect("image");
    emu
}

/// Registers, plus the stack either side of the pointer.
///
/// Both sides on purpose. A snapshot stores the writes made *since the last
/// one* and restore applies them, so anything written after the point being
/// travelled to keeps its later value unless something undoes it. Reading only
/// below the stack pointer would mostly miss that, because a run that keeps
/// going pushes deeper rather than rewriting what it already left behind.
fn state_of(emu: &mut Emulator) -> (Vec<(String, u64)>, Vec<u8>) {
    let regs: Vec<(String, u64)> = GP
        .iter()
        .map(|n| ((*n).to_string(), emu.read_register_u64(n).unwrap_or(0)))
        .collect();
    let sp = emu.read_register_u64("RSP").unwrap_or(0);
    let ram = emu.state.ram_space();
    let window = emu
        .state
        .read_space(ram, sp.saturating_sub(2048), 4096)
        .unwrap_or_default();
    (regs, window)
}

#[test]
fn recording_a_run_records_something() {
    let mut emu = build(400, 4);
    let _ = emu.run();

    let stats = emu.ttd.stats();
    assert!(
        stats.count > 0,
        "a 400-instruction run with a snapshot every 4 recorded nothing"
    );

    // Registers first: a snapshot must hold the machine's own registers,
    // named the way the machine names them.
    let snapshot_registers = emu
        .ttd
        .latest_snapshot()
        .expect("a snapshot")
        .registers
        .clone();
    assert_ne!(snapshot_registers.pc, 0, "no program counter was recorded");
    let live_rsp = emu.read_register_u64("RSP").expect("live RSP");
    assert_eq!(
        snapshot_registers.get("RSP"),
        Some(live_rsp),
        "the snapshot's stack pointer is not the machine's"
    );
    for name in GP {
        assert!(
            snapshot_registers.get(name).is_some(),
            "{name} was not recorded"
        );
    }

    // And memory: `tracing_memory` is what makes deltas exist, and a run that
    // pushes a stack frame writes memory.
    let recorded: usize = emu
        .ttd
        .snapshots()
        .iter()
        .map(|s| s.memory_deltas.len())
        .sum();
    assert!(
        recorded > 0,
        "{} snapshots and not one memory delta between them",
        stats.count
    );
}

#[test]
fn seeking_back_arrives_where_the_run_was() {
    // A step to travel to, and a later one to travel from.
    let mut traveler = build(600, 4);
    let _ = traveler.run();
    let Some(target) = traveler
        .ttd
        .snapshots()
        .iter()
        .map(|s| s.step_index)
        .find(|step| *step >= 200)
    else {
        let steps: Vec<u64> = traveler
            .ttd
            .snapshots()
            .iter()
            .map(|s| s.step_index)
            .collect();
        panic!(
            "no snapshot at or after step 200 to seek to; ran {} instructions, \
             recorded {} snapshots at steps {:?}",
            traveler.inst_count,
            steps.len(),
            &steps[..steps.len().min(20)]
        );
    };

    // What the machine was actually doing after `target` instructions.
    //
    // `max_inst` is a limit on instructions *entered*: the interpreter counts
    // one, then checks, then executes. So stopping with `inst_count == N` means
    // N-1 have run and the Nth is about to. A snapshot recorded when
    // `inst_count == N` has all N behind it, so the reference needs one more.
    let mut reference = build(target + 1, 0);
    let _ = reference.run();
    assert_eq!(
        reference.inst_count,
        target + 1,
        "the reference run did not stop on the step being compared"
    );
    let (want_regs, want_stack) = state_of(&mut reference);
    let want_pc = reference.pc;

    // Before seeking, the traveler is 600 instructions in. If that looked the
    // same as step `target` the comparison below would pass without the seek
    // having done anything, and this test would be measuring nothing.
    let (end_regs, end_stack) = state_of(&mut traveler);
    assert!(
        end_regs != want_regs || end_stack != want_stack,
        "the end of the run and the step being sought are indistinguishable, \
         so this test cannot tell a working seek from one that does nothing"
    );

    traveler.ttd_seek(target).expect("seek");
    let (got_regs, got_stack) = state_of(&mut traveler);

    assert_eq!(
        traveler.inst_count, target,
        "seek landed on a different step"
    );
    assert_eq!(
        traveler.pc, want_pc,
        "seek restored a different program counter: 0x{:X} vs 0x{want_pc:X}",
        traveler.pc
    );
    let differing: Vec<String> = want_regs
        .iter()
        .zip(got_regs.iter())
        .filter(|((_, a), (_, b))| a != b)
        .map(|((n, a), (_, b))| format!("{n}: want 0x{a:X} got 0x{b:X}"))
        .collect();
    assert!(
        differing.is_empty(),
        "seek restored different registers: {differing:?}"
    );
    assert_eq!(
        want_stack, got_stack,
        "seek restored different memory below the stack pointer"
    );
}

/// Time travel on an architecture whose registers are not called `RAX`.
///
/// `RegisterState` used to be eighteen x86-64 fields, so on aarch64 a
/// recorded snapshot held sixteen zeroes and `ttd_seek` failed on its first
/// `write_register_u64("RAX", ..)` -- the recorder stored snapshots that
/// could never be restored, and said nothing. Neither existing test could
/// see it: both run the x86-64 fixture.
#[test]
fn time_travel_works_on_an_architecture_that_is_not_x86() {
    let Some(mut traveler) = build_aarch64(2_000, 50) else {
        eprintln!("skipped: no aarch64 binary in the dev corpus");
        return;
    };
    let _ = traveler.run();

    let snapshot = traveler
        .ttd
        .latest_snapshot()
        .expect("a snapshot")
        .registers
        .clone();
    assert_eq!(
        snapshot.get("RAX"),
        None,
        "an aarch64 machine has no RAX; absent is the answer, not zero"
    );
    for name in ["X0", "X29", "X30", "SP"] {
        assert!(snapshot.get(name).is_some(), "{name} was not recorded");
    }
    let live_sp = traveler.read_register_u64("SP").expect("live SP");
    assert_eq!(
        snapshot.get("SP"),
        Some(live_sp),
        "the snapshot's stack pointer is not the machine's"
    );

    // And the round trip, against the machine itself.
    let target = traveler
        .ttd
        .snapshots()
        .iter()
        .map(|s| s.step_index)
        .find(|step| *step >= 500)
        .expect("a snapshot at or after step 500");

    let mut reference = build_aarch64(target + 1, 0).expect("the same binary");
    let _ = reference.run();
    let want_pc = reference.pc;
    let want: Vec<(String, u64)> = snapshot
        .iter()
        .map(|(name, _)| {
            (
                name.to_string(),
                reference.read_register_u64(name).unwrap_or(0),
            )
        })
        .collect();

    traveler.ttd_seek(target).expect("seek");
    assert_eq!(traveler.inst_count, target, "seek landed on another step");
    assert_eq!(
        traveler.pc, want_pc,
        "seek restored another program counter"
    );
    let differing: Vec<String> = want
        .iter()
        .filter_map(|(name, wanted)| {
            let got = traveler.read_register_u64(name).unwrap_or(0);
            (got != *wanted).then(|| format!("{name}: want 0x{wanted:X} got 0x{got:X}"))
        })
        .collect();
    assert!(
        differing.is_empty(),
        "seek restored different registers: {differing:?}"
    );
}

/// Vector registers come back too.
///
/// `RegisterState` carried `u64`s, and `read_register_u64` refuses anything
/// wider, so `XMM0`..`XMM15` were neither recorded nor restored: a seek left
/// them holding whatever the *later* part of the run had put there. The
/// fixture uses SSE (glibc's `memcpy`/`strlen` do), so the difference is
/// visible without writing any assembly.
#[test]
fn seeking_back_restores_vector_registers() {
    let mut traveler = build(4_000, 16);
    let _ = traveler.run();

    let target = traveler
        .ttd
        .snapshots()
        .iter()
        .map(|s| s.step_index)
        .find(|step| *step >= 1_000)
        .expect("a snapshot at or after step 1000");

    // The vector registers the snapshot recorded.
    let snapshot = traveler
        .ttd
        .get_snapshot(target)
        .expect("that snapshot")
        .registers
        .clone();
    let vectors: Vec<(String, Vec<u8>)> = snapshot
        .iter_all()
        .filter(|(_, value)| value.byte_len() > 8)
        .map(|(name, value)| (name.to_string(), value.to_bytes()))
        .collect();
    assert!(
        !vectors.is_empty(),
        "no register wider than eight bytes was recorded at all"
    );

    // What the machine really held there.
    let mut reference = build(target + 1, 0);
    let _ = reference.run();
    let want: Vec<(String, Vec<u8>)> = vectors
        .iter()
        .map(|(name, _)| {
            let (space, offset, size) = reference
                .register_map
                .iter()
                .find(|(known, _)| known.eq_ignore_ascii_case(name))
                .map(|(_, v)| *v)
                .expect("the reference machine has the same registers");
            (
                name.clone(),
                reference
                    .state
                    .read_space(space, offset, size as usize)
                    .expect("read"),
            )
        })
        .collect();

    // The test is only worth anything if the end of the run differs from the
    // point being sought -- otherwise restoring nothing would also pass.
    let at_end: Vec<Vec<u8>> = want
        .iter()
        .map(|(name, _)| {
            let (space, offset, size) = traveler
                .register_map
                .iter()
                .find(|(known, _)| known.eq_ignore_ascii_case(name))
                .map(|(_, v)| *v)
                .expect("register");
            traveler
                .state
                .read_space(space, offset, size as usize)
                .expect("read")
        })
        .collect();
    assert!(
        at_end
            .iter()
            .zip(want.iter())
            .any(|(end, (_, wanted))| end != wanted),
        "every vector register is the same at the end of the run as at the target, \
         so this cannot tell a working restore from one that does nothing"
    );

    traveler.ttd_seek(target).expect("seek");

    let differing: Vec<String> = want
        .iter()
        .filter_map(|(name, wanted)| {
            let (space, offset, size) = traveler
                .register_map
                .iter()
                .find(|(known, _)| known.eq_ignore_ascii_case(name))
                .map(|(_, v)| *v)
                .expect("register");
            let got = traveler
                .state
                .read_space(space, offset, size as usize)
                .expect("read");
            (&got != wanted).then(|| format!("{name}: want {wanted:02x?} got {got:02x?}"))
        })
        .collect();
    assert!(
        differing.is_empty(),
        "seek restored different vector registers: {differing:?}"
    );
}
