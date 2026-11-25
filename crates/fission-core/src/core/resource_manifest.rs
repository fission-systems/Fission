//! Which release a binary and its resource bundle come from (#1208).
//!
//! A release build embeds its tag and commit ([`BuildIdentity`], set by the
//! binary from `FISSION_BUILD_TAG` / `FISSION_BUILD_SHA` at compile time).
//! Packaging writes [`RESOURCE_MANIFEST_FILE`] into the bundle root
//! (`share/fission/`) with the same identity and per-directory content
//! digests (`scripts/release/resource_manifest.py`). This module compares the
//! two identities. It never hashes the bundle: the digests are for an
//! install-time or release-time check, not for every invocation.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// File name of the manifest in a resource bundle root.
pub const RESOURCE_MANIFEST_FILE: &str = "fission-resources.json";
/// The manifest layout this binary reads.
pub const RESOURCE_MANIFEST_SCHEMA: u64 = 1;

/// The release a binary was built from. `tag` and `sha` are `None` for a
/// build made outside the release workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuildIdentity {
    pub version: String,
    pub tag: Option<String>,
    pub sha: Option<String>,
}

impl BuildIdentity {
    /// Build an identity from compile-time values; empty strings count as unset.
    #[must_use]
    pub fn new(version: &str, tag: Option<&str>, sha: Option<&str>) -> Self {
        let set = |v: Option<&str>| v.map(str::trim).filter(|v| !v.is_empty()).map(String::from);
        Self {
            version: version.to_string(),
            tag: set(tag),
            sha: set(sha),
        }
    }
}

/// How a resolved resource bundle relates to the running binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestState {
    /// The manifest names this binary's version, and its tag and commit where both are known.
    Match,
    /// The manifest names another version, tag or commit.
    Mismatch,
    /// No bundle was resolved, or it has no manifest (a source checkout, custom
    /// resources, or a bundle older than the manifest).
    Missing,
    /// The manifest exists but cannot be read as a schema this binary knows.
    Invalid,
}

/// `resource_manifest` in `resources status --json`.
#[derive(Debug, Clone, Serialize)]
pub struct ResourceManifestStatus {
    pub path: Option<String>,
    pub state: ManifestState,
    pub detail: String,
    pub version: Option<String>,
    pub tag: Option<String>,
    pub sha: Option<String>,
}

#[derive(Deserialize)]
struct ManifestIdentity {
    schema: u64,
    fission_version: String,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    sha: Option<String>,
}

/// Compare a manifest's text with the binary's identity.
#[must_use]
pub fn compare_manifest(
    path: Option<String>,
    text: &str,
    build: &BuildIdentity,
) -> ResourceManifestStatus {
    let parsed = match serde_json::from_str::<ManifestIdentity>(text) {
        Ok(parsed) => parsed,
        Err(error) => {
            return status(
                path,
                ManifestState::Invalid,
                format!("unreadable manifest: {error}"),
            );
        }
    };
    if parsed.schema != RESOURCE_MANIFEST_SCHEMA {
        return status(
            path,
            ManifestState::Invalid,
            format!(
                "manifest schema {} is not {RESOURCE_MANIFEST_SCHEMA}",
                parsed.schema
            ),
        );
    }
    let identity = BuildIdentity::new(
        &parsed.fission_version,
        parsed.tag.as_deref(),
        parsed.sha.as_deref(),
    );
    let mut differences = Vec::new();
    if identity.version != build.version {
        differences.push(format!(
            "version {} (binary {})",
            identity.version, build.version
        ));
    }
    for (what, theirs, ours) in [
        ("tag", &identity.tag, &build.tag),
        ("commit", &identity.sha, &build.sha),
    ] {
        if let (Some(theirs), Some(ours)) = (theirs, ours)
            && theirs != ours
        {
            differences.push(format!("{what} {theirs} (binary {ours})"));
        }
    }
    let (state, detail) = if differences.is_empty() {
        let detail = if build.sha.is_some() && identity.sha.is_some() {
            "bundle and binary come from the same release commit".to_string()
        } else {
            "versions match; the commit was not compared because one side does not record it"
                .to_string()
        };
        (ManifestState::Match, detail)
    } else {
        (
            ManifestState::Mismatch,
            format!("bundle is for {}", differences.join(", ")),
        )
    };
    ResourceManifestStatus {
        path,
        state,
        detail,
        version: Some(identity.version),
        tag: identity.tag,
        sha: identity.sha,
    }
}

