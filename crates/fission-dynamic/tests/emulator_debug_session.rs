//! Can the emulator backend actually drive a debugging session?
//!
//! It is the one backend that works on every host and every architecture --
//! the native ones need a matching OS and a matching CPU -- and until now it
//! answered "SW breakpoints not yet implemented" to the most basic question a
//! debugger is asked. This drives the sequence a person actually performs:
//! launch, set a breakpoint, continue, read registers and memory, write
//! memory back, continue again.
//!
//! Feature-gated the same way the backend is. Nothing here executes anything
//! but the crate's own fixture.
#![cfg(feature = "interactive_runtime")]

use fission_dynamic::debug::emulator_backend::EmulatorBackend;
use fission_dynamic::debug::traits::ExecutionBackend;

fn fixture() -> String {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fission-emulator/testdata/x64_static_printf_malloc.elf"
    )
    .to_string()
}

/// An address the program reaches, found by stepping the backend itself
/// rather than by guessing one.
fn address_partway_in(steps: usize) -> u64 {
    let mut backend = EmulatorBackend::new();
    backend.launch(&fixture(), &[]).expect("launch");
    for _ in 0..steps {
        backend.single_step().expect("step");
    }
    backend.fetch_registers(1).expect("registers").pc
}

#[test]
fn a_session_launches_breaks_reads_and_continues() {
    let target = address_partway_in(40);

    let mut backend = EmulatorBackend::new();
    let pid = backend.launch(&fixture(), &[]).expect("launch");
    assert!(backend.is_attached());
    assert_eq!(backend.attached_pid(), Some(pid));

    backend.set_sw_breakpoint(target).expect("set breakpoint");
    backend.continue_execution().expect("continue");

    let regs = backend.fetch_registers(1).expect("registers");
    assert_eq!(
        regs.pc, target,
        "continue did not stop at the breakpoint (stopped at 0x{:X})",
        regs.pc
    );
    // And the registers are the machine's, not a zeroed struct.
    assert!(
        regs.get("RSP").is_some_and(|sp| sp != 0),
        "no stack pointer at the breakpoint: {:?}",
        regs.iter().collect::<Vec<_>>()
    );

    // Memory at the stack pointer, read and written back.
    let sp = regs.get("RSP").expect("RSP");
    let before = backend.read_memory(sp, 16).expect("read stack");
    assert_eq!(before.len(), 16);
    let mut patched = before.clone();
    patched[0] ^= 0xFF;
    backend.write_memory(sp, &patched).expect("write stack");
    assert_eq!(
        backend.read_memory(sp, 16).expect("re-read"),
        patched,
        "the write did not land"
    );

    // Removing it twice is an error the second time, rather than a silent
    // success that leaves a front end thinking it removed something.
    backend
        .remove_sw_breakpoint(target)
        .expect("remove breakpoint");
    assert!(backend.remove_sw_breakpoint(target).is_err());
}

#[test]
fn reading_unmapped_memory_is_an_error_not_zeroes() {
    let mut backend = EmulatorBackend::new();
    backend.launch(&fixture(), &[]).expect("launch");
    assert!(
        backend.read_memory(0xDEAD_0000_0000, 16).is_err(),
        "an unmapped address read back as data"
    );
}

/// The write path used to discard its error, so a patch to an unmapped
/// address reported success.
#[test]
fn writing_unmapped_memory_is_an_error_not_success() {
    let mut backend = EmulatorBackend::new();
    backend.launch(&fixture(), &[]).expect("launch");
    assert!(
        backend.write_memory(0xDEAD_0000_0000, &[0x90; 4]).is_err(),
        "a write to an unmapped address reported success"
    );
}

/// A step is one instruction.
///
/// `single_step` called `run_instruction`, which runs a whole translation
/// block -- so a step advanced between one and eight instructions depending
/// on where the block boundaries fell, and a front end counting steps was
/// counting something else.
#[test]
fn a_single_step_is_one_instruction() {
    let mut backend = EmulatorBackend::new();
    backend.launch(&fixture(), &[]).expect("launch");

    for _ in 0..20 {
        let before = backend.emulator.as_ref().unwrap().inst_count;
        let pc_before = backend.fetch_registers(1).unwrap().pc;
        backend.single_step().expect("step");
        let after = backend.emulator.as_ref().unwrap().inst_count;
        assert_eq!(
            after - before,
            1,
            "one step ran {} instructions, from 0x{pc_before:X}",
            after - before
        );
    }
}

