//! `resources status` names the release the binary and its resource bundle
//! come from, and says when they disagree (#1208). release-tag.yml refuses a
//! release unless this reports `match` for the packaged binary.

use serde_json::Value;
use std::path::Path;
use std::process::Command;

fn status(resource_root: &Path) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_fission_cli"))
        .arg("--resource-root")
        .arg(resource_root)
        .args(["resources", "status", "--json"])
        .env_remove("FISSION_CONTRACT")
        .env_remove("FISSION_RESOURCE_ROOT")
        .env_remove("FISSION_SIGNATURE_PATH")
        .output()
        .expect("run fission_cli");
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    serde_json::from_slice(&output.stdout).expect("json")
}

fn bundle_with_manifest(version: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("temp dir");
    std::fs::create_dir(root.path().join("signatures")).expect("signatures");
    let manifest = serde_json::json!({
        "schema": 1,
        "fission_version": version,
        "tag": "v0.0.0-test",
        "sha": "0000000000000000000000000000000000000000",
        "resources": {},
    });
    std::fs::write(
        root.path().join("fission-resources.json"),
        serde_json::to_vec_pretty(&manifest).expect("serialize"),
    )
    .expect("write manifest");
    root
}

#[test]
fn a_bundle_for_this_version_matches() {
    let root = bundle_with_manifest(env!("CARGO_PKG_VERSION"));
    let status = status(root.path());
    assert_eq!(status["build"]["version"], env!("CARGO_PKG_VERSION"));
    let manifest = &status["resource_manifest"];
    assert_eq!(manifest["state"], "match", "{manifest}");
    assert_eq!(
        manifest["path"],
        root.path()
            .join("fission-resources.json")
            .display()
            .to_string()
    );
    assert_eq!(manifest["sha"], "0000000000000000000000000000000000000000");
}

#[test]
fn a_bundle_for_another_version_is_a_mismatch() {
    let root = bundle_with_manifest("0.0.0-another");
    let manifest = status(root.path())["resource_manifest"].clone();
    assert_eq!(manifest["state"], "mismatch", "{manifest}");
    assert!(
        manifest["detail"]
            .as_str()
            .is_some_and(|d| d.contains("version 0.0.0-another")),
        "{manifest}"
    );
}

#[test]
fn a_bundle_without_a_manifest_is_reported_missing() {
    let root = tempfile::tempdir().expect("temp dir");
    std::fs::create_dir(root.path().join("signatures")).expect("signatures");
    let manifest = status(root.path())["resource_manifest"].clone();
    assert_eq!(manifest["state"], "missing", "{manifest}");
}
