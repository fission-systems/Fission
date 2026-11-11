//! A flag the CLI accepts but cannot honor says so on stderr instead of being
//! dropped silently.
use std::path::PathBuf;
use std::process::Command;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fission-sleigh/testdata/x64_static_switch_no_reloc.elf")
}

fn run(args: &[&str]) -> (String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(args)
        .output()
        .expect("run the CLI");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn a_dead_decomp_flag_is_reported_as_ignored() {
    let binary = fixture();
    let (_, stderr) = run(&[
        "decomp",
        binary.to_str().unwrap(),
        "--addr",
        "0x1002030",
        "--no-db",
        "--compiler-id",
        "gcc",
    ]);
    assert!(
        stderr.contains("--compiler-id") && stderr.contains("ignored"),
        "expected a warning naming --compiler-id:\n{stderr}"
    );
}

#[test]
fn a_global_flag_a_subcommand_cannot_honor_is_reported_as_ignored() {
    let binary = fixture();
    let (_, stderr) = run(&[
        "--language",
        "x86:LE:64:default",
        "verify",
        binary.to_str().unwrap(),
        "--addr",
        "0x1002030",
    ]);
    assert!(
        stderr.contains("--language") && stderr.contains("ignored"),
        "expected a warning naming --language:\n{stderr}"
    );
}

#[test]
fn a_flag_that_is_honored_produces_no_warning() {
    let binary = fixture();
    let (_, stderr) = run(&[
        "decomp",
        binary.to_str().unwrap(),
        "--addr",
        "0x1002030",
        "--no-db",
    ]);
    assert!(!stderr.contains("ignored"), "{stderr}");
}
