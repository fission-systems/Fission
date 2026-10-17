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

/// The type loop: correcting a type changes what the decompiler prints, and
/// the correction survives the invocation.
///
/// This is the lever Ghidra's type editor is, and the reason renaming alone
/// plateaus -- a name says what a function is, a signature says what goes in
/// and what comes out, and that propagates into every use.
#[test]
fn a_chosen_signature_reaches_the_decompiler() {
    let (_dir, binary) = sample("sample.exe");
    let path = binary.to_str().expect("path");

    // A function with a parameter the decompiler had to guess at.
    let (ok, before) = run(&["decomp", path, "--addr", ENTRY]);
    assert!(ok, "{before}");

    let (ok, out) = run(&["db", path, "sig", ENTRY, "int (const char *text, int len)"]);
    assert!(ok, "{out}");

    let (ok, after) = run(&["decomp", path, "--addr", ENTRY]);
    assert!(ok, "{after}");
    assert_ne!(before, after, "the signature changed nothing in the output");
    assert!(
        after.contains("const char *") && after.contains("text"),
        "the chosen parameter type did not reach the decompiler:\n{}",
        &after[..after.len().min(600)]
    );

    // And `--no-db` still shows the binary as it is.
    let (ok, without) = run(&["decomp", path, "--addr", ENTRY, "--no-db"]);
    assert!(ok, "{without}");
    assert_eq!(without, before, "--no-db still applied the signature");
}

/// A signature this cannot read confidently is refused: a wrong parse would
/// attach a type nobody chose, and the output would look like analysis.
#[test]
fn an_unreadable_signature_is_refused() {
    let (_dir, binary) = sample("sample.exe");
    let path = binary.to_str().expect("path");

    for bad in ["int", "int (char *", "int (int, ...)"] {
        let (ok, out) = run(&["db", path, "sig", ENTRY, bad]);
        assert!(!ok, "{bad:?} was accepted: {out}");
    }

    // And nothing was written by the attempts.
    let (ok, show) = run(&["db", path, "show"]);
    assert!(ok, "{show}");
    assert!(
        show.contains("nothing recorded yet"),
        "a refused signature was saved anyway:\n{show}"
    );
}

