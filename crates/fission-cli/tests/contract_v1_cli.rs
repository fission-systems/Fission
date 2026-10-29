//! `--contract v1`: envelope, exit codes, and the promise that legacy
//! behavior is untouched. None of these need a real binary to analyse.

use serde_json::Value;
use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(args)
        .env_remove("FISSION_CONTRACT")
        .output()
        .expect("run fission_cli")
}

/// stdout must be exactly one JSON document.
fn envelope(output: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|error| panic!("stdout is not one JSON value ({error}): {stdout}"))
}

#[test]
fn missing_input_is_an_input_error_with_exit_3() {
    let output = cli(&["--contract", "v1", "list", "no-such-file.exe", "--json"]);
    assert_eq!(output.status.code(), Some(3));
    let body = envelope(&output);
    assert_eq!(body["contract"], "fission-cli/1");
    assert_eq!(body["command"], "list");
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "input_not_found");
    assert_eq!(body["error"]["kind"], "input");
    assert_eq!(body["error"]["retryable"], false);
    assert!(body.get("data").is_none());
}

#[test]
fn contract_mode_requires_the_commands_own_json_flag() {
    let output = cli(&["--contract", "v1", "list", "no-such-file.exe"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(envelope(&output)["error"]["code"], "json_required");
}

#[test]
fn unsupported_contract_version_is_a_usage_error() {
    let output = cli(&["--contract", "v9", "list", "x.exe", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(envelope(&output)["error"]["code"], "usage_error");
}

#[test]
fn missing_required_arguments_are_a_usage_error() {
    let output = cli(&["--contract", "v1", "decomp", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    let body = envelope(&output);
    assert_eq!(body["error"]["code"], "usage_error");
    assert_eq!(body["command"], "decomp");
}

#[test]
fn environment_variable_alone_selects_the_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(["list", "no-such-file.exe", "--json"])
        .env("FISSION_CONTRACT", "v1")
        .output()
        .expect("run fission_cli");
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(envelope(&output)["error"]["code"], "input_not_found");
}

#[test]
fn legacy_failure_output_and_exit_code_are_unchanged() {
    let output = cli(&["list", "no-such-file.exe", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stdout.is_empty(),
        "legacy mode must not start writing to stdout on failure"
    );
}

#[test]
fn help_is_passed_through_in_contract_mode() {
    let output = cli(&["--contract", "v1", "list", "--help"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage"));
}

#[test]
fn schema_subcommand_describes_the_envelope_and_error_codes() {
    let schema = envelope(&cli(&["schema"]));
    assert_eq!(schema["properties"]["contract"]["const"], "fission-cli/1");

    let command = envelope(&cli(&["schema", "decomp"]));
    assert_eq!(command["properties"]["command"]["const"], "decomp");

    let codes = envelope(&cli(&["schema", "--error-codes"]));
    let listed: Vec<&str> = codes["error_codes"]
        .as_array()
        .expect("code list")
        .iter()
        .filter_map(|entry| entry["code"].as_str())
        .collect();
    for expected in ["input_not_found", "all_items_failed", "internal_panic"] {
        assert!(listed.contains(&expected), "missing {expected}");
    }

    let list = envelope(&cli(&["schema", "--list"]));
    assert!(
        list["commands"]
            .as_array()
            .expect("command list")
            .iter()
            .any(|name| name == "decomp")
    );

    assert_ne!(cli(&["schema", "no-such-command"]).status.code(), Some(0));
}
