//! `fission_cli quotient` (experimental, RFC #1093) against the step
//! functions in `crates/fission-quotient/testdata/quotient_fixture.S`.

use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Output};

const FIXTURE: &str = "crates/fission-quotient/testdata/quotient_fixture.elf";

fn repo_root() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
}

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(args)
        .env_remove("FISSION_CONTRACT")
        .current_dir(repo_root())
        .output()
        .expect("run fission_cli")
}

fn envelope(output: &Output) -> Value {
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|error| panic!("stdout is not one JSON value ({error}): {stdout}"))
}

fn contract(args: &[&str]) -> (Output, Value) {
    let mut full = vec!["--contract", "v1", "--no-db", "quotient"];
    full.extend_from_slice(args);
    full.push("--json");
    let output = cli(&full);
    let body = envelope(&output);
    (output, body)
}

/// The committed payload schema, as `schema quotient` embeds it.
fn payload_schema() -> Value {
    let text =
        std::fs::read_to_string(repo_root().join("docs/schemas/cli-quotient-data.v1.schema.json"))
            .expect("committed schema");
    serde_json::from_str(&text).expect("schema is JSON")
}

/// Every key the schema requires is present, at every level the schema
/// describes, and every `const` / `enum` holds. (CI validates the full schema
/// with a JSON Schema validator; this keeps the requirement visible to
/// `cargo nextest` alone.)
fn assert_conforms(schema: &Value, value: &Value, at: &str) {
    if let Some(variants) = schema.get("oneOf").and_then(Value::as_array) {
        let matching = variants
            .iter()
            .filter(|variant| {
                let kind = &variant["properties"]["kind"]["const"];
                kind.is_null() || *kind == value["kind"]
            })
            .collect::<Vec<_>>();
        assert_eq!(matching.len(), 1, "{at}: no single variant for {value}");
        return assert_conforms(matching[0], value, at);
    }
    if let Some(constant) = schema.get("const") {
        assert_eq!(value, constant, "{at}");
    }
    if let Some(options) = schema.get("enum").and_then(Value::as_array) {
        assert!(options.contains(value), "{at}: {value} not in {options:?}");
    }
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        for key in required {
            let key = key.as_str().unwrap();
            assert!(value.get(key).is_some(), "{at}: missing `{key}`");
        }
    }
    if let (Some(properties), Some(object)) = (
        schema.get("properties").and_then(Value::as_object),
        value.as_object(),
    ) {
        for (key, sub) in properties {
            if let Some(child) = object.get(key) {
                assert_conforms(sub, child, &format!("{at}.{key}"));
            }
        }
    }
    if let (Some(items), Some(array)) = (schema.get("items"), value.as_array()) {
        for (index, item) in array.iter().enumerate() {
            assert_conforms(items, item, &format!("{at}[{index}]"));
        }
    }
}

#[test]
fn a_flattened_dispatcher_is_reported_as_its_three_protocol_states() {
    let output = cli(&[
        "quotient",
        FIXTURE,
        "--name",
        "auth_step",
        "--input",
        "cmd:1:32",
    ]);
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8_lossy(&output.stdout);
    for line in [
        "proved under model separated-registers-frame-context.v1 for words of up to 35 steps",
        "context: ctx[0x0:4] control, ctx[0x4:4] data",
        "3 states, 5 transitions",
        "0 -> 1 if (cmd[7:0] == 0x5a) / returns 0",
        "1 -> 2 if (cmd[7:0] == 0xa5) / returns 1",
        "states 0 and 1 differ on [0xa5]",
    ] {
        assert!(text.contains(line), "missing `{line}` in:\n{text}");
    }
}

#[test]
fn the_contract_payload_is_the_committed_v1_schema() {
    let (output, body) = contract(&[FIXTURE, "--name", "auth_step"]);
    assert_eq!(output.status.code(), Some(0), "{body}");
    assert_eq!(body["command"], "quotient");
    assert_eq!(body["ok"], true);
    assert_eq!(body["schema_version"], 1);
    let data = &body["data"];
    assert_conforms(&payload_schema(), data, "data");
    assert_eq!(data["status"]["kind"], "proved_under_model");
    assert_eq!(
        data["boundary"]["entry"],
        data["foundation"]["function_address"]
    );
    assert_eq!(data["states"].as_array().map(Vec::len), Some(3));

    // `schema quotient` serves the same payload schema.
    let schema = envelope(&cli(&["schema", "quotient"]));
    let mut committed = payload_schema();
    committed.as_object_mut().unwrap().remove("$schema");
    assert_eq!(schema["properties"]["data"], committed);
    let list = envelope(&cli(&["schema", "--list"]));
    assert!(
        list["typed_data_commands"]
            .as_array()
            .unwrap()
            .contains(&Value::from("quotient"))
    );
}

/// A function outside the model is a result -- the payload says why -- not a
/// command failure.
#[test]
fn an_unsupported_function_is_a_result_not_an_error() {
    let (output, body) = contract(&[FIXTURE, "--name", "clock_step"]);
    assert_eq!(output.status.code(), Some(0), "{body}");
    let data = &body["data"];
    assert_conforms(&payload_schema(), data, "data");
    assert_eq!(data["status"]["kind"], "unsupported_semantics");
    assert!(
        data["status"]["reason"]
            .as_str()
            .unwrap()
            .contains("user-defined operation")
    );
    assert_eq!(data["transitions"].as_array().map(Vec::len), Some(0));
}

#[test]
fn every_status_the_schema_names_conforms() {
    for (args, kind) in [
        (vec![FIXTURE, "--name", "split_step"], "refuted"),
        (
            vec![FIXTURE, "--name", "counter_step", "--max-states", "4"],
            "partial_frontier",
        ),
        (
            vec![FIXTURE, "--name", "latch_step", "--conflict-budget", "0"],
            "empirically_consistent",
        ),
    ] {
        let (output, body) = contract(&args);
        assert_eq!(output.status.code(), Some(0), "{body}");
        assert_conforms(&payload_schema(), &body["data"], "data");
        assert_eq!(body["data"]["status"]["kind"], kind, "{args:?}");
    }
}

#[test]
fn an_unknown_name_is_function_not_found_with_exit_3() {
    let (output, body) = contract(&[FIXTURE, "--name", "no_such_step"]);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(body["error"]["code"], "function_not_found");
}

#[test]
fn a_malformed_port_is_a_usage_error() {
    for port in ["cmd:1:7", "cmd:1", ":1:8", "cmd:x:8"] {
        let (output, body) = contract(&[FIXTURE, "--addr", "0x201122", "--input", port]);
        assert_eq!(output.status.code(), Some(2), "{port}: {body}");
        assert_eq!(body["error"]["code"], "usage_error");
    }
}

#[test]
fn the_context_can_start_from_given_bytes() {
    // Starting in AUTH_OK (mode 2), every command is accepted: one state.
    let (output, body) = contract(&[
        FIXTURE,
        "--name",
        "auth_step",
        "--context-init",
        "0200000000000000",
    ]);
    assert_eq!(output.status.code(), Some(0), "{body}");
    let data = &body["data"];
    assert_eq!(data["boundary"]["context"]["initial"], "0200000000000000");
    assert_eq!(data["status"]["kind"], "proved_under_model");
    assert_eq!(data["states"].as_array().map(Vec::len), Some(1));
    assert_eq!(data["transitions"][0]["output"], "1");
}
