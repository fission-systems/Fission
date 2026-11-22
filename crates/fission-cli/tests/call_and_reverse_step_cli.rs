//! `call` runs one function in the emulator with chosen arguments, and a
//! recording emulator session can step back and seek. Both run only
//! checked-in fixtures.
use serde_json::Value;
use std::process::{Command, Output};

const GROUND_TRUTH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../fission-dir/testdata/ground_truth_fixture.elf"
);
const CONCOLIC: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../fission-emulator/testdata/x64_concolic_branch_sys.elf"
);

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(args)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .expect("run fission_cli")
}

fn json(args: &[&str]) -> Value {
    let out = run(args);
    assert!(out.status.success(), "{out:?}");
    serde_json::from_slice(&out.stdout).expect("json")
}

/// `widen(c)` is `c * 3`, `pick(b, x)` is `b ? x : -x` (see
/// `ground_truth_fixture.c`); each answer depends on the argument given.
#[test]
fn call_returns_what_the_function_computes_from_its_arguments() {
    let call = |name: &str, args: &[&str]| {
        let mut argv = vec!["call", GROUND_TRUTH, "--name", name, "--json"];
        for a in args {
            argv.extend(["--arg", a]);
        }
        json(&argv)
    };
    let five = call("widen", &["5"]);
    assert_eq!(five["outcome"], "returned");
    assert_eq!(five["return_value"], "0xf");
    assert_eq!(call("widen", &["-2"])["return_value"], "0xfffffffa");
    assert_eq!(call("pick", &["1", "7"])["return_value"], "0x7");
    assert_eq!(call("pick", &["0", "7"])["return_value"], "0xfffffff9");
}

/// `reload_after_call` passes `g_value` (5) to `note` and returns
/// `5 * 3 + g_value`: the stubbed call is recorded with its argument.
#[test]
fn stubbed_calls_are_recorded_instead_of_run() {
    let body = json(&[
        "call",
        GROUND_TRUTH,
        "--name",
        "reload_after_call",
        "--stub-calls",
        "0",
        "--json",
    ]);
    assert_eq!(body["outcome"], "returned", "{body}");
    assert_eq!(body["return_value"], "0x14", "{body}");
    let calls = body["calls"].as_array().expect("calls");
    assert_eq!(calls.len(), 1, "{body}");
    assert_eq!(calls[0]["args"][0], "0x5", "{body}");

    let plain = json(&[
        "call",
        GROUND_TRUTH,
        "--name",
        "reload_after_call",
        "--json",
    ]);
    assert!(plain.get("calls").is_none(), "{plain}");
}

#[test]
fn an_unknown_function_name_is_an_error() {
    let out = run(&["call", GROUND_TRUTH, "--name", "no_such_function"]);
    assert!(!out.status.success());
}

fn session(extra: &[&str]) -> Value {
    let mut argv = vec![
        "debug",
        "--emulator",
        "session",
        CONCOLIC,
        "--json",
        "--keep-going",
    ];
    argv.extend_from_slice(extra);
    let out = run(&argv);
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {out:?}"))
}

/// The fixture's fourth instruction is `mov rsi, rsp`. Stepping back undoes
/// it, and seeking forward redoes it.
#[test]
fn a_recording_session_steps_back_and_seeks() {
    let body = session(&[
        "--record",
        "--record-interval",
        "1",
        "-c",
        "step",
        "-c",
        "step",
        "-c",
        "step",
        "-c",
        "step",
        "-c",
        "regs",
        "-c",
        "back-step",
        "-c",
        "regs",
        "-c",
        "seek 4",
        "-c",
        "regs",
    ]);
    let results = body["results"].as_array().expect("results");
    let regs = |i: usize| &results[i]["registers"];
    assert_eq!(results[3]["step"], 4, "{body}");
    assert_eq!(regs(4)["rsi"], regs(4)["rsp"], "{body}");
    assert_eq!(results[5]["action"], "back_step", "{body}");
    assert_eq!(results[5]["step"], 3, "{body}");
    assert_ne!(regs(6)["rsi"], regs(6)["rsp"], "{body}");
    assert_eq!(regs(6)["pc"], results[2]["pc"], "{body}");
    assert_eq!(results[7]["step"], 4, "{body}");
    assert_eq!(regs(8)["rsi"], regs(8)["rsp"], "{body}");
}

/// Without `--record` there is no history, and the session says so.
#[test]
fn back_step_needs_a_recording() {
    let body = session(&["-c", "step", "-c", "back-step"]);
    let error = body["results"][1]["error"].as_str().unwrap_or_default();
    assert!(error.contains("--record"), "{body}");

    let one_shot = run(&["debug", "--emulator", "back-step"]);
    assert!(!one_shot.status.success());
}

/// A hand-made change after going back makes the recorded future stale.
/// `continue` records the whole run; going back to step 3 and changing RSP
/// there, a seek forward to step 8 has to execute the changed machine --
/// `mov rsi, rsp` at step 4 copies the new RSP -- where replaying the
/// recording lands on the old RSP and RSI.
#[test]
fn seeking_forward_after_set_reg_executes_the_changed_machine() {
    let body = session(&[
        "--record",
        "--record-interval",
        "1",
        "-c",
        "continue",
        "-c",
        "seek 3",
        "-c",
        "set-reg rsp 0x7ffffffe0000",
        "-c",
        "seek 8",
        "-c",
        "regs",
    ]);
    let results = body["results"].as_array().expect("results");
    assert_eq!(results[3]["step"], 8, "{body}");
    let regs = &results[4]["registers"];
    assert_eq!(regs["rsp"], "0x7ffffffe0000", "{body}");
    assert_eq!(regs["rsi"], "0x7ffffffe0000", "{body}");
}

#[test]
fn a_zero_record_interval_is_refused() {
    let out = run(&[
        "debug",
        "--emulator",
        "session",
        CONCOLIC,
        "--record",
        "--record-interval",
        "0",
        "-c",
        "step",
    ]);
    assert!(!out.status.success());
}
