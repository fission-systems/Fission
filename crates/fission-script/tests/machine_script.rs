//! A script driving a live machine.
//!
//! The read-only half of this crate is a longer way to spell
//! `fission functions --json | jq`. This half has no outside equivalent: an
//! emulated process exists only inside the command that launched it, so
//! anything built from more than one step -- run until a buffer is written
//! and read it, step until the program counter leaves a range, break on every
//! import and see which are reached -- cannot be assembled from separate
//! invocations at any price.
//!
//! Static fixtures only; the guest instructions are emulated.
#![cfg(feature = "emulator")]

use fission_loader::loader::LoadedBinary;
use fission_script::{ScriptLimits, ScriptOptions, ScriptRunStatus, run_script_with};

fn fixture() -> LoadedBinary {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fission-emulator/testdata/win_x64_write.exe"
    );
    LoadedBinary::from_file(path).expect("load the fixture")
}

fn run(source: &str) -> fission_script::ScriptRunResult {
    let limits = ScriptLimits {
        max_runtime_ms: 30_000,
        max_operations: 100_000_000,
        ..ScriptLimits::default()
    };
    run_script_with(
        &fixture(),
        source,
        "test.rhai",
        limits,
        ScriptOptions { machine: true },
    )
}

#[test]
fn a_script_can_step_and_read_the_machine() {
    let result = run(r#"
        let start = machine.pc();
        machine.step();
        machine.step();
        emit(#{ kind: "moved", address: machine.pc(), same: machine.pc() == start });
        "#);
    assert_eq!(
        result.status,
        ScriptRunStatus::Ok,
        "{:?}",
        result.diagnostics
    );
    let finding = &result.findings[0];
    assert_eq!(finding.kind, "moved");
    assert!(
        finding
            .address
            .as_deref()
            .is_some_and(|a| a.starts_with("0x")),
        "an address emitted as an integer was dropped or left in decimal: {finding:?}"
    );
    assert_eq!(
        finding.data.as_ref().unwrap()["same"],
        serde_json::json!(false)
    );
}

/// The thing separate invocations cannot do: a loop over a running program.
#[test]
fn a_script_can_watch_memory_and_name_the_instruction() {
    let result = run(r#"
        let sp = machine.reg("RSP");
        machine.watch(sp - 0x200, 0x200, "write");
        let seen = 0;
        while seen < 3 && !machine.finished() {
            let why = machine.resume();
            if !why.starts_with("watchpoint") { break; }
            for e in machine.events() {
                if e.event == "watchpoint_hit" {
                    emit(#{ kind: "write", address: e.address, by: hex(e.pc) });
                    seen += 1;
                }
            }
        }
        "#);
    assert_eq!(
        result.status,
        ScriptRunStatus::Ok,
        "{:?}",
        result.diagnostics
    );
    assert_eq!(result.findings.len(), 3, "{:?}", result.findings);
    for finding in &result.findings {
        let address = finding.address.as_deref().expect("the watched address");
        let by = finding.data.as_ref().expect("data")["by"]
            .as_str()
            .expect("the instruction");
        assert!(address.starts_with("0x") && by.starts_with("0x"));
        assert_ne!(
            address, by,
            "the memory written and the instruction that wrote it are the same address"
        );
    }
}

/// A program's own output, read back inside the script that ran it.
#[test]
fn a_script_sees_what_the_program_printed() {
    let result = run(r#"
        while !machine.finished() {
            machine.resume();
        }
        emit(#{ kind: "output", message: machine.output() });
        "#);
    assert_eq!(
        result.status,
        ScriptRunStatus::Ok,
        "{:?}",
        result.diagnostics
    );
    let message = result.findings[0].message.as_deref().unwrap_or_default();
    assert!(
        message.contains("hi"),
        "the program's output never reached the script: {message:?}"
    );
}

/// A machine is not handed to a script that did not ask for one: launching
/// costs a real program load, and reading a binary's inventory should not pay
/// for an emulator it never touches.
#[test]
fn no_machine_unless_asked() {
    let result = run_script_with(
        &fixture(),
        "emit(#{ kind: \"x\", address: machine.pc() });",
        "test.rhai",
        ScriptLimits::default(),
        ScriptOptions::default(),
    );
    assert_eq!(result.status, ScriptRunStatus::Error);
    assert!(
        result.diagnostics[0].message.contains("machine"),
        "unhelpful: {:?}",
        result.diagnostics
    );
}

/// Every key a script emits is kept. This used to read five names and drop
/// the rest without a word.
#[test]
fn emit_keeps_what_it_was_given() {
    let result = run(r#"emit(#{ kind: "k", functions: 12, note: "x" });"#);
    assert_eq!(
        result.status,
        ScriptRunStatus::Ok,
        "{:?}",
        result.diagnostics
    );
    let data = result.findings[0].data.as_ref().expect("extra keys kept");
    assert_eq!(data["functions"], serde_json::json!(12));
    assert_eq!(data["note"], serde_json::json!("x"));
}
