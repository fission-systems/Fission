//! A raw image (no container header) loads at a caller-given base under a
//! caller-given language; `languages` lists the IDs that `--language` takes;
//! `disasm --json` carries the DWARF source line of each instruction.
use serde_json::Value;
use std::path::PathBuf;
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(args)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .expect("run fission_cli")
}

/// `push rbp; mov rbp,rsp; xor eax,eax; pop rbp; ret` with nothing around it.
/// One file per test, so tests running in one process (`cargo test`) never
/// write the file another is reading.
fn raw_blob(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fission-raw-{}-{test}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("blob.bin");
    std::fs::write(&path, [0x55, 0x48, 0x89, 0xe5, 0x31, 0xc0, 0x5d, 0xc3]).unwrap();
    path
}

#[test]
fn a_raw_image_disassembles_at_the_given_base() {
    let blob = raw_blob("a_raw_image_disassembles_at_the_given_base");
    let out = run(&[
        "--language",
        "x86:LE:64:default",
        "--base",
        "0x400000",
        "disasm",
        blob.to_str().unwrap(),
        "--addr",
        "0x400000",
        "-n",
        "4",
        "--json",
        "--no-db",
    ]);
    assert!(out.status.success(), "{out:?}");
    let rows: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(rows[0]["address"], "0x400000");
    assert_eq!(rows[0]["instruction"], "push RBP");
    assert_eq!(rows[3]["instruction"], "pop RBP");
}

#[test]
fn without_base_a_raw_image_is_refused_with_the_way_out() {
    let blob = raw_blob("without_base_a_raw_image_is_refused_with_the_way_out");
    let out = run(&[
        "--language",
        "x86:LE:64:default",
        "disasm",
        blob.to_str().unwrap(),
        "--addr",
        "0",
    ]);
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("--base"),
        "{out:?}"
    );
}

#[test]
fn base_needs_a_language_and_entry_must_be_inside() {
    let blob = raw_blob("base_needs_a_language_and_entry_must_be_inside");
    let no_language = run(&["--base", "0x400000", "list", blob.to_str().unwrap()]);
    assert!(!no_language.status.success());
    assert!(String::from_utf8_lossy(&no_language.stderr).contains("--language"));

    let outside = run(&[
        "--language",
        "x86:LE:64:default",
        "--base",
        "0x400000",
        "--entry",
        "0x500000",
        "list",
        blob.to_str().unwrap(),
    ]);
    assert!(!outside.status.success());
    assert!(String::from_utf8_lossy(&outside.stderr).contains("outside the image"));
}

#[test]
fn languages_lists_what_language_accepts() {
    let out = run(&["languages", "--json"]);
    assert!(out.status.success(), "{out:?}");
    let all: Value = serde_json::from_slice(&out.stdout).expect("json");
    let ids: Vec<&str> = all
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"x86:LE:64:default"), "{ids:?}");

    let arm: Value =
        serde_json::from_slice(&run(&["languages", "--processor", "arm", "--json"]).stdout)
            .expect("json");
    let arm = arm.as_array().unwrap();
    assert!(!arm.is_empty());
    assert!(arm.len() < ids.len());
    assert!(arm.iter().all(|r| {
        r["processor"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("arm")
    }));
}

/// `manhattan` is line 13 of `dwarf5_strx.c`; a binary without a line table
/// gets no `source` at all rather than an empty one.
#[test]
fn disasm_json_carries_the_dwarf_source_line() {
    let with_lines = "crates/fission-loader/testdata/x64_dwarf5_strx.elf";
    let out = run(&[
        "disasm",
        with_lines,
        "--addr",
        "0x201130",
        "--function",
        "--json",
        "--no-db",
    ]);
    assert!(out.status.success(), "{out:?}");
    let body: Value = serde_json::from_slice(&out.stdout).expect("json");
    let first = &body["instructions"][0];
    assert_eq!(first["source"]["file"], "dwarf5_strx.c");
    assert_eq!(first["source"]["line"], 13);

    let without = "crates/fission-sleigh/testdata/x64_static_switch_no_reloc.elf";
    let out = run(&[
        "disasm",
        without,
        "--addr",
        "0x1002030",
        "-n",
        "4",
        "--json",
        "--no-db",
    ]);
    assert!(out.status.success(), "{out:?}");
    let rows: Value = serde_json::from_slice(&out.stdout).expect("json");
    assert!(
        rows.as_array()
            .unwrap()
            .iter()
            .all(|r| r.get("source").is_none())
    );
}
