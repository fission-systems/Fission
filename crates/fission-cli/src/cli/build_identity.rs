//! The release this binary was built from (#1208).
//!
//! The release workflow (`cd.yml`) sets `FISSION_BUILD_TAG` and
//! `FISSION_BUILD_SHA` for the build; `option_env!` records them in the
//! binary, and Cargo rebuilds this crate when either changes. Any other
//! build leaves both unset and reports only its package version.

use fission_core::resource_manifest::BuildIdentity;
use std::sync::OnceLock;

/// This binary's identity.
#[must_use]
pub fn build_identity() -> BuildIdentity {
    BuildIdentity::new(
        env!("CARGO_PKG_VERSION"),
        option_env!("FISSION_BUILD_TAG"),
        option_env!("FISSION_BUILD_SHA"),
    )
}

/// `--version` text: the package version, then the tag and commit when known.
#[must_use]
pub fn long_version() -> &'static str {
    static TEXT: OnceLock<String> = OnceLock::new();
    TEXT.get_or_init(|| describe(&build_identity()))
}

fn describe(build: &BuildIdentity) -> String {
    match (&build.tag, &build.sha) {
        (Some(tag), Some(sha)) => format!("{} ({tag}, commit {sha})", build.version),
        (None, Some(sha)) => format!("{} (commit {sha})", build.version),
        (Some(tag), None) => format!("{} ({tag})", build.version),
        (None, None) => format!("{} (not a release build)", build.version),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_build_names_its_tag_and_commit() {
        let build = BuildIdentity::new("0.5.0", Some("v0.5.0"), Some("abc"));
        assert_eq!(describe(&build), "0.5.0 (v0.5.0, commit abc)");
        let dev = BuildIdentity::new("0.5.0", Some(""), None);
        assert_eq!(describe(&dev), "0.5.0 (not a release build)");
    }

    #[test]
    fn the_long_version_starts_with_the_package_version() {
        // Homebrew's formula test and install.sh match the version in it.
        assert!(long_version().starts_with(env!("CARGO_PKG_VERSION")));
    }
}
