//! `sandbox --indirect-trace-out`, `evidence import` and `evidence explain`,
//! as an operator runs them.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../fission-emulator/testdata/indirect_dispatch_x64.elf")
}

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(args)
        .env_remove("FISSION_CONTRACT")
        .output()
        .expect("run fission_cli")
}

fn json(args: &[&str]) -> Value {
    let output = cli(args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "{args:?}: {error}: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn text(path: &Path) -> &str {
    path.to_str().expect("utf-8 path")
}

fn function_address(name: &str) -> String {
    let listed = json(&["list", text(&fixture()), "--json"]);
    listed
        .as_array()
        .expect("functions")
        .iter()
        .find(|f| f["name"] == name)
        .unwrap_or_else(|| panic!("no {name} in {listed}"))["address"]
        .as_str()
        .expect("address")
        .to_string()
}

#[test]
fn two_recorded_runs_explain_two_targets_and_decomp_is_unaffected() {
    let directory = tempfile::tempdir().expect("tempdir");
    let binary = fixture();
    let first = directory.path().join("first.json");
    let second = directory.path().join("second.json");
    let store = directory.path().join("evidence.json");
    let addr = function_address("dispatch_call");
    let decomp = |label: &str| {
        let result = json(&["decomp", text(&binary), "--addr", &addr, "--json"]);
        let code = result[0]["code"].as_str().unwrap_or_else(|| {
            panic!("{label}: no code in {result}");
        });
        code.to_string()
    };
    let before = decomp("before any evidence");

    for (trace, arguments) in [(&first, &[][..]), (&second, &["other"][..])] {
        let mut args = vec![
            "sandbox",
            text(&binary),
            "--json",
            "--indirect-trace-out",
            text(trace),
        ];
        if !arguments.is_empty() {
            args.push("--");
            args.extend_from_slice(arguments);
        }
        json(&args);
        assert!(trace.exists(), "no trace at {}", trace.display());
    }

    let imported = json(&[
        "evidence",
        "import",
        text(&binary),
        text(&first),
        "--store",
        text(&store),
        "--json",
    ]);
    assert_eq!(imported["claims"], 2);
    assert_eq!(imported["traces"][0]["added"], 2);
    let explain = |store: &Path| {
        json(&[
            "evidence",
            "explain",
            text(&binary),
            "--addr",
            &addr,
            "--store",
            text(store),
            "--json",
        ])
    };
    let one = explain(&store);
    assert_eq!(one["sites"][0]["target_set"], "open");
    assert_eq!(
        one["sites"][0]["observed_candidates"]
            .as_array()
            .map(Vec::len),
        Some(1)
    );

    // Importing the first again adds nothing; the second adds its own.
    let imported = json(&[
        "evidence",
        "import",
        text(&binary),
        text(&first),
        text(&second),
        "--store",
        text(&store),
        "--json",
    ]);
    assert_eq!(imported["claims"], 4);
    assert_eq!(imported["traces"][0]["added"], 0);
    assert_eq!(imported["traces"][0]["duplicate"], 2);
    assert_eq!(imported["traces"][1]["added"], 2);

    let two = explain(&store);
    let names: Vec<&str> = two["sites"][0]["observed_candidates"]
        .as_array()
        .expect("candidates")
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();
    assert_eq!(names, ["op_increment", "op_triple"]);
    assert_eq!(two["sites"][0]["target_set"], "open");

    let readable = cli(&[
        "evidence",
        "explain",
        text(&binary),
        "--addr",
        &addr,
        "--store",
        text(&store),
    ]);
    let readable = String::from_utf8_lossy(&readable.stdout);
    assert!(readable.contains("target sets are open"), "{readable}");
    assert!(readable.contains("op_triple"), "{readable}");

    // The contract envelope carries the same view.
    let wrapped = json(&[
        "--contract",
        "v1",
        "evidence",
        "explain",
        text(&binary),
        "--addr",
        &addr,
        "--store",
        text(&store),
        "--json",
    ]);
    assert_eq!(wrapped["command"], "evidence");
    assert_eq!(wrapped["ok"], true);
    assert_eq!(wrapped["data"], two);

    assert_eq!(decomp("after importing evidence"), before);
}

#[test]
fn a_refused_trace_leaves_the_evidence_file_as_it_was() {
    let directory = tempfile::tempdir().expect("tempdir");
    let binary = fixture();
    let trace = directory.path().join("trace.json");
    let store = directory.path().join("evidence.json");
    json(&[
        "sandbox",
        text(&binary),
        "--json",
        "--indirect-trace-out",
        text(&trace),
    ]);
    let other = binary.with_file_name("indirect_dispatch_aarch64.elf");
    let refused = cli(&[
        "evidence",
        "import",
        text(&other),
        text(&trace),
        "--store",
        text(&store),
    ]);
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("another image"),
        "{}",
        String::from_utf8_lossy(&refused.stderr)
    );
    assert!(!store.exists(), "nothing was written");
}

#[test]
fn forced_paths_cannot_be_recorded_as_observations() {
    let refused = cli(&[
        "sandbox",
        text(&fixture()),
        "--sym-explore",
        "--indirect-trace-out",
        "/nonexistent/trace.json",
    ]);
    assert_eq!(refused.status.code(), Some(2));
}
