//! One-shot commands can only reopen sessions backed by a live native process.
#![cfg(feature = "debugger")]

use std::process::Command;

#[test]
fn emulator_session_commands_explain_the_process_boundary() {
    for args in [
        &["regs"][..],
        &["read", "0x401000", "--size", "4"],
        &["threads"],
        &["bp-list"],
        &["stack-peek"],
        &["stack-pop"],
        &["stack-push", "0x10"],
        &["detach"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
            .args(["debug", "--emulator"])
            .args(args)
            .output()
            .unwrap();
        assert!(!output.status.success(), "{args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Emulated debug sessions exist only in the process that launched them"),
            "{args:?}: {stderr}"
        );
    }
}

#[cfg(unix)]
#[test]
fn without_a_native_session_the_commands_say_so() {
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(["debug", "threads"])
        .env("TMPDIR", dir.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("No active debug session"), "{stderr}");
}
