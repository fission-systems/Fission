//! Resource discovery for vendored Detect-It-Easy signatures.

use std::fs;
use std::path::{Path, PathBuf};

/// Locate the checked-in DIE mirror without depending on the vendor source tree.
pub(crate) fn detect_it_easy_mirror_root() -> Option<PathBuf> {
    let suffix = Path::new("utils")
        .join("signatures")
        .join("die")
        .join("detect-it-easy");

    if let Some(die_path) = fission_core::PATHS.get_die_signatures_path()
        && let Some(die_dir) = die_path.parent()
    {
        let candidate = die_dir.join("detect-it-easy");
        if candidate.is_dir() {
            return Some(candidate);
        }
    }

    let mut roots = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        roots.push(parent.to_path_buf());
    }

    for root in roots {
        for dir in root.ancestors() {
            let candidate = dir.join(&suffix);
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }

    None
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