/// Read and compare the manifest of `bundle_root`, the bundle the resources resolved from.
#[must_use]
pub fn manifest_status(
    bundle_root: Option<&Path>,
    build: &BuildIdentity,
) -> ResourceManifestStatus {
    let Some(root) = bundle_root else {
        return status(
            None,
            ManifestState::Missing,
            "no installed resource bundle was resolved (source checkout or custom resources)"
                .to_string(),
        );
    };
    let path = root.join(RESOURCE_MANIFEST_FILE);
    let shown = Some(path.display().to_string());
    match std::fs::read_to_string(&path) {
        Ok(text) => compare_manifest(shown, &text, build),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => status(
            shown,
            ManifestState::Missing,
            format!("{} has no {RESOURCE_MANIFEST_FILE}", root.display()),
        ),
        Err(error) => status(
            shown,
            ManifestState::Invalid,
            format!("unreadable manifest: {error}"),
        ),
    }
}

fn status(path: Option<String>, state: ManifestState, detail: String) -> ResourceManifestStatus {
    ResourceManifestStatus {
        path,
        state,
        detail,
        version: None,
        tag: None,
        sha: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(tag: Option<&str>, sha: Option<&str>) -> BuildIdentity {
        BuildIdentity::new("0.5.0", tag, sha)
    }

    fn manifest(version: &str, tag: &str, sha: &str) -> String {
        format!(
            r#"{{"schema": 1, "fission_version": "{version}", "tag": "{tag}", "sha": "{sha}", "resources": {{}}}}"#
        )
    }

    #[test]
    fn the_same_release_matches() {
        let s = compare_manifest(
            None,
            &manifest("0.5.0", "v0.5.0", "abc"),
            &build(Some("v0.5.0"), Some("abc")),
        );
        assert_eq!(s.state, ManifestState::Match, "{}", s.detail);
        assert_eq!(s.sha.as_deref(), Some("abc"));
    }

    #[test]
    fn another_commit_tag_or_version_is_a_mismatch() {
        let ours = build(Some("v0.5.0"), Some("abc"));
        for (text, needle) in [
            (manifest("0.5.0", "v0.5.0", "def"), "commit def"),
            (manifest("0.5.0", "v0.5.1", "abc"), "tag v0.5.1"),
            (manifest("0.4.0", "v0.5.0", "abc"), "version 0.4.0"),
        ] {
            let s = compare_manifest(None, &text, &ours);
            assert_eq!(s.state, ManifestState::Mismatch, "{text}");
            assert!(s.detail.contains(needle), "{}", s.detail);
        }
    }

    #[test]
    fn a_build_without_a_commit_compares_the_version_only() {
        let s = compare_manifest(
            None,
            &manifest("0.5.0", "v0.5.0", "abc"),
            &build(None, Some("")),
        );
        assert_eq!(s.state, ManifestState::Match);
        assert!(s.detail.contains("not compared"), "{}", s.detail);
        let s = compare_manifest(
            None,
            &manifest("0.4.0", "v0.4.0", "abc"),
            &build(None, None),
        );
        assert_eq!(s.state, ManifestState::Mismatch);
    }

    #[test]
    fn an_unknown_schema_or_garbage_is_invalid() {
        let ours = build(None, None);
        let newer = r#"{"schema": 2, "fission_version": "0.5.0"}"#;
        assert_eq!(
            compare_manifest(None, newer, &ours).state,
            ManifestState::Invalid
        );
        assert_eq!(
            compare_manifest(None, "{", &ours).state,
            ManifestState::Invalid
        );
    }

    #[test]
    fn a_bundle_without_a_manifest_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let ours = build(None, None);
        assert_eq!(manifest_status(None, &ours).state, ManifestState::Missing);
        let s = manifest_status(Some(dir.path()), &ours);
        assert_eq!(s.state, ManifestState::Missing);
        std::fs::write(
            dir.path().join(RESOURCE_MANIFEST_FILE),
            manifest("0.5.0", "v0.5.0", "abc"),
        )
        .unwrap();
        assert_eq!(
            manifest_status(Some(dir.path()), &ours).state,
            ManifestState::Match
        );
    }
}
