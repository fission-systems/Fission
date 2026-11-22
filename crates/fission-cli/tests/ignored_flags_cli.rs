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

#[test]
fn decomp_profile_is_reported_as_ignored() {
    let binary = fixture();
    let (_, stderr) = run(&[
        "decomp",
        binary.to_str().unwrap(),
        "--addr",
        "0x1002030",
        "--no-db",
        "--profile",
        "quality",
    ]);
    assert!(
        stderr.contains("--profile") && stderr.contains("ignored by `decomp`"),
        "expected a warning naming --profile:\n{stderr}"
    );
}

#[test]
fn an_inventory_flag_names_inventory_not_decomp() {
    let binary = fixture();
    let (_, stderr) = run(&[
        "inventory",
        "function-facts",
        binary.to_str().unwrap(),
        "--functions-limit",
        "1",
        "--output-jsonl",
        "/dev/null",
        "--summary-json",
        "/dev/null",
        "--compiler-id",
        "gcc",
    ]);
    assert!(
        stderr.contains("`--compiler-id` is accepted but ignored by `inventory`"),
        "{stderr}"
    );
}

/// A hardware breakpoint is not set on any host, so the command must not
/// exit 0 as if it had been.
#[test]
fn hw_bp_fails_instead_of_reporting_success() {
    let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(["debug", "--emulator", "hw-bp", "0x1000"])
        .output()
        .expect("run the CLI");
    assert!(!output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("hardware breakpoints"));
}
