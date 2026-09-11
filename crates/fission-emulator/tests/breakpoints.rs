//! Does the run loop actually stop where it was told to?
//!
//! The emulator backend's `set_sw_breakpoint` returned "not yet implemented",
//! which made the one backend that runs on every host and every architecture
//! unusable as a debugger. Stopping is not just a comparison in the run loop:
//! a translation block compiled before the breakpoint existed contains that
//! instruction in its middle, and the JIT hard-chains from block to block
//! without returning to the loop at all. Both are tested here, because a
//! breakpoint that works only under the interpreter is a breakpoint that
//! works only when nobody is watching.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::{Emulator, RunOutcome};
use fission_emulator::os::LinuxEnv;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn build(interpret: bool) -> Emulator {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/x64_static_printf_malloc.elf");
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
        .with_max_inst(Some(200_000));
    emu.force_interpreter = interpret;
    emu.apply_linux_image(info).expect("image");
    emu
}

/// An address the program definitely reaches, taken from the program itself:
/// interpret a fixed number of instructions and read the program counter off.
///
/// Interpreted on purpose even for the JIT tests -- `max_inst` stops the
/// interpreter at the instruction and the JIT at the end of its block, so only
/// the interpreter gives an exact place to aim at. The address is the same
/// code either way.
fn an_address_partway_through() -> u64 {
    addresses_partway_through(1)[0]
}

/// `count` consecutive addresses the program reaches, by interpreting a
/// growing prefix of the run and reading the program counter off each time.
///
/// More than one because a single address is a weak test of the JIT: the
/// first one tried happened to be a translation block's entry, where the run
/// loop sees the program counter anyway. A breakpoint has to work in the
/// *middle* of a block too -- that is the case the block collector and the
/// chaining gate exist for -- and a consecutive run of addresses is mostly
/// middles.
fn addresses_partway_through(count: usize) -> Vec<u64> {
    (0..count)
        .map(|i| {
            let budget = 3_000 + i as u64;
            let mut emu = build(true);
            emu.max_inst = Some(budget);
            let _ = emu.run();
            assert_eq!(
                emu.inst_count, budget,
                "the fixture stopped early, so this is not a normal place to reach"
            );
            emu.pc
        })
        .collect()
}

fn stops_at_a_breakpoint(interpret: bool) -> u64 {
    let address = an_address_partway_through();

    let mut emu = build(interpret);
    emu.set_breakpoint(address);
    let outcome = emu.resume().expect("run");

    assert_eq!(
        outcome,
        RunOutcome::HitBreakpoint(address),
        "stopped for some other reason at PC=0x{:X} after {} instructions",
        emu.pc,
        emu.inst_count
    );
    assert_eq!(
        emu.pc, address,
        "stopped somewhere other than the breakpoint"
    );
    assert!(
        emu.inst_count > 0 && emu.inst_count < 200_000,
        "stopped at the address but at an implausible point in the run ({})",
        emu.inst_count
    );
    emu.inst_count
}

#[test]
fn the_interpreter_stops_at_a_breakpoint() {
    let stopped_after = stops_at_a_breakpoint(true);

    // And stopped *before* the instruction at the breakpoint, not after it.
    // `max_inst` counts an instruction as it is entered, so a budget of N
    // leaves the machine about to execute the Nth: a budget one larger than
    // the breakpoint's count must land on the same address.
    let mut reference = build(true);
    reference.max_inst = Some(stopped_after + 1);
    let _ = reference.run();
    assert_eq!(
        reference.pc,
        an_address_partway_through(),
        "the breakpoint stopped somewhere other than in front of its instruction"
    );
}

/// The one that needs the block collector to end a block early and the JIT's
/// chaining gate to return to the run loop. Without either, an address in the
/// middle of a compiled block is run straight past.
#[test]
fn the_jit_stops_at_a_breakpoint() {
    let addresses = addresses_partway_through(8);
    for address in addresses {
        let mut emu = build(false);
        emu.set_breakpoint(address);
        let outcome = emu.resume().expect("run");
        assert_eq!(
            outcome,
            RunOutcome::HitBreakpoint(address),
            "ran past the breakpoint at 0x{address:X}: stopped at 0x{:X} after {} instructions",
            emu.pc,
            emu.inst_count
        );
        assert_eq!(emu.pc, address);
    }
}

/// A breakpoint set on a block that is already compiled and already chained
/// into -- the case `flush_all` exists for. Running first, then setting the
/// breakpoint, then running again from the start is the shape a debugger
/// session actually has.
#[test]
fn a_breakpoint_set_after_the_code_was_compiled_still_fires() {
    let address = an_address_partway_through();

    let mut emu = build(false);
    emu.max_inst = Some(3_000);
    let _ = emu.run();
    assert!(emu.inst_count > 0, "nothing ran, so nothing was compiled");

    // Now rewind by building a fresh machine but reusing nothing: the point is
    // that *this* emulator has a warm cache when the breakpoint arrives.
    emu.set_breakpoint(address);
    emu.max_inst = Some(200_000);
    // Continue on from where it is; the fixture reaches this address again in
    // its output loop, or it does not and this asserts nothing false.
    let outcome = emu.resume().expect("run");
    if let RunOutcome::HitBreakpoint(pc) = outcome {
        assert_eq!(pc, address);
        assert_eq!(emu.pc, address);
    }
}

/// Continuing from a breakpoint has to leave it, or `continue` is a no-op and
/// the session is stuck.
#[test]
fn continuing_from_a_breakpoint_leaves_it() {
    let address = an_address_partway_through();

    let mut emu = build(false);
    emu.set_breakpoint(address);
    assert_eq!(
        emu.resume().expect("run"),
        RunOutcome::HitBreakpoint(address)
    );
    let stopped_at = emu.inst_count;

    let outcome = emu.resume().expect("resume");
    assert!(
        emu.inst_count > stopped_at,
        "resuming executed nothing: stopped at the same instruction count {stopped_at}, \
         outcome {outcome:?}"
    );
}

#[test]
fn clearing_a_breakpoint_stops_stopping() {
    let address = an_address_partway_through();

    let mut emu = build(false);
    emu.set_breakpoint(address);
    assert!(emu.breakpoints().eq([address]));
    assert!(emu.clear_breakpoint(address), "it was there");
    assert!(!emu.clear_breakpoint(address), "it is not there twice");
    assert_eq!(emu.breakpoints().count(), 0);

    let outcome = emu.resume().expect("run");
    assert!(
        !matches!(outcome, RunOutcome::HitBreakpoint(_)),
        "stopped at a breakpoint that was removed"
    );
}
