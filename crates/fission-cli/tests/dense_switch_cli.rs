//! A switch whose jump table a range check proves to be 300 entries long keeps
//! every case; recovery used to stop at 256.

use serde_json::Value;
use std::process::Command;

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

#[test]
fn a_range_checked_table_longer_than_256_entries_keeps_every_case() {
    let fixture = format!(
        "{}/../fission-sleigh/testdata/x64_dense_switch_300.so",
        env!("CARGO_MANIFEST_DIR")
    );
    let functions: Value =
        serde_json::from_slice(&cli(&["list", &fixture, "--json", "--no-db"])).expect("list json");
    let address = functions
        .as_array()
        .expect("function list")
        .iter()
        .find(|f| f["name"] == "dispatch")
        .and_then(|f| f["address"].as_str())
        .expect("dispatch")
        .to_string();

    let body: Value = serde_json::from_slice(&cli(&[
        "decomp",
        &fixture,
        "--addr",
        &address,
        "--json",
        "--no-db",
        "--timeout-ms",
        "60000",
    ]))
    .expect("decomp json");
    let code = body[0]["code"].as_str().expect("code");

    let labels: std::collections::BTreeSet<u32> = code
        .lines()
        .filter_map(|line| line.trim().strip_prefix("case "))
        .filter_map(|rest| rest.strip_suffix(':'))
        .filter_map(|n| n.parse().ok())
        .collect();
    // Case 0 is the table's first entry; it may print as the guard's `default`.
    assert!(
        labels.len() >= 298 && labels.contains(&299),
        "{} case labels, highest {:?}",
        labels.len(),
        labels.iter().next_back()
    );
}
