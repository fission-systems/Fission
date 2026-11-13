//! AArch64 `csel`/`cinc`/`cset` lift to `tmp = a; if (cond) goto L; tmp = b;
//! L: use(tmp)`. The skip is a p-code-internal label; read as a machine address
//! it printed a call to address 2 and dropped the select. The oracle is
//! `fission_cli verify`'s ground truth: the machine code run in the emulator
//! against the decompiled function, so the test does not depend on how the
//! pseudocode happens to be spelled.

use serde_json::Value;
use std::process::Command;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../fission-sleigh/testdata/a64_csel_select.so"
);

fn cli(args: &[&str]) -> Vec<u8> {
    let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(args)
        .env("FISSION_BENCHMARK_NO_CACHE", "1")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .expect("run fission_cli");
    assert!(output.status.success(), "{output:?}");
    output.stdout
}

fn address_of(name: &str) -> String {
    let functions: Value =
        serde_json::from_slice(&cli(&["list", FIXTURE, "--json", "--no-db"])).expect("list json");
    functions
        .as_array()
        .expect("function list")
        .iter()
        .find(|f| f["name"] == name)
        .and_then(|f| f["address"].as_str())
        .unwrap_or_else(|| panic!("{name} missing from the fixture"))
        .to_string()
}

#[test]
fn a_select_agrees_with_the_machine_code_and_calls_nothing() {
    for name in ["sel_zero", "sel_two", "sel_inc", "sel_cst", "count_char"] {
        let address = address_of(name);
        let body: Value = serde_json::from_slice(&cli(&[
            "decomp", FIXTURE, "--addr", &address, "--json", "--no-db",
        ]))
        .expect("decomp json");
        let code = body[0]["code"].as_str().expect("code");
        assert!(
            !code.contains("sub_"),
            "{name}: a select must not print a call:\n{code}"
        );

        let verdict = String::from_utf8(cli(&[
            "verify", FIXTURE, "--addr", &address, "--name", name,
        ]))
        .expect("utf8");
        let ground_truth = verdict
            .lines()
            .find(|line| line.starts_with("ground_truth:"))
            .unwrap_or_else(|| panic!("{name}: no ground_truth line in:\n{verdict}"));
        assert!(
            ground_truth.starts_with("ground_truth: Equivalent"),
            "{name}: decompiled function disagrees with the machine code: {ground_truth}\n{code}"
        );
    }
}
