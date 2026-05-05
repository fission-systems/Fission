//! Resource discovery for vendored Detect-It-Easy signatures.

use std::fs;
use std::path::{Path, PathBuf};

/// Locate the DIE `.sg` mirror root via centralized [`fission_core::PATHS`] resolution.
pub(crate) fn detect_it_easy_mirror_root() -> Option<PathBuf> {
    fission_core::PATHS.die_mirror_root()
}

pub(super) fn collect_sg_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_sg_files(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("sg") {
            out.push(path);
        }
    }
}
