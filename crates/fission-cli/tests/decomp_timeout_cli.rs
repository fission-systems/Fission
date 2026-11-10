//! `--timeout-ms 0` means "no timeout".
//!
//! The help says so, and the worker layer honors it (`Some(0)` is filtered to
//! no deadline). The NIR renderer it hands the value to reads `Some(0)` as a
//! deadline that has already passed, so with `--timeout-ms 0` every function
//! failed with `nir_timeout ... after 0ms`.
use std::process::Command;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../fission-emulator/testdata/win_x64_write.exe"
);
const ENTRY: &str = "0x140001000";

fn decomp(extra: &[&str]) -> (bool, String) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("sample.exe");
    std::fs::copy(FIXTURE, &path).expect("copy fixture");
    let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .arg("decomp")
        .arg(&path)
        .args(["--addr", ENTRY, "--no-db"])
        .args(extra)
        .output()
        .expect("run the CLI");
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    (output.status.success(), text)
}

/// The control: no flag decompiles the entry point.
#[test]
fn without_the_flag_the_entry_point_decompiles() {
    let (ok, text) = decomp(&[]);
    assert!(ok, "{text}");
    assert!(!text.contains("timed out"), "{text}");
    assert!(!text.contains("Error decompiling"), "{text}");
}

#[test]
fn a_zero_timeout_means_no_timeout() {
    let (ok, text) = decomp(&["--timeout-ms", "0"]);
    assert!(
        !text.contains("timed out after 0ms"),
        "--timeout-ms 0 is documented as no timeout but every function timed out: {text}"
    );
    assert!(!text.contains("Error decompiling"), "{text}");
    assert!(ok, "{text}");
}

/// The flag still limits when it is positive and generous: a one-minute
/// budget is not hit by a small function.
#[test]
fn a_generous_timeout_still_decompiles() {
    let (ok, text) = decomp(&["--timeout-ms", "60000"]);
    assert!(ok, "{text}");
    assert!(!text.contains("timed out"), "{text}");
}