/// Stepping over a call lands on the instruction after it, however much the
/// call did in between.
#[test]
fn step_over_runs_the_call_and_comes_back() {
    let mut backend = EmulatorBackend::new();
    backend.launch(&fixture(), &[]).expect("launch");

    // Walk until the next instruction is a call.
    let mut found = None;
    for _ in 0..4_000 {
        let shape = backend
            .emulator
            .as_mut()
            .unwrap()
            .instruction_at_pc()
            .expect("decode");
        if shape.is_call {
            found = Some(shape);
            break;
        }
        backend.single_step().expect("step");
    }
    let shape = found.expect("the fixture makes a call in its first few thousand instructions");

    let before = backend.emulator.as_ref().unwrap().inst_count;
    backend.step_over().expect("step over");
    let after = backend.emulator.as_ref().unwrap().inst_count;

    assert_eq!(
        backend.fetch_registers(1).unwrap().pc,
        shape.fall_through(),
        "step over did not land after the call at 0x{:X}",
        shape.address
    );
    assert!(
        after - before > 1,
        "step over ran {} instructions, so it did not run the call",
        after - before
    );
}

/// Step until the next instruction is a call, and return its shape.
fn walk_to_a_call(backend: &mut EmulatorBackend) -> fission_emulator::core::InstructionShape {
    for _ in 0..4_000 {
        let shape = backend
            .emulator
            .as_mut()
            .unwrap()
            .instruction_at_pc()
            .expect("decode");
        if shape.is_call {
            return shape;
        }
        backend.single_step().expect("step");
    }
    panic!("the fixture makes a call in its first few thousand instructions");
}

fn drain_events(backend: &mut EmulatorBackend) -> Vec<fission_dynamic::debug::types::DebugEvent> {
    let mut events = Vec::new();
    while let Some(event) = backend.poll_event(0).expect("poll") {
        events.push(event);
    }
    events
}

/// A completed step reports itself, whichever kind of step it was.
///
/// Stepping over a call ends with the run outcome `Returned`, which had no
/// event, so a front end waiting for the step to finish saw nothing at all.
#[test]
fn step_over_a_call_reports_a_single_step_like_any_other_step() {
    use fission_dynamic::debug::types::DebugEvent;

    let mut backend = EmulatorBackend::new();
    backend.launch(&fixture(), &[]).expect("launch");
    let shape = walk_to_a_call(&mut backend);

    // Positive control: an ordinary step, one instruction before, reported one.
    let before_call = drain_events(&mut backend);
    assert!(
        matches!(before_call.last(), Some(DebugEvent::SingleStep { .. })),
        "the steps up to the call reported no single step: {before_call:?}"
    );

    backend.step_over().expect("step over");
    assert_eq!(backend.fetch_registers(1).unwrap().pc, shape.fall_through());
    let events = drain_events(&mut backend);
    assert!(
        events
            .iter()
            .any(|event| matches!(event, DebugEvent::SingleStep { thread_id: 1 })),
        "stepping over the call at 0x{:X} reported {events:?}",
        shape.address
    );
}

/// A step over a call that fails takes its temporary breakpoint with it.
#[test]
fn a_failed_step_over_leaves_no_breakpoint_behind() {
    let mut backend = EmulatorBackend::new();
    backend.launch(&fixture(), &[]).expect("launch");
    let pc = walk_to_a_call(&mut backend).address;

    // A direct call whose target nothing maps: `call rel32` to 0x7000_0000_0000.
    let unmapped: u64 = 0x7000_0000_0000;
    assert!(backend.read_memory(unmapped, 1).is_err());
    let rel = (unmapped as i64 - (pc as i64 + 5)) as i32;
    let mut call = vec![0xE8];
    call.extend_from_slice(&rel.to_le_bytes());
    // The code page is read-only to the guest; this is the test planting an
    // instruction, not a guest write, so the page check is lifted for it.
    {
        let emu = backend.emulator.as_mut().unwrap();
        let enforce = std::mem::replace(&mut emu.state.enforce_page_faults, false);
        let ram = emu.state.ram_space();
        emu.state
            .write_space(ram, pc, &call)
            .expect("patch the call");
        emu.state.enforce_page_faults = enforce;
        emu.invalidate_translations(pc, call.len());
    }
    let shape = backend
        .emulator
        .as_mut()
        .unwrap()
        .instruction_at_pc()
        .expect("decode the patched call");
    assert!(shape.is_call);
    let fall_through = shape.fall_through();

    // Positive control: nothing is set there beforehand.
    let breakpoints = |backend: &EmulatorBackend| -> Vec<u64> {
        backend.emulator.as_ref().unwrap().breakpoints().collect()
    };
    assert!(!breakpoints(&backend).contains(&fall_through));

    let result = backend.step_over();
    assert!(
        result.is_err(),
        "a call into unmapped memory completed: {result:?}"
    );
    assert!(
        !breakpoints(&backend).contains(&fall_through),
        "the temporary breakpoint at 0x{fall_through:X} outlived the failed step"
    );
}

