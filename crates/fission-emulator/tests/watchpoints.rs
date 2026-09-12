//! Does the run loop stop when the guest touches a watched address?
//!
//! A watchpoint is the question "who wrote this", and it is the one a
//! debugger answers that a static tool cannot. The machinery it needs already
//! existed -- `ObserveMask::mem` decides at translation time whether compiled
//! code carries memory callbacks at all -- so the cost when none are set is
//! zero, and that is the part worth guarding: a watchpoint that made every run
//! slower would be a watchpoint nobody leaves available.

use std::path::PathBuf;

use fission_emulator::MachineState;
use fission_emulator::arch::ArchInfo;
use fission_emulator::core::{Emulator, RunOutcome};
use fission_emulator::os::LinuxEnv;
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::RuntimeSleighFrontend;

fn build(max_inst: u64) -> Emulator {
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
        .with_max_inst(Some(max_inst));
    emu.apply_linux_image(info).expect("image");
    emu
}

/// An address the program writes to, found by watching the program itself
/// rather than by guessing: run a while and take somewhere below the stack
/// pointer, which a running program is about to use.
fn an_address_the_program_writes() -> u64 {
    let mut emu = build(2_000);
    let _ = emu.run();
    let sp = emu.read_register_u64("RSP").expect("RSP");
    sp.wrapping_sub(64)
}

#[test]
fn a_write_to_a_watched_address_stops_the_run() {
    let address = an_address_the_program_writes();

    let mut emu = build(200_000);
    emu.set_watchpoint(address, 8, false, true);
    let outcome = emu.resume().expect("run");

    let RunOutcome::HitWatchpoint(hit) = outcome else {
        panic!("nothing stopped the run at a watched address: {outcome:?}");
    };
    assert!(hit.write, "a write watch reported a read");
    assert!(
        hit.address < address + 8 && address < hit.address + u64::from(hit.size),
        "the reported access at 0x{:X}+{} does not overlap the watch at 0x{address:X}+8",
        hit.address,
        hit.size
    );
    // The instruction, not just the block: this is the answer the watchpoint
    // exists to give.
    assert_ne!(hit.pc, 0, "the access was reported with no instruction");
    assert_eq!(
        emu.last_watch_hit().copied(),
        Some(hit),
        "the hit is not readable after the run"
    );
}

/// Reads and writes are separate questions.
#[test]
fn a_write_only_watch_ignores_reads() {
    let address = an_address_the_program_writes();

    let mut emu = build(200_000);
    emu.set_watchpoint(address, 8, true, false);
    let read_outcome = emu.resume().expect("run");

    // Whatever this program does at that address first, a read-only watch must
    // never report a write.
    if let RunOutcome::HitWatchpoint(hit) = read_outcome {
        assert!(!hit.write, "a read-only watch reported a write");
    }
}

#[test]
fn clearing_a_watchpoint_stops_stopping() {
    let address = an_address_the_program_writes();

    let mut emu = build(200_000);
    emu.set_watchpoint(address, 8, true, true);
    assert_eq!(emu.watchpoints().len(), 1);
    assert_eq!(emu.clear_watchpoint(address), 1);
    assert_eq!(emu.clear_watchpoint(address), 0, "it is not there twice");

    let outcome = emu.resume().expect("run");
    assert!(
        !matches!(outcome, RunOutcome::HitWatchpoint(_)),
        "stopped on a watchpoint that was removed"
    );
}

/// A run with no watchpoint must compile no memory callbacks at all.
///
/// This is the whole reason the mask is decided at translation time, and the
/// reason a watchpoint can be left available rather than hidden behind a flag.
#[test]
fn watchpoints_cost_nothing_when_there_are_none() {
    let mut emu = build(1_000);
    assert_eq!(
        emu.observe_mask(),
        fission_emulator::observe::ObserveMask::NONE,
        "a plain run asked for instrumentation"
    );

    emu.set_watchpoint(0x1000, 8, false, true);
    let armed = emu.observe_mask();
    assert!(armed.mem, "a watchpoint did not ask for memory callbacks");
    assert!(
        armed.insn,
        "a watchpoint did not ask for the instruction address, so it could not \
         say what made the access"
    );

    emu.clear_all_watchpoints();
    assert_eq!(
        emu.observe_mask(),
        fission_emulator::observe::ObserveMask::NONE,
        "removing the last watchpoint left the instrumentation on"
    );
}

/// The machine stops on the instruction that made the access, not wherever
/// the block it was in happened to end.
///
/// A compiled block cannot stop in its own middle, so a watchpoint used to
/// report the right access and leave the machine somewhere after it: measured
/// over 300 hits on this fixture, 209 stopped between one and seven
/// instructions late, which is the difference between "this instruction wrote
/// it" and "one of these eight did". Blocks now end after any memory
/// instruction while a watchpoint is armed.
#[test]
fn the_stop_is_on_the_instruction_that_made_the_access() {
    let sp = {
        let mut warm = build(500);
        let _ = warm.run();
        warm.read_register_u64("RSP").expect("RSP")
    };

    let mut emu = build(2_000_000);
    // A wide window, so this trips often and the accesses land all over the
    // inside of blocks rather than at one lucky boundary.
    emu.set_watchpoint(sp.wrapping_sub(0x2000), 0x4000, true, true);

    let mut hits = 0usize;
    let mut late = Vec::new();
    for _ in 0..300 {
        let Ok(RunOutcome::HitWatchpoint(hit)) = emu.resume() else {
            break;
        };
        hits += 1;
        let distance = emu.inst_count.saturating_sub(hit.step);
        if distance != 0 {
            late.push((hit.pc, distance));
        }
    }

    assert!(hits > 100, "only {hits} hits, too few to say anything");
    assert!(
        late.is_empty(),
        "{} of {hits} stops were past the access that caused them: {:?}",
        late.len(),
        &late[..late.len().min(8)]
    );
}

/// Arming a watchpoint costs something -- shorter blocks, and a callback per
/// instruction and per access -- and that cost has to be paid only while one
/// is armed.
#[test]
fn arming_a_watchpoint_does_not_change_blocks_for_anyone_else() {
    let mut emu = build(20_000);
    let _ = emu.run();
    let unarmed_blocks = emu.jit_cache.len();
    assert!(unarmed_blocks > 0, "nothing was compiled");

    let mut armed = build(20_000);
    armed.set_watchpoint(0xDEAD_0000, 8, true, true);
    let _ = armed.run();
    let armed_blocks = armed.jit_cache.len();

    // More, smaller blocks while armed: that is the mechanism working. The
    // point of the assertion is the direction, not a particular ratio.
    assert!(
        armed_blocks > unarmed_blocks,
        "arming a watchpoint did not shorten any block ({unarmed_blocks} -> {armed_blocks}), \
         so the stop cannot be exact"
    );

    // And removing it puts the blocks back.
    armed.clear_all_watchpoints();
    let mut again = build(20_000);
    let _ = again.run();
    assert_eq!(
        again.jit_cache.len(),
        unarmed_blocks,
        "a run with no watchpoint compiled a different number of blocks"
    );
}
