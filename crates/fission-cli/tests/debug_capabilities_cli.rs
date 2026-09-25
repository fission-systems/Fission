#![cfg(feature = "debugger")]

use serde_json::Value;
use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(args)
        .output()
        .expect("run fission_cli")
}

fn json_report(output: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let start = stdout
        .find('{')
        .unwrap_or_else(|| panic!("no JSON report in stdout: {stdout}"));
    serde_json::from_str(&stdout[start..])
        .unwrap_or_else(|error| panic!("invalid JSON report ({error}): {}", &stdout[start..]))
}

fn operation<'a>(report: &'a Value, name: &str) -> &'a Value {
    report["operations"]
        .as_array()
        .expect("operation list")
        .iter()
        .find(|operation| operation["operation"] == name)
        .unwrap_or_else(|| panic!("missing operation {name} in report: {report:#}"))
}

#[test]
fn native_capability_query_is_structured_and_does_not_need_a_target() {
    let output = cli(&["debug", "capabilities", "--json"]);
    assert!(
        output.status.success(),
        "capability query failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = json_report(&output);

    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["host"]["os"], std::env::consts::OS);
    assert_eq!(report["host"]["architecture"], std::env::consts::ARCH);
    assert_eq!(
        report["operations"].as_array().unwrap().len(),
        fission_dynamic::debug::DebugOperation::ALL.len()
    );
    assert_eq!(
        operation(&report, "attach")["support"]["status"],
        if cfg!(target_os = "macos") {
            "unsupported"
        } else if cfg!(target_os = "windows") && !cfg!(feature = "windows_native_debugger") {
            "unsupported"
        } else if cfg!(target_os = "linux") {
            "conditional"
        } else {
            "conditional"
        }
    );
}

#[test]
fn emulator_capability_query_and_text_mode_are_available_without_launching() {
    let output = cli(&["debug", "--emulator", "capabilities", "--json"]);
    assert!(
        output.status.success(),
        "emulator capability query failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = json_report(&output);
    assert_eq!(report["backend"], "emulator");
    assert_eq!(report["availability"]["status"], "available");
    assert_eq!(
        operation(&report, "launch")["support"]["status"],
        "conditional"
    );
    assert_eq!(
        operation(&report, "process_enumeration")["support"]["status"],
        "unsupported"
    );

    let text = cli(&["debug", "--emulator", "capabilities"]);
    assert!(text.status.success());
    let text = String::from_utf8_lossy(&text.stdout);
    assert!(text.contains("Backend: Emulator"), "{text}");
    assert!(text.contains("Availability: available"), "{text}");
    assert!(text.contains("Operations:"), "{text}");
}

#[test]
fn debug_help_describes_each_native_backend_without_claiming_windows_only_support() {
    let output = cli(&["debug", "--help"]);
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);

    assert!(help.contains("Linux ptrace"), "{help}");
    assert!(help.contains("windows_native_debugger"), "{help}");
    assert!(
        help.contains("macOS native process control is not implemented"),
        "{help}"
    );
    assert!(
        !help.contains("live Windows processes with the native-debugger feature"),
        "stale Windows-only help remains: {help}"
    );
}
