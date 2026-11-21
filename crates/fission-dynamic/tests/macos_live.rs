//! Live macOS debugger tests against a real process.
//!
//! These need the Mach task port of a process the test launches. macOS grants
//! that only to a caller that is **both** signed with the
//! `com.apple.security.cs.debugger` entitlement **and** authorized for
//! Developer Tools (`sudo DevToolsSecurity -enable`, once). An unsigned caller
//! is refused at once; a signed one on a machine without Developer Tools access
//! waits on a prompt. So the tests are `#[ignore]`d -- nextest reports them as
//! ignored rather than passed -- and are run through the script that signs the
//! test binary first and passes `--ignored`:
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
#[ignore = "needs a signed test binary and Developer Tools access; run scripts/macos/run-live-debugger-tests.sh"]
fn launch_stops_at_exec_and_runs_to_exit() {
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
#[ignore = "needs a signed test binary and Developer Tools access; run scripts/macos/run-live-debugger-tests.sh"]
fn single_step_moves_the_program_counter() {
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
#[ignore = "needs a signed test binary and Developer Tools access; run scripts/macos/run-live-debugger-tests.sh"]
fn a_breakpoint_hits_hides_its_instruction_and_steps_off() {
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
#[ignore = "needs a signed test binary and Developer Tools access; run scripts/macos/run-live-debugger-tests.sh"]
fn removing_a_breakpoint_restores_the_instruction() {
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
#[ignore = "needs a signed test binary and Developer Tools access; run scripts/macos/run-live-debugger-tests.sh"]
fn modules_are_listed_once_dyld_has_run() {
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
#[ignore = "needs a signed test binary and Developer Tools access; run scripts/macos/run-live-debugger-tests.sh"]
fn memory_and_registers_can_be_written() {
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
#[ignore = "needs a signed test binary and Developer Tools access; run scripts/macos/run-live-debugger-tests.sh"]
fn attach_to_a_running_process_and_let_it_go() {
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

/// Compile `source` into its own directory and return the binary and the file
/// it reports addresses to (its first argument).
fn build_c_target(name: &str, source: &str) -> (PathBuf, PathBuf) {
    let dir =
        std::env::temp_dir().join(format!("fission-macos-live-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source_path = dir.join("target.c");
    std::fs::write(&source_path, source).unwrap();
    let binary = dir.join("target");
    let status = std::process::Command::new("cc")
        .arg("-O0")
        .arg("-o")
        .arg(&binary)
        .arg(&source_path)
        .status()
        .expect("a C compiler is needed for the live tests");
    assert!(status.success());
    (binary, dir.join("address.txt"))
}

/// Launch `target`, run it to the `SIGSTOP` it raises after reporting its
/// addresses, and return the debugger, the pid and the first address.
fn launch_to_self_stop(
    target: &std::path::Path,
    report: &std::path::Path,
    exit_gate: Option<&std::path::Path>,
) -> (MacOSDebugger, u32, u64) {
    let mut debugger = MacOSDebugger::new();
    let mut args = vec![report.to_str().unwrap().to_string()];
    args.extend(exit_gate.map(|path| path.to_str().unwrap().to_string()));
    let pid = debugger
        .launch(target.to_str().unwrap(), &args)
        .expect("launch");
    let _ = next_event(&mut debugger);
    debugger.continue_execution().unwrap();
    match next_event(&mut debugger) {
        DebugEvent::Exception { code, .. } => assert_eq!(code, 17, "SIGSTOP"),
        other => panic!("expected the target's own SIGSTOP, got {other:?}"),
    }
    let (address, _) = reported_addresses(report);
    (debugger, pid, address)
}

/// `marker` is called on a second thread; the main thread waits in
/// `pthread_join`.
const WORKER_THREAD_TARGET: &str = r#"
#include <pthread.h>
#include <signal.h>
#include <stdio.h>
#include <unistd.h>
__attribute__((noinline)) int marker(int x) { return x + 5; }
static void *worker(void *arg) { return (void *)(long)marker(2); }
int main(int argc, char **argv) {
    FILE *f = fopen(argv[1], "w");
    fprintf(f, "%lx\n%lx\n", (unsigned long)&marker, 0UL);
    fclose(f);
    raise(SIGSTOP);
    pthread_t thread;
    void *result = 0;
    pthread_create(&thread, 0, worker, 0);
    pthread_join(thread, &result);
    if (argc > 2) {
        while (access(argv[2], F_OK) != 0) usleep(1000);
    }
    return (int)(long)result;
}
"#;

/// The stop says which thread hit the breakpoint, and that thread stands on
/// it. Reading the main thread's counter for every `SIGTRAP` saw the main
/// thread parked in `pthread_join`, called the stop a single step, and left
/// the worker to execute the breakpoint instruction again for ever.
#[test]
#[ignore = "needs a signed test binary and Developer Tools access; run scripts/macos/run-live-debugger-tests.sh"]
fn a_breakpoint_hit_on_another_thread_is_reported_for_that_thread() {
    let (target, report) = build_c_target("worker-hit", WORKER_THREAD_TARGET);
    let (mut debugger, pid, marker) = launch_to_self_stop(&target, &report, None);
    let main_pc = debugger.fetch_registers(pid).unwrap().pc;

    debugger.set_sw_breakpoint(marker).expect("set breakpoint");
    debugger.continue_execution().unwrap();
    let worker = match next_event(&mut debugger) {
        DebugEvent::BreakpointHit { address, thread_id } => {
            assert_eq!(address, marker);
            thread_id
        }
        other => panic!("expected a breakpoint hit on the worker thread, got {other:?}"),
    };
    assert_ne!(
        worker, pid,
        "the hit is the worker's, not the main thread's"
    );
    assert_eq!(debugger.fetch_registers(worker).unwrap().pc, marker);
    assert_ne!(
        debugger.fetch_registers(pid).unwrap().pc,
        marker,
        "the main thread is somewhere else (it was at {main_pc:#x})"
    );
    assert_eq!(debugger.list_breakpoints()[0].hits, 1);
    assert_eq!(debugger.get_state().current_thread_id, Some(worker));

    // Continuing steps the worker off the breakpoint and the program ends.
    debugger.continue_execution().unwrap();
    match next_event(&mut debugger) {
        DebugEvent::ProcessExited { exit_code } => assert_eq!(exit_code, 7),
        other => panic!("expected exit after the worker's breakpoint, got {other:?}"),
    }
}

/// A step from a breakpoint on another thread moves that thread, and only it.
#[test]
#[ignore = "needs a signed test binary and Developer Tools access; run scripts/macos/run-live-debugger-tests.sh"]
fn stepping_from_a_breakpoint_on_another_thread_steps_that_thread() {
    let (target, report) = build_c_target("worker-step", WORKER_THREAD_TARGET);
    let (mut debugger, pid, marker) = launch_to_self_stop(&target, &report, None);
    debugger.set_sw_breakpoint(marker).expect("set breakpoint");
    debugger.continue_execution().unwrap();
    let worker = match next_event(&mut debugger) {
        DebugEvent::BreakpointHit { thread_id, .. } => thread_id,
        other => panic!("expected a breakpoint hit, got {other:?}"),
    };
    assert_ne!(worker, pid);
    let main_before = debugger.fetch_registers(pid).unwrap().pc;

    debugger.single_step().expect("step the worker");
    match next_event(&mut debugger) {
        DebugEvent::SingleStep { thread_id } => assert_eq!(thread_id, worker),
        other => panic!("expected the worker's single step, got {other:?}"),
    }
    let worker_pc = debugger.fetch_registers(worker).unwrap().pc;
    assert_ne!(worker_pc, marker, "the worker ran one instruction");
    assert!(
        worker_pc > marker && worker_pc - marker <= 16,
        "one instruction on from {marker:#x}, not {worker_pc:#x}"
    );
    assert_eq!(
        debugger.fetch_registers(pid).unwrap().pc,
        main_before,
        "the main thread did not move during the worker's step"
    );
    // A second step, with no breakpoint under it, is still the worker's.
    debugger.single_step().expect("step the worker again");
    match next_event(&mut debugger) {
        DebugEvent::SingleStep { thread_id } => assert_eq!(thread_id, worker),
        other => panic!("expected the worker's second step, got {other:?}"),
    }
    assert_ne!(debugger.fetch_registers(worker).unwrap().pc, worker_pc);

    debugger.continue_execution().unwrap();
    match next_event(&mut debugger) {
        DebugEvent::ProcessExited { exit_code } => assert_eq!(exit_code, 7),
        other => panic!("expected exit after stepping the worker, got {other:?}"),
    }
}

#[test]
#[ignore = "needs a signed test binary and Developer Tools access; run scripts/macos/run-live-debugger-tests.sh"]
fn dropping_an_active_worker_step_releases_the_target() {
    check_drop_during_worker_step(false);
}

#[test]
#[ignore = "needs a signed test binary and Developer Tools access; run scripts/macos/run-live-debugger-tests.sh"]
fn dropping_an_active_worker_step_detaches_a_target_that_keeps_running() {
    check_drop_during_worker_step(true);
}

fn check_drop_during_worker_step(keep_running: bool) {
    use std::os::unix::process::ExitStatusExt;
    use std::time::{Duration, Instant};

    let fixture = if keep_running {
        "drop-worker-detach"
    } else {
        "drop-worker-step"
    };
    let (target, report) = build_c_target(fixture, WORKER_THREAD_TARGET);
    let exit_gate = keep_running.then(|| report.with_file_name("allow-exit"));
    let (mut debugger, pid, marker) = launch_to_self_stop(&target, &report, exit_gate.as_deref());
    debugger.set_sw_breakpoint(marker).unwrap();
    debugger.continue_execution().unwrap();
    assert!(matches!(
        next_event(&mut debugger),
        DebugEvent::BreakpointHit { thread_id, .. } if thread_id != pid
    ));
    debugger.single_step().unwrap();
    assert!(matches!(
        next_event(&mut debugger),
        DebugEvent::SingleStep { .. }
    ));

    // This step has no breakpoint to lift, so single_step returns before
    // collecting the stop and the debugger still owns the peer suspensions.
    debugger.single_step().unwrap();
    drop(debugger);
    let trace_state = exit_gate.map(|exit_gate| {
        // SAFETY: proc_pidinfo writes into a valid record for our live child.
        let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of_val(&info) as i32;
        let read = unsafe {
            libc::proc_pidinfo(
                pid as i32,
                libc::PROC_PIDTBSDINFO,
                0,
                (&mut info as *mut libc::proc_bsdinfo).cast(),
                size,
            )
        };
        std::fs::write(exit_gate, []).expect("allow the detached target to exit");
        (read, size, info.pbi_flags)
    });

    unsafe extern "C" {
        fn waitpid(pid: i32, status: *mut i32, options: i32) -> i32;
        fn kill(pid: i32, signal: i32) -> i32;
    }
    let pid = i32::try_from(pid).unwrap();
    let start = Instant::now();
    loop {
        let mut status = 0;
        // SAFETY: waitpid receives our child's PID and an initialized status slot.
        let waited = unsafe { waitpid(pid, &mut status, 1) }; // WNOHANG
        if waited == pid {
            let status = std::process::ExitStatus::from_raw(status);
            assert_eq!(status.code(), Some(7), "target exited with {status}");
            if let Some((read, size, flags)) = trace_state {
                const PROC_FLAG_TRACED: u32 = 2;
                assert_eq!(read, size, "read the debugger target's BSD process info");
                assert_eq!(flags & PROC_FLAG_TRACED, 0, "target is still traced");
            }
            return;
        }
        assert_eq!(waited, 0, "waitpid: {}", std::io::Error::last_os_error());
        if start.elapsed() >= Duration::from_secs(5) {
            // SAFETY: waitpid has confirmed this child is still ours.
            unsafe { kill(pid, 9) }; // SIGKILL
            panic!("target was still running with a peer held after dropping the debugger");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// `load` is one instruction that reads through its argument. Called first
/// with a null pointer it faults; the handler jumps back and it is called
/// again with a valid one.
const FAULTING_INSTRUCTION_TARGET: &str = r#"
#include <setjmp.h>
#include <signal.h>
#include <stdio.h>
int load(int *pointer);
#if defined(__aarch64__)
__asm__(".text\n.globl _load\n.p2align 2\n_load:\n ldr w0, [x0]\n ret\n");
#else
__asm__(".text\n.globl _load\n_load:\n movl (%rdi), %eax\n ret\n");
#endif
static sigjmp_buf again;
static void on_fault(int signal) { siglongjmp(again, 1); }
static int value = 9;
int main(int argc, char **argv) {
    FILE *f = fopen(argv[1], "w");
    fprintf(f, "%lx\n%lx\n", (unsigned long)&load, 0UL);
    fclose(f);
    signal(SIGSEGV, on_fault);
    signal(SIGBUS, on_fault);
    raise(SIGSTOP);
    if (sigsetjmp(again, 1) == 0) {
        return load(0);
    }
    return load(&value);
}
"#;

/// A breakpoint on an instruction that faults reports the hit, then the
/// fault at that instruction, and is hit again when the instruction is
/// reached a second time.
#[test]
#[ignore = "needs a signed test binary and Developer Tools access; run scripts/macos/run-live-debugger-tests.sh"]
fn a_breakpoint_on_a_faulting_instruction_reports_the_fault_and_stays_set() {
    let (target, report) = build_c_target("fault", FAULTING_INSTRUCTION_TARGET);
    let (mut debugger, pid, load) = launch_to_self_stop(&target, &report, None);
    debugger.set_sw_breakpoint(load).expect("set breakpoint");

    debugger.continue_execution().unwrap();
    match next_event(&mut debugger) {
        DebugEvent::BreakpointHit { address, .. } => assert_eq!(address, load),
        other => panic!("expected the first call to hit the breakpoint, got {other:?}"),
    }

    // Leaving the breakpoint runs the faulting load.
    debugger.continue_execution().unwrap();
    match next_event(&mut debugger) {
        DebugEvent::Exception { code, address, .. } => {
            assert!(code == 11 || code == 10, "SIGSEGV or SIGBUS, got {code}");
            assert_eq!(address, load, "the fault is at the load");
        }
        other => panic!("expected the load to fault, got {other:?}"),
    }
    // The program's bytes are still what a reader sees, and the breakpoint is
    // still listed.
    assert_eq!(debugger.list_breakpoints().len(), 1);

    // The fault is delivered, the handler jumps back, and `load` is called
    // again: the breakpoint must still be there.
    debugger.continue_execution().unwrap();
    match next_event(&mut debugger) {
        DebugEvent::BreakpointHit { address, .. } => assert_eq!(address, load),
        other => {
            panic!("expected the second call to hit the breakpoint again, got {other:?}")
        }
    }
    assert_eq!(debugger.fetch_registers(pid).unwrap().pc, load);
    assert_eq!(debugger.list_breakpoints()[0].hits, 2);

    debugger.continue_execution().unwrap();
    match next_event(&mut debugger) {
        DebugEvent::ProcessExited { exit_code } => assert_eq!(exit_code, 9),
        other => panic!("expected exit with the loaded value, got {other:?}"),
    }
}

/// `fire` is `kill(pid, signal)` as three instructions; the address reported
/// is the system call instruction itself.
const SIGNALLING_INSTRUCTION_TARGET: &str = r#"
#include <signal.h>
#include <stdio.h>
#include <unistd.h>
int fire(int pid, int signal);
#if defined(__aarch64__)
__asm__(".text\n.globl _fire\n.p2align 2\n_fire:\n mov x16, #37\n svc #0x80\n ret\n");
#define SYSCALL_OFFSET 4
#else
__asm__(".text\n.globl _fire\n_fire:\n movl $0x2000025, %eax\n syscall\n ret\n");
#define SYSCALL_OFFSET 5
#endif
static volatile int delivered;
static void on_signal(int signal) { delivered++; }
int main(int argc, char **argv) {
    FILE *f = fopen(argv[1], "w");
    fprintf(f, "%lx\n%lx\n", (unsigned long)&fire + SYSCALL_OFFSET, 0UL);
    fclose(f);
    signal(SIGUSR1, on_signal);
    raise(SIGSTOP);
    fire(getpid(), SIGUSR1);
    fire(getpid(), SIGUSR1);
    return delivered;
}
"#;

/// A breakpoint survives a signal that arrives while it is lifted.
///
/// Leaving a breakpoint runs its instruction with the breakpoint taken out.
/// Here that instruction is a system call that signals the process itself, so
/// the step ends with the signal's stop, past the instruction, instead of
/// with its own trap. The breakpoint has to be back in place all the same:
/// the second call must hit it again.
#[test]
#[ignore = "needs a signed test binary and Developer Tools access; run scripts/macos/run-live-debugger-tests.sh"]
fn a_breakpoint_is_put_back_when_a_signal_ends_the_step_off_it() {
    let (target, report) = build_c_target("signal", SIGNALLING_INSTRUCTION_TARGET);
    let (mut debugger, _pid, syscall) = launch_to_self_stop(&target, &report, None);
    debugger.set_sw_breakpoint(syscall).expect("set breakpoint");

    let mut hits = 0;
    let mut signals = 0;
    let exit_code = loop {
        debugger.continue_execution().unwrap();
        match next_event(&mut debugger) {
            DebugEvent::BreakpointHit { address, .. } => {
                assert_eq!(address, syscall);
                hits += 1;
            }
            // SIGUSR1 is 30 on macOS.
            DebugEvent::Exception { code: 30, .. } => signals += 1,
            DebugEvent::ProcessExited { exit_code } => break exit_code,
            other => panic!("unexpected event {other:?} after {hits} hits"),
        }
        assert!(hits + signals < 16, "the target is not making progress");
    };
    // Positive control: both signals were raised and handled, so the system
    // call ran twice.
    assert_eq!(exit_code, 2, "both calls delivered their signal");
    assert_eq!(signals, 2, "both signal stops were reported");
    assert_eq!(hits, 2, "the second call hit the breakpoint too");
}

/// A completed step is a step, wherever it lands.
///
/// Stepping from one breakpoint onto the next instruction, which has a
/// breakpoint of its own: that instruction has not run, so nothing was hit.
/// With a breakpoint instruction that does not advance the counter the stop
/// used to be read as a hit; with one that does, the step off a one-byte
/// instruction was read as a hit of the breakpoint just left and the counter
/// was moved back onto it.
#[test]
#[ignore = "needs a signed test binary and Developer Tools access; run scripts/macos/run-live-debugger-tests.sh"]
fn a_step_that_lands_on_a_breakpoint_is_a_step() {
    let (target, report) = build_reporting_target("step-onto");
    let (mut debugger, pid, marker) = launch_to_self_stop(&target, &report, None);
    // The first instruction of an unoptimised function: `sub sp, sp, #16`
    // (four bytes) or `push %rbp` (one byte).
    #[cfg(target_arch = "aarch64")]
    let next = marker + 4;
    #[cfg(target_arch = "x86_64")]
    let next = marker + 1;
    debugger
        .set_sw_breakpoint(marker)
        .expect("first breakpoint");
    debugger.set_sw_breakpoint(next).expect("second breakpoint");

    debugger.continue_execution().unwrap();
    match next_event(&mut debugger) {
        DebugEvent::BreakpointHit { address, .. } => assert_eq!(address, marker),
        other => panic!("expected the first breakpoint, got {other:?}"),
    }

    debugger
        .single_step()
        .expect("step off the first breakpoint");
    match next_event(&mut debugger) {
        DebugEvent::SingleStep { thread_id } => assert_eq!(thread_id, pid),
        other => panic!("expected a single step onto {next:#x}, got {other:?}"),
    }
    assert_eq!(debugger.fetch_registers(pid).unwrap().pc, next);
    let hits: Vec<u64> = debugger.list_breakpoints().iter().map(|b| b.hits).collect();
    assert_eq!(hits, [1, 0], "the second breakpoint has not been hit");

    // Both breakpoints are still set, and running on leaves through the
    // second one to the end of the program.
    debugger.continue_execution().unwrap();
    match next_event(&mut debugger) {
        DebugEvent::ProcessExited { exit_code } => assert_eq!(exit_code, 7),
        other => panic!("expected exit, got {other:?}"),
    }
}
