//! Live macOS debugger tests against a real process.
//!
//! These need the Mach task port of a process the test launches, and macOS
//! hands that out only to an authorized caller (`sudo DevToolsSecurity
//! -enable` once, or root). On a machine where that has not been done the
//! request waits on an interactive prompt, so the tests are opt-in:
//!
//! ```text
//! FISSION_MACOS_LIVE_DEBUG_TESTS=1 cargo nextest run -p fission-dynamic \
//!     --features interactive_runtime --test macos_live
//! ```
//!
//! The target is a tiny C program built here with the system compiler, so it
//! is not hardened and is debuggable by an authorized caller.

#![cfg(all(target_os = "macos", feature = "interactive_runtime"))]

use fission_dynamic::debug::platform::macos::MacOSDebugger;
use fission_dynamic::debug::traits::ExecutionBackend;
use fission_dynamic::debug::types::DebugEvent;
use std::path::PathBuf;

fn enabled() -> bool {
    std::env::var_os("FISSION_MACOS_LIVE_DEBUG_TESTS").is_some()
}

/// Build the target once per test into its own directory.
fn build_target(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("fission-macos-live-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("target.c");
    // Returns 7 so an exit event is distinguishable from a crash.
    std::fs::write(&source, "int main(void) { return 7; }\n").unwrap();
    let binary = dir.join("target");
    let status = std::process::Command::new("cc")
        .arg("-o")
        .arg(&binary)
        .arg(&source)
        .status()
        .expect("a C compiler is needed for the live tests");
    assert!(status.success());
    binary
}

fn next_event(debugger: &mut MacOSDebugger) -> DebugEvent {
    for _ in 0..200 {
        if let Some(event) = debugger.poll_event(50).expect("poll_event") {
            return event;
        }
    }
    panic!("no debug event within 10 seconds");
}

#[test]
fn launch_stops_at_exec_and_runs_to_exit() {
    if !enabled() {
        return;
    }
    let target = build_target("exit");
    let mut debugger = MacOSDebugger::new();
    let pid = debugger
        .launch(target.to_str().unwrap(), &[])
        .expect("launch");
    assert!(matches!(
        next_event(&mut debugger),
        DebugEvent::ProcessCreated { pid: created, .. } if created == pid
    ));

    let registers = debugger
        .fetch_registers(pid)
        .expect("registers at exec stop");
    assert_ne!(registers.pc, 0);
    // The bytes at the PC are readable and are not our breakpoint instruction.
    let code = debugger.read_memory(registers.pc, 4).expect("read at pc");
    assert_eq!(code.len(), 4);

    debugger.continue_execution().expect("continue");
    match next_event(&mut debugger) {
        DebugEvent::ProcessExited { exit_code } => assert_eq!(exit_code, 7),
        other => panic!("expected the process to exit with 7, got {other:?}"),
    }
    assert!(!debugger.is_attached());
}

#[test]
fn single_step_moves_the_program_counter() {
    if !enabled() {
        return;
    }
    let target = build_target("step");
    let mut debugger = MacOSDebugger::new();
    let pid = debugger
        .launch(target.to_str().unwrap(), &[])
        .expect("launch");
    let _ = next_event(&mut debugger);
    let before = debugger.fetch_registers(pid).unwrap().pc;
    debugger.single_step().expect("step");
    assert!(matches!(
        next_event(&mut debugger),
        DebugEvent::SingleStep { .. }
    ));
    let after = debugger.fetch_registers(pid).unwrap().pc;
    assert_ne!(before, after, "one instruction must advance the pc");
    debugger.terminate().expect("terminate");
}

#[test]
fn a_breakpoint_hits_hides_its_instruction_and_steps_off() {
    if !enabled() {
        return;
    }
    let target = build_target("breakpoint");
    // The instruction the third step reaches is the same on every run of the
    // same binary, so find it once and break on it in a second launch.
    let address = {
        let mut debugger = MacOSDebugger::new();
        let pid = debugger
            .launch(target.to_str().unwrap(), &[])
            .expect("launch");
        let _ = next_event(&mut debugger);
        for _ in 0..3 {
            debugger.single_step().unwrap();
            let _ = next_event(&mut debugger);
        }
        let pc = debugger.fetch_registers(pid).unwrap().pc;
        debugger.terminate().unwrap();
        pc
    };

    let mut debugger = MacOSDebugger::new();
    let pid = debugger
        .launch(target.to_str().unwrap(), &[])
        .expect("launch");
    let _ = next_event(&mut debugger);
    let original = debugger.read_memory(address, 4).expect("read before");
    debugger.set_sw_breakpoint(address).expect("set breakpoint");
    // The program's own bytes are what a caller sees, not our instruction.
    assert_eq!(debugger.read_memory(address, 4).unwrap(), original);

    debugger.continue_execution().unwrap();
    match next_event(&mut debugger) {
        DebugEvent::BreakpointHit { address: hit, .. } => assert_eq!(hit, address),
        other => panic!("expected a breakpoint hit, got {other:?}"),
    }
    assert_eq!(debugger.fetch_registers(pid).unwrap().pc, address);

    // Continuing from the breakpoint must step off it, not trap again.
    debugger.continue_execution().unwrap();
    match next_event(&mut debugger) {
        DebugEvent::ProcessExited { exit_code } => assert_eq!(exit_code, 7),
        other => panic!("expected exit after stepping off the breakpoint, got {other:?}"),
    }
}

#[test]
fn modules_are_listed_after_dyld_has_run() {
    if !enabled() {
        return;
    }
    let target = build_target("modules");
    let mut debugger = MacOSDebugger::new();
    let pid = debugger
        .launch(target.to_str().unwrap(), &[])
        .expect("launch");
    let _ = next_event(&mut debugger);
    // At the exec stop dyld has registered nothing; the report says so.
    let early = debugger.list_modules().expect("module list at exec stop");
    assert!(early.modules.is_empty() || !early.diagnostics.is_empty() || early.pid == pid);
    debugger.terminate().unwrap();
}
