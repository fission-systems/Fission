//! Real guest verdicts belong inside sandbox JSON, never before the document.
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Output};

fn run(sample: &str, json: bool, args: &[&str]) -> Output {
    let binary = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fission-emulator/testdata")
        .join(sample);
    let mut command = Command::new(env!("CARGO_BIN_EXE_fission_cli"));
    command
        .arg("sandbox")
        .arg(binary)
        .args(["--max-inst", "10000"]);
    if json {
        command.arg("--json");
    }
    command.args(args).output().unwrap()
}
fn bytes(value: &Value) -> Vec<u8> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|byte| byte.as_u64().unwrap() as u8)
        .collect()
}

#[test]
fn sandbox_json_captures_linux_libc_syscall_and_windows_output() {
    for fixture in [
        "x64_dyn_puts.elf",
        "linux_x64_hello_sys.elf",
        "win_x64_write.exe",
    ] {
        let output = run(fixture, true, &[]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value =
            serde_json::from_slice(&output.stdout).expect("one complete JSON stdout document");
        assert_eq!(report["halt_requested"], true);
        let stdout = bytes(&report["guest_output"]["stdout"]);
        assert!(
            !stdout.is_empty(),
            "fixture did not exercise output: {fixture}"
        );
        assert_eq!(bytes(&report["guest_output"]["stderr"]), b"");
        let text = run(fixture, false, &[]);
        assert!(text.status.success());
        assert_eq!(text.stdout, stdout, "text console compatibility: {fixture}");
    }
}

#[test]
fn sandbox_json_retains_guard_failure_verdict_and_diagnostic() {
    let output = run("linux_hle_guard.elf", true, &["--", "!"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["exit_code"], 134);
    assert_eq!(report["halt_requested"], true);
    assert!(bytes(&report["guest_output"]["stdout"]).is_empty());
    let diagnostic = bytes(&report["guest_output"]["stderr"]);
    assert!(String::from_utf8_lossy(&diagnostic).contains("stack smashing"));
}

#[test]
fn sandbox_metrics_file_matches_protocol_output() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("metrics.json");
    let output = run(
        "x64_dyn_puts.elf",
        true,
        &["--metrics-out", path.to_str().unwrap()],
    );
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        serde_json::from_slice::<Value>(&std::fs::read(path).unwrap()).unwrap()
    );
}
