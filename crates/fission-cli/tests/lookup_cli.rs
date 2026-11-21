//! `types`, `whatis`, `api`, `identify` on a no-match, `db watch` and the
//! PE identity details read facts their owners already hold.
use serde_json::Value;
use std::process::{Command, Output};

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

const DWARF5: &str = "crates/fission-loader/testdata/x64_dwarf5_strx.elf";

/// `struct point { int horizontal; long vertical; }` comes from DWARF.
#[test]
fn types_lists_the_dwarf_struct_with_its_fields() {
    let types = json(&["types", DWARF5, "--source", "dwarf", "--json", "--no-db"]);
    let point = types
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "point")
        .unwrap_or_else(|| panic!("no point in {types}"));
    assert_eq!(point["source"], "dwarf");
    assert_eq!(point["verified"], true);
    let fields: Vec<&str> = point["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["name"].as_str().unwrap())
        .collect();
    assert_eq!(fields, ["horizontal", "vertical"]);
    // The filter keeps the header structs out.
    assert!(
        types
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["source"] == "dwarf")
    );
}

#[test]
fn types_lists_cpp_rtti_classes() {
    let types = json(&[
        "types",
        "crates/fission-loader/testdata/x64_cpp_methods_a.elf",
        "--source",
        "cpp_rtti",
        "--json",
        "--no-db",
    ]);
    let names: Vec<&str> = types
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"Base") && names.contains(&"Derived"),
        "{names:?}"
    );
}

/// 0x201140 is 16 bytes into `manhattan`, line 14 of `dwarf5_strx.c`, in `.text`.
#[test]
fn whatis_names_function_section_and_line() {
    let body = json(&["whatis", DWARF5, "--addr", "0x201140", "--json", "--no-db"]);
    assert_eq!(body["function"]["name"], "manhattan", "{body}");
    assert_eq!(body["function"]["offset"], 16, "{body}");
    assert_eq!(body["memory_block"]["name"], ".text", "{body}");
    assert_eq!(body["source"]["line"], 14, "{body}");

    let nowhere = json(&["whatis", DWARF5, "--addr", "0x10", "--json", "--no-db"]);
    assert!(nowhere["function"].is_null(), "{nowhere}");
}

#[test]
fn api_prints_a_known_prototype_and_refuses_an_unknown_one() {
    let memcpy = json(&["api", "memcpy", "--json"]);
    assert_eq!(memcpy["name"], "memcpy");
    assert_eq!(memcpy["params"].as_array().unwrap().len(), 3);
    assert!(!run(&["api", "definitely_not_an_api_name"]).status.success());
}

/// A function that hashes but matches nothing reports its hashes, so the
/// miss can be looked into.
#[test]
fn identify_without_a_match_reports_the_hashes() {
    let body = json(&[
        "identify",
        "crates/fission-dir/testdata/ground_truth_fixture.elf",
        "--function",
        "0x201510",
        "--json",
        "--no-db",
    ]);
    assert_eq!(body["matched"], false);
    assert!(
        body["full_hash"].as_str().unwrap().starts_with("0x"),
        "{body}"
    );
    assert!(body["code_unit_count"].as_u64().unwrap() > 0, "{body}");
}

#[test]
fn db_records_and_forgets_a_watchpoint() {
    let dir = std::env::temp_dir().join(format!("fission-watch-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("w.fission.json");
    let db = db.to_str().unwrap();
    let binary = "crates/fission-dir/testdata/ground_truth_fixture.elf";
    json(&[
        "--db", db, "db", binary, "watch", "0x202698", "--size", "8", "--read", "--json",
    ]);
    let shown = json(&["--db", db, "db", binary, "show", "--json"]);
    let watch = &shown["watchpoints"][0];
    assert_eq!(watch["address"], "0x202698");
    assert_eq!(watch["size"], 8);
    assert_eq!(watch["on_read"], true);
    json(&["--db", db, "db", binary, "rm-watch", "0x202698", "--json"]);
    let shown = json(&["--db", db, "db", binary, "show", "--json"]);
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(shown["watchpoints"], serde_json::json!([]));
}

/// The CodeView record names the PDB a symbol server would look up.
#[test]
fn info_identity_carries_the_codeview_record() {
    let body = json(&[
        "info",
        "crates/fission-loader/testdata/x64_pdb_struct_test.exe",
        "--identity",
        "--json",
        "--no-db",
    ]);
    let identity = if body.get("identity").is_some() {
        &body["identity"]
    } else {
        &body
    };
    let codeview = &identity["pe"]["codeview"];
    assert_eq!(codeview["age"], 1, "{identity}");
    assert_eq!(codeview["guid"].as_str().unwrap().len(), 32, "{identity}");
    assert!(
        codeview["path"].as_str().unwrap().ends_with(".pdb"),
        "{identity}"
    );
}
