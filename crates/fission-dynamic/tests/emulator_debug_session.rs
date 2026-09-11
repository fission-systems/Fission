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
