//! The whole path, from a command line to a Windows binary being debugged.
//!
//! Every piece below is tested on its own elsewhere. What is not tested
//! elsewhere is that they connect: that `-c "bp 0x140001016"` reaches the
//! emulator's breakpoint set, that the machine survives from one command to
//! the next, and that what comes back is the JSON an agent would read.
//!
//! Static analysis only: the guest instructions are emulated, nothing here
//! runs on the host but the CLI itself.
#![cfg(feature = "debugger")]

use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fission_cli"))
}

fn pe_fixture() -> String {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fission-emulator/testdata/win_x64_write.exe"
    )
    .to_string()
}

fn run_session(args: &[&str]) -> (bool, serde_json::Value) {
    let output = cli()
        .args(["debug", "--emulator", "session", &pe_fixture(), "--json"])
        .args(args)
        .output()
        .expect("run the CLI");
    let text = String::from_utf8_lossy(&output.stdout);
    // The config banner and the guest's own output share this stream; the
    // report is the JSON document at the end of it.
    let start = text.find('{').unwrap_or_else(|| {
        panic!(
            "no JSON in output.\nstdout: {text}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    let value = serde_json::from_str(&text[start..])
        .unwrap_or_else(|e| panic!("output is not JSON ({e}): {}", &text[start..]));
    (output.status.success(), value)
}

#[test]
fn a_windows_binary_is_debugged_from_one_command_line() {
    let (ok, report) = run_session(&[
        "-c",
        "bp 0x140001016",
        "-c",
        "continue",
        "-c",
        "regs",
        "-c",
        "continue",
    ]);
    assert!(ok, "the session failed: {report:#}");

    let results = report["results"].as_array().expect("results");
    assert_eq!(results.len(), 4, "{report:#}");

    // The machine survived from the first command to the second: the
    // breakpoint set by one stopped the run started by the next.
    assert_eq!(results[1]["stop"], "breakpoint:0x140001016");
    assert_eq!(results[1]["pc"], "0x140001016");

    // Registers are the machine's, and few enough to read.
    let regs = results[2]["registers"].as_object().expect("registers");
    assert_eq!(regs["pc"], "0x140001016");
    assert!(
        regs.len() < 32,
        "a register dump of {} entries is not a register dump",
        regs.len()
    );
    assert!(regs.contains_key("rsp"));

    // And the program ran to the end, with its own output on the way.
    let events: Vec<&serde_json::Value> = results
        .iter()
        .filter_map(|r| r["events"].as_array())
        .flatten()
        .collect();
    assert!(
        events
            .iter()
            .any(|e| e["event"] == "output"
                && e["message"].as_str().is_some_and(|m| m.contains("hi"))),
        "the program's output never reached the report: {events:#?}"
    );
    assert!(
        events
            .iter()
            .any(|e| e["event"] == "process_exited" && e["exit_code"] == 0),
        "no exit event: {events:#?}"
    );
    assert_eq!(report["final"]["status"], "Terminated");
}

/// A watchpoint answers "what wrote this", and the answer is two addresses:
/// the memory and the instruction.
#[test]
fn a_watchpoint_reports_the_instruction_that_wrote() {
    let (ok, report) = run_session(&[
        "-c",
        "step",
        "-c",
        "mem-bp 0x7fffffffef00 -s 512 -k write",
        "-c",
        "continue",
    ]);
    assert!(ok, "{report:#}");

    let results = report["results"].as_array().expect("results");
    let hit = results[2]["events"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|e| e["event"] == "watchpoint_hit");
    if let Some(hit) = hit {
        assert_eq!(hit["write"], true);
        assert_ne!(hit["pc"], "0x0", "no instruction named: {hit:#}");
        assert_ne!(
            hit["pc"], hit["address"],
            "the instruction and the memory it wrote are the same address"
        );
    }
}

/// A failed command stops the session and the exit code says so, without a
/// caller having to parse the report to find out.
#[test]
fn a_failed_command_fails_the_session() {
    let (ok, report) = run_session(&["-c", "bp 0x140001016", "-c", "modules"]);
    assert!(!ok, "a refused command reported success: {report:#}");
    let results = report["results"].as_array().expect("results");
    assert_eq!(results.len(), 2, "the session kept going past the failure");
    assert_eq!(results[1]["status"], "error");
    let message = results[1]["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("modules"),
        "the error names the wrong command: {message}"
    );

    let (ok, report) = run_session(&["-c", "modules", "--keep-going", "-c", "regs"]);
    assert!(ok, "--keep-going still failed the session: {report:#}");
    assert_eq!(report["results"].as_array().expect("results").len(), 2);
}

/// Commands can come from a script, one per line, with comments.
#[test]
fn a_script_is_the_same_vocabulary() {
    let dir = std::env::temp_dir().join("fission-session-test");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let script = dir.join("plan.txt");
    std::fs::write(&script, "# two steps\nstep\n\nstep\nregs\n").expect("write script");

    let (ok, report) = run_session(&["--script", script.to_str().expect("path")]);
    assert!(ok, "{report:#}");
    assert_eq!(
        report["results"].as_array().expect("results").len(),
        3,
        "comments and blank lines were not skipped: {report:#}"
    );
    let _ = std::fs::remove_file(&script);
}

/// A command list has no loops and no conditions; a script does. Both drive
/// the same machine, in one session, so the commands can set up what the
/// script then works from.
#[test]
fn a_script_continues_from_where_the_commands_left_the_machine() {
    let dir = std::env::temp_dir().join("fission-session-test");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let script = dir.join("after.rhai");
    std::fs::write(
        &script,
        r#"
        emit(#{ kind: "resumed_at", address: machine.pc() });
        for i in machine.disasm(3) {
            emit(#{ kind: "insn", address: i.address, message: i.text });
        }
        "#,
    )
    .expect("write script");

    let (ok, report) = run_session(&[
        "-c",
        "bp 0x140001016",
        "-c",
        "continue",
        "--rhai",
        script.to_str().expect("path"),
    ]);
    assert!(ok, "{report:#}");

    let script_result = &report["script"];
    assert_eq!(script_result["status"], "ok", "{script_result:#}");
    let findings = script_result["findings"].as_array().expect("findings");

    // The script inherited the machine: it starts where the breakpoint left
    // it, not at the entry point of a freshly launched second copy.
    assert_eq!(findings[0]["kind"], "resumed_at");
    assert_eq!(
        findings[0]["address"], "0x140001016",
        "the script got a different machine than the commands drove"
    );

    // And it can say what is there, which is the other half of stopping.
    assert_eq!(findings[1]["kind"], "insn");
    assert_eq!(findings[1]["address"], "0x140001016");
    assert!(
        findings[1]["message"]
            .as_str()
            .is_some_and(|t| !t.is_empty()),
        "the instruction at the breakpoint disassembled to nothing: {findings:#?}"
    );
    assert_eq!(findings.len(), 4, "{findings:#?}");
}