/// Stepping out arrives above the frame it started in.
#[test]
fn step_out_returns_from_the_function() {
    let mut backend = EmulatorBackend::new();
    backend.launch(&fixture(), &[]).expect("launch");

    // Get inside a function: step until a call, then step *into* it.
    let mut entered = false;
    for _ in 0..4_000 {
        let shape = backend
            .emulator
            .as_mut()
            .unwrap()
            .instruction_at_pc()
            .expect("decode");
        backend.single_step().expect("step");
        if shape.is_call {
            entered = true;
            break;
        }
    }
    assert!(entered, "never entered a function");

    let sp_inside = backend.fetch_registers(1).unwrap().get("RSP").unwrap();
    backend.step_out().expect("step out");
    let sp_after = backend.fetch_registers(1).unwrap().get("RSP").unwrap();

    assert!(
        sp_after > sp_inside,
        "step out left the stack pointer at or below where it started \
         (0x{sp_inside:X} -> 0x{sp_after:X}), so it did not return"
    );
}

/// Writing registers back, and an error rather than a partial write when the
/// machine does not have one of them.
#[test]
fn registers_can_be_written_back() {
    let mut backend = EmulatorBackend::new();
    backend.launch(&fixture(), &[]).expect("launch");
    backend.single_step().expect("step");

    let mut regs = backend.fetch_registers(1).expect("registers");
    let original = regs.get("RBX").expect("RBX");
    regs.set("RBX", original ^ 0xDEAD_BEEF);
    backend.set_registers(1, &regs).expect("set registers");

    assert_eq!(
        backend.fetch_registers(1).unwrap().get("RBX"),
        Some(original ^ 0xDEAD_BEEF),
        "the register write did not land"
    );

    let mut wrong = RegisterStateForTest::at(0);
    wrong.set("X0", 1);
    assert!(
        backend.set_registers(1, &wrong).is_err(),
        "writing a register this machine does not have reported success"
    );
}

use fission_dynamic::debug::types::RegisterState as RegisterStateForTest;

/// The arguments given to `launch` are the guest's `argv`: at process entry the
/// stack pointer addresses `argc`, which is the program path plus each
/// argument. `launch` used to drop them, so a program reading its arguments
/// always saw `argc == 1`.
fn guest_argc(args: &[String]) -> u64 {
    let mut backend = EmulatorBackend::new();
    backend.launch(&fixture(), args).expect("launch");
    let sp = backend
        .fetch_registers(1)
        .expect("registers")
        .get("RSP")
        .expect("RSP");
    let bytes = backend
        .read_memory(sp, 8)
        .expect("argc at the stack pointer");
    u64::from_le_bytes(bytes.try_into().expect("8 bytes"))
}

#[test]
fn launch_passes_its_arguments_to_the_guest_as_argv() {
    let args = vec!["alpha".to_string(), "beta".to_string()];
    assert_eq!(guest_argc(&args), 3, "program path + two arguments");
}

#[test]
fn launch_without_arguments_still_starts_the_guest_with_argc_one() {
    assert_eq!(guest_argc(&[]), 1);
}

/// The emulator runs one guest context, thread 1; `set_current_thread`
/// already refuses any other id, and the register calls take the same id.
#[test]
fn register_access_refuses_a_thread_the_emulator_does_not_have() {
    let mut backend = EmulatorBackend::new();
    backend.launch(&fixture(), &[]).expect("launch");

    let regs = backend.fetch_registers(1).expect("thread 1 exists");
    backend
        .set_registers(1, &regs)
        .expect("thread 1 is writable");

    let fetch = backend.fetch_registers(99);
    assert!(fetch.is_err(), "no thread 99, got {fetch:?}");
    let set = backend.set_registers(99, &regs);
    assert!(set.is_err(), "no thread 99, got {set:?}");
}
