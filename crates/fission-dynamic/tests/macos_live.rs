//! Live macOS debugger tests against a real process.
//!
//! These need the Mach task port of a process the test launches. macOS grants
//! that only to a caller that is **both** signed with the
//! `com.apple.security.cs.debugger` entitlement **and** authorized for
//! Developer Tools (`sudo DevToolsSecurity -enable`, once). An unsigned caller
//! is refused at once; a signed one on a machine without Developer Tools access
//! waits on a prompt. So the tests are opt-in and are run through the script
//! that signs the test binary first:
//!
//! ```text
//! scripts/macos/run-live-debugger-tests.sh
//! ```
//!
//! The targets are tiny C programs built here with the system compiler, so they
//! are not hardened and are debuggable by an authorized caller.

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

/// A target that reports where its own function is, then stops itself.
///
/// Addresses differ per run (dyld slides the image), so the breakpoint address
/// has to come from the process being debugged, not from an earlier run.
fn build_reporting_target(name: &str) -> (PathBuf, PathBuf) {
    let dir =
        std::env::temp_dir().join(format!("fission-macos-live-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("target.c");
    std::fs::write(
        &source,
        r#"
#include <signal.h>
#include <stdio.h>
__attribute__((noinline)) int marker(int x) { return x + 5; }
volatile int cell = 41;
int main(int argc, char **argv) {
    FILE *f = fopen(argv[1], "w");
    fprintf(f, "%lx\n%lx\n", (unsigned long)&marker, (unsigned long)&cell);
    fclose(f);
    raise(SIGSTOP);
    return argc > 2 ? cell : marker(2);
}
"#,
    )
    .unwrap();
    let binary = dir.join("target");
    let status = std::process::Command::new("cc")
        .arg("-O0")
        .arg("-o")
        .arg(&binary)
        .arg(&source)
        .status()
        .expect("a C compiler is needed for the live tests");
    assert!(status.success());
    (binary, dir.join("address.txt"))
}

#[test]
fn a_breakpoint_hits_hides_its_instruction_and_steps_off() {
    if !enabled() {
        return;
    }
    let (target, report) = build_reporting_target("breakpoint");
    let mut debugger = MacOSDebugger::new();
    let pid = debugger
        .launch(
            target.to_str().unwrap(),
            &[report.to_str().unwrap().to_string()],
        )
        .expect("launch");
    let _ = next_event(&mut debugger);
    debugger.continue_execution().unwrap();

    // The target wrote its function's address and stopped itself.
    match next_event(&mut debugger) {
        DebugEvent::Exception { code, .. } => assert_eq!(code, 17, "SIGSTOP"),
        other => panic!("expected the target's own SIGSTOP, got {other:?}"),
    }
    let (address, _) = reported_addresses(&report);

    let original = debugger.read_memory(address, 4).expect("read before");
    debugger.set_sw_breakpoint(address).expect("set breakpoint");
    // The program's own bytes are what a caller sees, not our instruction.
    assert_eq!(debugger.read_memory(address, 4).unwrap(), original);
    assert_eq!(debugger.list_breakpoints().len(), 1);

    debugger.continue_execution().unwrap();
    match next_event(&mut debugger) {
        DebugEvent::BreakpointHit { address: hit, .. } => assert_eq!(hit, address),
        other => panic!("expected a breakpoint hit, got {other:?}"),
    }
    assert_eq!(debugger.fetch_registers(pid).unwrap().pc, address);
    assert_eq!(debugger.list_breakpoints()[0].hits, 1);

    // Continuing from the breakpoint must step off it, not trap again.
    debugger.continue_execution().unwrap();
    match next_event(&mut debugger) {
        DebugEvent::ProcessExited { exit_code } => assert_eq!(exit_code, 7),
        other => {
            let pc = debugger.fetch_registers(pid).map(|r| r.pc);
            panic!(
                "expected exit after stepping off the breakpoint at {address:#x}, got {other:?} \
                 with pc {pc:?}"
            )
        }
    }
}

#[test]
fn removing_a_breakpoint_restores_the_instruction() {
    if !enabled() {
        return;
    }
    let (target, report) = build_reporting_target("remove");
    let mut debugger = MacOSDebugger::new();
    debugger
        .launch(
            target.to_str().unwrap(),
            &[report.to_str().unwrap().to_string()],
        )
        .expect("launch");
    let _ = next_event(&mut debugger);
    debugger.continue_execution().unwrap();
    let _ = next_event(&mut debugger); // the target's SIGSTOP
    let (address, _) = reported_addresses(&report);

    debugger.set_sw_breakpoint(address).unwrap();
    debugger.remove_sw_breakpoint(address).unwrap();
    assert!(debugger.list_breakpoints().is_empty());
    // With the breakpoint gone the target runs straight through to its exit.
    debugger.continue_execution().unwrap();
    match next_event(&mut debugger) {
        DebugEvent::ProcessExited { exit_code } => assert_eq!(exit_code, 7),
        other => panic!("expected a clean exit, got {other:?}"),
    }
}

#[test]
fn modules_are_listed_once_dyld_has_run() {
    if !enabled() {
        return;
    }
    let (target, report) = build_reporting_target("modules");
    let mut debugger = MacOSDebugger::new();
    let pid = debugger
        .launch(
            target.to_str().unwrap(),
            &[report.to_str().unwrap().to_string()],
        )
        .expect("launch");
    let _ = next_event(&mut debugger);
    // At the exec stop dyld has registered nothing, and the report says so.
    let early = debugger.list_modules().expect("module list at exec stop");
    assert_eq!(early.pid, pid);
    assert!(!early.diagnostics.is_empty() || !early.modules.is_empty());

    // The target stops itself after dyld has finished.
    debugger.continue_execution().unwrap();
    let _ = next_event(&mut debugger);
    let report = debugger.list_modules().expect("module list after dyld");
    assert!(
        report.modules.iter().any(|m| m.name == "target"),
        "the main executable must be listed: {:?}",
        report.modules.iter().map(|m| &m.name).collect::<Vec<_>>()
    );
    assert!(
        report.modules.iter().any(|m| m.name.contains("libSystem")),
        "libSystem must be listed"
    );
    debugger.terminate().unwrap();
}

fn reported_addresses(report: &std::path::Path) -> (u64, u64) {
    let text = std::fs::read_to_string(report).unwrap();
    let mut lines = text.lines();
    let parse = |line: Option<&str>| u64::from_str_radix(line.unwrap().trim(), 16).unwrap();
    (parse(lines.next()), parse(lines.next()))
}

#[test]
fn memory_and_registers_can_be_written() {
    if !enabled() {
        return;
    }
    let (target, report) = build_reporting_target("write");
    let mut debugger = MacOSDebugger::new();
    // A second argument makes the target return `cell` instead of `marker(2)`.
    let pid = debugger
        .launch(
            target.to_str().unwrap(),
            &[report.to_str().unwrap().to_string(), "use-cell".into()],
        )
        .expect("launch");
    let _ = next_event(&mut debugger);
    debugger.continue_execution().unwrap();
    let _ = next_event(&mut debugger); // the target's SIGSTOP
    let (_, cell) = reported_addresses(&report);

    assert_eq!(debugger.read_memory(cell, 4).unwrap(), 41i32.to_le_bytes());
    debugger
        .write_memory(cell, &42i32.to_le_bytes())
        .expect("write memory");
    assert_eq!(debugger.read_memory(cell, 4).unwrap(), 42i32.to_le_bytes());

    // A caller-saved scratch register written and read back.
    #[cfg(target_arch = "aarch64")]
    let scratch = "X15";
    #[cfg(target_arch = "x86_64")]
    let scratch = "R11";
    let mut registers = debugger.fetch_registers(pid).unwrap();
    registers.set(scratch, 0x1122_3344_5566_7788);
    debugger
        .set_registers(pid, &registers)
        .expect("set registers");
    assert_eq!(
        debugger.fetch_registers(pid).unwrap().get(scratch),
        Some(0x1122_3344_5566_7788)
    );

    // The program sees the byte it was given: it returns `cell`.
    debugger.continue_execution().unwrap();
    match next_event(&mut debugger) {
        DebugEvent::ProcessExited { exit_code } => assert_eq!(exit_code, 42),
        other => panic!("expected exit with the written value, got {other:?}"),
    }
}

#[test]
fn attach_to_a_running_process_and_let_it_go() {
    if !enabled() {
        return;
    }
    let dir =
        std::env::temp_dir().join(format!("fission-macos-live-{}-attach", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("sleeper.c");
    std::fs::write(
        &source,
        "#include <unistd.h>\nint main(void){ sleep(30); return 3; }\n",
    )
    .unwrap();
    let binary = dir.join("sleeper");
    assert!(
        std::process::Command::new("cc")
            .arg("-o")
            .arg(&binary)
            .arg(&source)
            .status()
            .unwrap()
            .success()
    );
    let mut child = std::process::Command::new(&binary).spawn().unwrap();
    let pid = child.id();

    let mut debugger = MacOSDebugger::new();
    debugger.attach(pid).expect("attach");
    assert!(debugger.is_attached());
    assert_eq!(debugger.attached_pid(), Some(pid));
    let registers = debugger
        .fetch_registers(pid)
        .expect("registers after attach");
    assert_ne!(registers.pc, 0);
    debugger.detach().expect("detach");
    assert!(!debugger.is_attached());

    // Let go means it keeps running on its own, and can be killed normally.
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert!(
        child.try_wait().unwrap().is_none(),
        "the target must still be running"
    );
    child.kill().unwrap();
    let _ = child.wait();
}
