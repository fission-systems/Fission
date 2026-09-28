#![cfg(all(target_os = "linux", feature = "interactive_runtime"))]

use fission_dynamic::debug::platform::linux::LinuxDebugger;
use fission_dynamic::debug::{DebugEvent, DebugStatus, ExecutionBackend};

#[test]
fn launch_stops_at_exec_reports_creation_and_runs_with_arguments() {
    let mut debugger = LinuxDebugger::new();
    let args = vec!["-c".to_string(), "exit 23".to_string()];

    let pid = debugger
        .launch("/bin/sh", &args)
        .expect("launch child under ptrace");

    assert_eq!(debugger.attached_pid(), Some(pid));
    assert_eq!(debugger.state().attached_pid, Some(pid));
    assert_eq!(debugger.state().main_thread_id, Some(pid));
    assert_eq!(debugger.state().current_thread_id, Some(pid));
    assert_eq!(debugger.state().status, DebugStatus::Suspended);
    assert!(debugger.fetch_registers(pid).expect("initial registers").pc > 0);

    assert!(matches!(
        debugger.poll_event(0).expect("initial process event"),
        Some(DebugEvent::ProcessCreated {
            pid: event_pid,
            main_thread_id,
        }) if event_pid == pid && main_thread_id == pid
    ));

    debugger.continue_execution().expect("continue child");
    assert!(matches!(
        debugger.poll_event(5_000).expect("child exit event"),
        Some(DebugEvent::ProcessExited { exit_code: 23 })
    ));
    assert_eq!(debugger.state().status, DebugStatus::Terminated);
    assert_eq!(debugger.attached_pid(), None);
    assert_eq!(debugger.state().attached_pid, None);
}

#[test]
fn failed_launch_does_not_attach_a_nonexistent_child() {
    let mut debugger = LinuxDebugger::new();

    let error = debugger
        .launch("/definitely/not/a/fission-launch-target", &[])
        .expect_err("missing executable must fail");

    assert!(error.to_string().contains("launch"), "{error}");
    assert_eq!(debugger.attached_pid(), None);
    assert_eq!(debugger.state().status, DebugStatus::Detached);
}
