//! Does work accumulate between invocations?
//!
//! Everything else this tool prints is derived: run it again and you get it
//! again. A name someone chose cannot be re-derived, and until there was
//! somewhere to put it, every invocation started from the file and ended with
//! the terminal. This drives the sequence that makes it a platform rather
//! than a set of tools: decide something in one command, see it in the next.

use std::path::{Path, PathBuf};
use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fission_cli"))
}

/// A private copy, because the database lives beside the binary and these
/// tests write one.
fn sample(name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join(name);
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../fission-emulator/testdata/win_x64_write.exe"
        ),
        &path,
    )
    .expect("copy fixture");
    (dir, path)
}

fn run(args: &[&str]) -> (bool, String) {
    let output = cli().args(args).output().expect("run the CLI");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.success(), text)
}

const ENTRY: &str = "0x140001000";

#[test]
fn a_name_chosen_in_one_command_shows_up_in_the_next() {
    let (_dir, binary) = sample("sample.exe");
    let path = binary.to_str().expect("path");

    let (ok, before) = run(&["list", path]);
    assert!(ok, "{before}");
    assert!(before.contains("pe_entry"), "{before}");

    let (ok, out) = run(&["db", path, "name", ENTRY, "write_hello_and_exit"]);
    assert!(ok, "{out}");

    // A separate process, reading the same binary.
    let (ok, after) = run(&["list", path]);
    assert!(ok, "{after}");
    assert!(
        after.contains("write_hello_and_exit"),
        "the name did not survive the invocation:\n{after}"
    );
    assert!(!after.contains("pe_entry"), "{after}");

    // And it reaches decompilation, not just the listing -- everything goes
    // through one `LoadedBinary`, which is the point of applying it at load.
    let (ok, decomp) = run(&["decomp", path, "--addr", ENTRY]);
    assert!(ok, "{decomp}");
    assert!(
        decomp.contains("write_hello_and_exit"),
        "the name did not reach decompilation:\n{}",
        &decomp[..decomp.len().min(400)]
    );
}

#[test]
fn no_db_reports_the_binary_as_it_is_on_disk() {
    let (_dir, binary) = sample("sample.exe");
    let path = binary.to_str().expect("path");
    let (ok, out) = run(&["db", path, "name", ENTRY, "renamed"]);
    assert!(ok, "{out}");

    let (ok, with) = run(&["list", path]);
    assert!(ok && with.contains("renamed"), "{with}");

    let (ok, without) = run(&["list", path, "--no-db"]);
    assert!(ok, "{without}");
    assert!(
        without.contains("pe_entry") && !without.contains("renamed"),
        "--no-db still applied the database:\n{without}"
    );
}

/// Naming an address no function starts at is nearly always a typo, and
/// finding that out a week later -- from a name that silently never appeared
/// -- is worse than being told now.
#[test]
fn naming_an_address_with_no_function_is_refused() {
    let (_dir, binary) = sample("sample.exe");
    let path = binary.to_str().expect("path");

    let (ok, out) = run(&["db", path, "name", "0x140001005", "middle_of_a_function"]);
    assert!(!ok, "it accepted an address inside a function: {out}");
    assert!(
        out.contains("0x140001000"),
        "the error does not say which address to use instead: {out}"
    );

    let (ok, out) = run(&["db", path, "name", "0xdead0000", "nowhere"]);
    assert!(!ok, "it accepted an address in no function: {out}");
    assert!(out.contains("no function starts"), "{out}");
}

/// A database is bound to a binary by content hash. Applying it to a
/// different file would rename whatever happens to live at those addresses,
/// and the result would read like analysis.
#[test]
fn a_database_from_another_binary_is_refused() {
    let (_dir, mine) = sample("mine.exe");
    let (_other_dir, other) = sample("other.exe");
    let (ok, out) = run(&[
        "db",
        mine.to_str().expect("path"),
        "name",
        ENTRY,
        "parse_header",
    ]);
    assert!(ok, "{out}");

    // Move the database next to a binary it was not made for, having first
    // made that binary different.
    std::fs::write(&other, {
        let mut bytes = std::fs::read(&other).expect("read");
        bytes.push(0);
        bytes
    })
    .expect("differ");
    std::fs::copy(db_path(&mine), db_path(&other)).expect("copy database");

    let (ok, out) = run(&["list", other.to_str().expect("path")]);
    assert!(!ok, "it applied a database from a different binary: {out}");
    assert!(out.contains("different binary"), "unhelpful error: {out}");
}

fn db_path(binary: &Path) -> PathBuf {
    let mut name = binary.as_os_str().to_os_string();
    name.push(".fission.json");
    PathBuf::from(name)
}

/// A breakpoint that took an hour to find should still be there tomorrow.
#[test]
#[cfg(feature = "debugger")]
fn a_recorded_breakpoint_is_restored_by_a_session() {
    let (_dir, binary) = sample("sample.exe");
    let path = binary.to_str().expect("path");

    let (ok, out) = run(&["db", path, "bp", "0x140001016"]);
    assert!(ok, "{out}");

    // No `-c bp` here: the session finds it in the database.
    let (ok, out) = run(&[
        "debug",
        "--emulator",
        "session",
        path,
        "-c",
        "continue",
        "--json",
    ]);
    assert!(ok, "{out}");
    // The report is the first JSON document on the stream; the banner and
    // anything on stderr sit around it.
    let start = out.find('{').expect("json");
    let report: serde_json::Value = serde_json::Deserializer::from_str(&out[start..])
        .into_iter()
        .next()
        .expect("a document")
        .expect("parse");
    assert_eq!(
        report["results"][0]["stop"], "breakpoint:0x140001016",
        "the session did not restore the recorded breakpoint: {report:#}"
    );
}
