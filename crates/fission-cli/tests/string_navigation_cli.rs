//! A string lead can be followed to its evidence, function and one-hop calls.
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn sample() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("sample.elf");
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../fission-emulator/testdata/x64_static_printf_malloc.elf"
        ),
        &binary,
    )
    .unwrap();
    (dir, binary)
}
fn run(binary: &Path, command: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .arg(command)
        .arg(binary)
        .args(args)
        .output()
        .unwrap()
}
fn json(binary: &Path, command: &str, args: &[&str]) -> Value {
    let mut arguments = args.to_vec();
    arguments.push("--json");
    let output = run(binary, command, &arguments);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("one JSON stdout document")
}

#[test]
fn string_context_reaches_real_reference_function_and_callgraph() {
    let (_dir, binary) = sample();
    let original = std::fs::read(&binary).unwrap();
    let context = json(&binary, "strings", &["--context", "--contains", "msg=%s"]);
    let strings = context["strings"].as_array().unwrap();
    assert_eq!(strings.len(), 1);
    let lead = &strings[0]["context"];
    assert!(!lead["references"].as_array().unwrap().is_empty());
    assert_eq!(lead["functions"].as_array().unwrap().len(), 1);
    let function = &lead["functions"][0];
    let entry = function["address"].as_str().unwrap();
    let scoped = json(&binary, "callgraph", &["--function", entry]);
    assert_eq!(scoped["nodes"].as_array().unwrap().len(), 1);
    assert_eq!(scoped["nodes"][0]["address"], entry);
    assert_eq!(scoped["analysis"], context["analysis"]);
    for relation in ["callers", "callees"] {
        let expected: Vec<_> = scoped["nodes"][0][relation]
            .as_array()
            .unwrap()
            .iter()
            .map(|edge| (edge["addr"].clone(), edge["count"].clone()))
            .collect();
        let actual: Vec<_> = function[relation]
            .as_array()
            .unwrap()
            .iter()
            .map(|edge| (edge["address"].clone(), edge["count"].clone()))
            .collect();
        assert_eq!(actual, expected);
    }
    let refs = lead["references"].as_array().unwrap();
    for id in function["reference_ids"].as_array().unwrap() {
        assert!(refs.iter().any(|reference| &reference["id"] == id));
    }
    assert_eq!(std::fs::read(&binary).unwrap(), original);
}

#[test]
fn context_uses_saved_names_and_no_db_can_ignore_them() {
    let (_dir, binary) = sample();
    let context = json(&binary, "strings", &["--context", "--contains", "msg=%s"]);
    let entry = context["strings"][0]["context"]["functions"][0]["address"]
        .as_str()
        .unwrap();
    json(&binary, "db", &["name", entry, "format_message"]);
    let named = json(&binary, "strings", &["--context", "--contains", "msg=%s"]);
    assert_eq!(
        named["strings"][0]["context"]["functions"][0]["name"],
        "format_message"
    );
    let scoped = json(&binary, "callgraph", &["--function", entry]);
    assert_eq!(scoped["nodes"][0]["name"], "format_message");
    let raw = json(
        &binary,
        "strings",
        &["--context", "--contains", "msg=%s", "--no-db"],
    );
    assert_ne!(
        raw["strings"][0]["context"]["functions"][0]["name"],
        "format_message"
    );
}

#[test]
fn legacy_arrays_filters_and_empty_context_keep_their_contracts() {
    let (_dir, binary) = sample();
    let ordinary = json(&binary, "strings", &["--contains", "msg=%s"]);
    assert!(ordinary.is_array());
    assert_eq!(ordinary.as_array().unwrap().len(), 1);
    assert!(ordinary[0].get("referrers").is_none());
    let xrefs = json(&binary, "strings", &["--xrefs", "--contains", "msg=%s"]);
    assert!(xrefs[0]["referrers"].is_array());
    let empty = json(
        &binary,
        "strings",
        &["--context", "--contains", "this-string-is-not-present"],
    );
    assert!(empty["strings"].as_array().unwrap().is_empty());
    assert_eq!(empty["analysis"]["layers"].as_array().unwrap().len(), 5);
    assert!(
        empty["analysis"]["layers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|layer| layer["layer"] == "pcode" && layer["status"] == "not_requested")
    );
    let text = run(&binary, "strings", &["--context", "--contains", "msg=%s"]);
    assert!(text.status.success());
    let text = String::from_utf8(text.stdout).unwrap();
    assert!(text.contains("xref_analysis:") && text.contains("function 0x"));
}

#[test]
fn unknown_callgraph_entry_is_refused_without_success_json() {
    let (_dir, binary) = sample();
    let output = run(
        &binary,
        "callgraph",
        &["--function", "0xdeadbeef", "--json"],
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no discovered function starts"));
}

#[test]
fn unsupported_isa_is_refused_without_success_json() {
    let (_dir, binary) = sample();
    let mut bytes = std::fs::read(&binary).unwrap();
    // The loader rejects an unknown ISA before an index can be built.
    bytes[18..20].copy_from_slice(&0xffffu16.to_le_bytes());
    std::fs::write(&binary, bytes).unwrap();
    let output = run(
        &binary,
        "strings",
        &["--context", "--contains", "msg=%s", "--json"],
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported machine for ELF"));
}
