//! [`ResourceProvider`] — single entry point for runtime resource paths.
//!
//! Resolution is delegated to [`crate::core::path_config::PathConfig`] (bundle roots, workspace, CWD fallbacks).

use std::path::PathBuf;

use super::path_config::{PATHS, PathConfig};

/// Process-global resource paths (FID, DIE, patterns, Win32 typeinfo).
#[derive(Clone, Debug)]
pub struct ResourceProvider {
    paths: PathConfig,
}

impl ResourceProvider {
    /// Snapshot from the lazily initialized global [`PATHS`].
    #[must_use]
    pub fn global() -> Self {
        Self {
            paths: PATHS.clone(),
        }
    }

    /// Fresh detection (use for diagnostics; prefer [`Self::global`] for steady-state runtime).
    #[must_use]
    pub fn detect() -> Self {
        Self {
            paths: PathConfig::detect(),
        }
    }

    #[must_use]
    pub fn from_paths(paths: PathConfig) -> Self {
        Self { paths }
    }

    #[must_use]
    pub fn paths(&self) -> &PathConfig {
        &self.paths
    }

    #[must_use]
    pub fn into_paths(self) -> PathConfig {
        self.paths
    }

    #[must_use]
    pub fn die_pe_signatures_json(&self) -> Option<PathBuf> {
        self.paths.get_die_signatures_path()
    }

    #[must_use]
    pub fn die_mirror_root(&self) -> Option<PathBuf> {
        self.paths.die_mirror_root()
    }

    #[must_use]
    pub fn win_api_signatures_txt(&self) -> Option<PathBuf> {
        self.paths.get_win_api_signatures_path()
    }

    #[must_use]
    pub fn ntoskrnl_signatures_txt(&self) -> Option<PathBuf> {
        self.paths.get_ntoskrnl_signatures_path()
    }

    #[must_use]
    pub fn generic_clib_signatures_txt(&self) -> Option<PathBuf> {
        self.paths.get_generic_clib_signatures_path()
    }

    #[must_use]
    pub fn generic_clib_64_signatures_txt(&self) -> Option<PathBuf> {
        self.paths.get_generic_clib_64_signatures_path()
    }

    #[must_use]
    pub fn mac_osx_signatures_txt(&self) -> Option<PathBuf> {
        self.paths.get_mac_osx_signatures_path()
    }

    #[must_use]
    pub fn win32_typeinfo_json_path(&self, filename: &str) -> Option<PathBuf> {
        self.paths.get_win32_typeinfo_json_path(filename)
    }
}
