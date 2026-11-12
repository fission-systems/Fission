//! `--signature-path`: a directory of FID databases the user supplies, searched
//! after the bundle. A path the user typed is never silently ignored.

use serde_json::Value;
use std::process::{Command, Output};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(args)
        .env_remove("FISSION_CONTRACT")
        .env_remove("FISSION_SIGNATURE_PATH")
        .output()
        .expect("run fission_cli")
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "not json ({e}): {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

#[test]
fn resources_status_reports_each_signature_path_and_its_databases() {
    let dir = std::env::temp_dir().join(format!("fission-sigpath-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    for name in ["one.fn.fpk", "one.lib.fpk", "two.fidbf", "readme.txt"] {
        std::fs::write(dir.join(name), b"x").expect("write");
    }
    let path = dir.to_str().expect("utf-8");

    let output = cli(&["--signature-path", path, "resources", "status", "--json"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let status = json(&output);
    let entry = &status["signature_paths"][0];
    assert_eq!(entry["path"], path);
    assert_eq!(entry["exists"], true);
    assert_eq!(entry["fid_databases"], 2);

    // The same directory through the environment.
    let via_env = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .args(["resources", "status", "--json"])
        .env("FISSION_SIGNATURE_PATH", path)
        .output()
        .expect("run");
    assert_eq!(json(&via_env)["signature_paths"][0]["fid_databases"], 2);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_signature_path_fails_with_input_not_found_and_exit_3() {
    let output = cli(&[
        "--contract",
        "v1",
        "--signature-path",
        "/definitely/not/a/signature/dir",
        "resources",
        "status",
        "--json",
    ]);
    let body = json(&output);
    assert_eq!(body["ok"], false, "{body}");
    assert_eq!(body["error"]["code"], "input_not_found");
    assert_eq!(output.status.code(), Some(3));
}