/// A chosen signature reaches the *callers* too, which is where most of the
/// leverage is.
///
/// A called function this unit does not define is declared `extern T f(...)`,
/// with an open parameter list, because the call sites are the only evidence
/// of arity and they disagree. A signature somebody wrote down is the one
/// case where that reasoning does not apply -- and until it was wired
/// through, a typed callee was typed only inside itself.
#[test]
fn a_chosen_signature_reaches_the_callers() {
    // A fixture with calls in it: the PE one above holds a single function,
    // so nothing in it can declare anything.
    let dir = tempfile::tempdir().expect("temp dir");
    let binary = dir.path().join("static.elf");
    std::fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../fission-emulator/testdata/x64_static_printf_malloc.elf"
        ),
        &binary,
    )
    .expect("copy fixture");
    let path = binary.to_str().expect("path");

    let addresses = {
        let (ok, listing) = run(&["list", path, "--json"]);
        assert!(ok, "{listing}");
        let start = listing.find(['[', '{']).expect("json");
        let listed: serde_json::Value = serde_json::Deserializer::from_str(&listing[start..])
            .into_iter()
            .next()
            .expect("a document")
            .expect("parse");
        let functions = listed
            .get("functions")
            .and_then(|f| f.as_array())
            .or_else(|| listed.as_array())
            .expect("a function list")
            .clone();
        functions
            .iter()
            .filter_map(|f| Some(f["address"].as_str()?.to_string()))
            .collect::<Vec<_>>()
    };
    assert!(addresses.len() > 5, "too few functions to find a call");

    // Find a real caller/callee pair rather than guessing one: decompile each
    // function and look for an `extern` it emits for somebody else.
    let mut found = None;
    for caller in &addresses {
        let (ok, text) = run(&["decomp", path, "--addr", caller]);
        if !ok {
            continue;
        }
        for line in text.lines() {
            let Some(rest) = line.trim().strip_prefix("extern ") else {
                continue;
            };
            let Some(open) = rest.find('(') else { continue };
            let name = rest[..open].rsplit([' ', '*']).next().unwrap_or("").trim();
            // Only a function this binary defines can be given a signature.
            if let Some(callee) = addresses.iter().find(|a| {
                name == format!("FUN_{a}") || name == format!("sub_{}", a.trim_start_matches("0x"))
            }) {
                found = Some((caller.clone(), callee.clone(), name.to_string()));
                break;
            }
        }
        if found.is_some() {
            break;
        }
    }
    let Some((caller, callee, _)) = found else {
        // Nothing in this fixture calls a locally defined function through an
        // `extern`; there is no pair to check rather than a failure to report.
        eprintln!("skipped: no caller/callee pair in the fixture");
        return;
    };

    let (ok, out) = run(&["db", path, "name", &callee, "chosen_callee"]);
    assert!(ok, "{out}");
    let (ok, out) = run(&["db", path, "sig", &callee, "int (const char *what)"]);
    assert!(ok, "{out}");

    let (ok, text) = run(&["decomp", path, "--addr", &caller]);
    assert!(ok, "{text}");
    assert!(
        text.contains("extern int chosen_callee(const char *what);"),
        "the caller did not declare the callee the way it was written:\n{}",
        text.lines()
            .filter(|l| l.contains("chosen_callee"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

fn db_json(binary: &Path, args: &[&str]) -> serde_json::Value {
    let output = cli()
        .arg("db")
        .arg(binary)
        .args(args)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout is one JSON document")
}

#[test]
fn history_undo_redo_and_diff_work_across_processes() {
    let (dir, binary) = sample("history.exe");
    let bytes = std::fs::read(&binary).unwrap();
    assert_eq!(db_json(&binary, &["name", ENTRY, "chosen"])["revision"], 1);
    let snapshot = dir.path().join("snapshot.json");
    std::fs::copy(db_path(&binary), &snapshot).unwrap();
    assert_eq!(
        db_json(&binary, &["sig", ENTRY, "int (char *buf)"])["revision"],
        2
    );
    assert_eq!(
        db_json(&binary, &["note", ENTRY, "inspect caller"])["revision"],
        3
    );
    assert_eq!(db_json(&binary, &["bp", ENTRY])["revision"], 4);
    let history = db_json(&binary, &["history"]);
    assert_eq!(history["cursor"], 4);
    assert_eq!(history["revisions"].as_array().unwrap().len(), 4);
    assert_eq!(history["revisions"][0]["changes"][0]["address"], ENTRY);
    let diff = db_json(&binary, &["diff", snapshot.to_str().unwrap()]);
    assert_eq!(diff["changes"].as_array().unwrap().len(), 3);
    assert_eq!(diff["changes"][0]["kind"], "comment");
    assert_eq!(diff["changes"][0]["after"], serde_json::Value::Null);
    for revision in (1..=4).rev() {
        assert_eq!(db_json(&binary, &["undo"])["revision"], revision);
    }
    let listing = run(&["list", binary.to_str().unwrap()]);
    assert!(listing.0 && listing.1.contains("pe_entry"), "{}", listing.1);
    let undone = db_json(&binary, &["show"]);
    assert!(undone["names"].as_object().unwrap().is_empty());
    for revision in 1..=4 {
        assert_eq!(db_json(&binary, &["redo"])["revision"], revision);
    }
    let listing = run(&["list", binary.to_str().unwrap()]);
    assert!(listing.0 && listing.1.contains("chosen"), "{}", listing.1);
    assert_eq!(db_json(&binary, &["history"]), history);
    assert_eq!(
        std::fs::read(&binary).unwrap(),
        bytes,
        "analysis never rewrites the binary"
    );
}

#[test]
fn legacy_project_migrates_on_edit_and_no_op_keeps_redo() {
    let (_dir, binary) = sample("legacy.exe");
    db_json(&binary, &["name", ENTRY, "legacy"]);
    let path = db_path(&binary);
    let mut legacy = db_json(&binary, &["show"]);
    legacy["schema_version"] = serde_json::json!(1);
    legacy.as_object_mut().unwrap().remove("history");
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    assert_eq!(db_json(&binary, &["show"])["schema_version"], 1);
    assert_eq!(db_json(&binary, &["name", ENTRY, "next"])["revision"], 1);
    assert_eq!(db_json(&binary, &["show"])["schema_version"], 2);
    db_json(&binary, &["undo"]);
    assert_eq!(db_json(&binary, &["show"])["names"][ENTRY], "legacy");
    assert_eq!(
        db_json(&binary, &["name", ENTRY, "legacy"])["revision"],
        serde_json::Value::Null
    );
    assert_eq!(db_json(&binary, &["redo"])["revision"], 1);
    db_json(&binary, &["undo"]);
    assert_eq!(
        db_json(&binary, &["note", ENTRY, "new branch"])["revision"],
        2
    );
    let output = cli()
        .arg("db")
        .arg(&binary)
        .arg("redo")
        .arg("--json")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn read_only_commands_create_no_files_and_invalid_history_is_not_rewritten() {
    let (dir, binary) = sample("readonly.exe");
    assert_eq!(db_json(&binary, &["history"])["cursor"], 0);
    db_json(&binary, &["show"]);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    db_json(&binary, &["name", ENTRY, "chosen"]);
    let path = db_path(&binary);
    let mut invalid = db_json(&binary, &["show"]);
    invalid["history"]["cursor"] = serde_json::json!(999);
    let bytes = serde_json::to_vec(&invalid).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let output = cli()
        .arg("db")
        .arg(&binary)
        .args(["undo", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn a_busy_writer_emits_no_success_and_leaves_committed_bytes_unchanged() {
    let (_dir, binary) = sample("busy.exe");
    db_json(&binary, &["name", ENTRY, "original"]);
    let path = db_path(&binary);
    let original = std::fs::read(&path).unwrap();
    let guard = fission_project::ProjectLock::acquire(&path).unwrap();
    let output = cli()
        .arg("db")
        .arg(&binary)
        .args(["name", ENTRY, "pending", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("project is being edited"));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    // A reader still sees the last committed file while a writer is active.
    assert_eq!(db_json(&binary, &["show"])["names"][ENTRY], "original");
    drop(guard);
    assert_eq!(
        db_json(&binary, &["name", ENTRY, "committed"])["revision"],
        2
    );
}

#[test]
#[cfg(unix)]
fn failed_save_has_no_success_payload_and_preserves_the_target() {
    use std::os::unix::fs::symlink;
    let (dir, binary) = sample("failed-save.exe");
    db_json(&binary, &["name", ENTRY, "original"]);
    let path = db_path(&binary);
    let target = dir.path().join("target.json");
    std::fs::rename(&path, &target).unwrap();
    let bytes = std::fs::read(&target).unwrap();
    symlink(&target, &path).unwrap();
    let output = cli()
        .arg("db")
        .arg(&binary)
        .args(["name", ENTRY, "pending", "--json"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("symbolic link"));
    assert_eq!(std::fs::read(&target).unwrap(), bytes);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3);
}
