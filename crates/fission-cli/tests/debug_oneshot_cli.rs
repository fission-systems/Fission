//! The one-shot `debug` subcommands act on the session they found.
//!
//! Each of them looks up the active session's state file and is then supposed
//! to attach a backend to that session's process before doing anything. Six of
//! them (`read`, `threads`, `bp-list`, `stack-peek`, `stack-pop`,
//! `stack-push`) looked the session up, threw the result away and asked a
//! backend that was attached to nothing.
//!
//! The emulator backend makes that visible without a process to debug: its
//! `attach` refuses with its own message ("Nothing to attach to"), which is
//! different from what an unattached backend says when asked to read memory
//! ("Emulator not running"). So a command that attaches says the first thing,
//! on every host.
#![cfg(feature = "debugger")]

use std::path::{Path, PathBuf};
use std::process::Command;

/// A private temporary directory holding one session state file.
fn session_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("fission-cli-oneshot-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("fission-debug-9999.json"),
        r#"{"pid":9999,"last_thread_id":1,"breakpoints":[4198400]}"#,
    )
    .unwrap();
    dir
}

/// Run `fission_cli debug --emulator <args>` with `dir` as the temporary
/// directory the state file is looked up in.
fn debug(dir: &Path, args: &[&str]) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(["debug", "--emulator"])
        .args(args)
        .env("TMPDIR", dir)
        .env("TMP", dir)
        .env("TEMP", dir)
        .output()
        .expect("run fission_cli");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

const ATTACH_REFUSAL: &str = "Nothing to attach to";

#[test]
fn a_command_that_always_attached_reports_the_attach_refusal() {
    // Positive control: `regs` attached before this change too. If this does
    // not see the refusal, the tests below prove nothing.
    let dir = session_dir("control");
    let (ok, stderr) = debug(&dir, &["regs"]);
    assert!(!ok);
    assert!(stderr.contains(ATTACH_REFUSAL), "regs: {stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_session_command_attaches_before_it_acts() {
    let dir = session_dir("attach");
    for args in [
        &["read", "0x401000", "--size", "4"][..],
        &["threads"],
        &["bp-list"],
        &["stack-peek"],
        &["stack-pop"],
        &["stack-push", "0x10"],
    ] {
        let (ok, stderr) = debug(&dir, args);
        assert!(
            !ok,
            "{args:?} succeeded against a session it cannot attach to"
        );
        assert!(
            stderr.contains(ATTACH_REFUSAL),
            "{args:?} did not try to attach to the session: {stderr}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_a_session_the_commands_say_so() {
    let dir = session_dir("none");
    std::fs::remove_file(dir.join("fission-debug-9999.json")).unwrap();
    // A directory with the session prefix is not a session.
    std::fs::create_dir(dir.join("fission-debug-stale")).unwrap();
    let (ok, stderr) = debug(&dir, &["threads"]);
    assert!(!ok);
    assert!(stderr.contains("No active debug session"), "{stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}
