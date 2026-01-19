//! File system utilities

use std::path::{Path, PathBuf};

/// Find first existing directory from candidates
pub fn find_existing_dir(candidates: &[impl AsRef<Path>]) -> Option<PathBuf> {
    for candidate in candidates {
        let path = candidate.as_ref();
        if path.exists() && path.is_dir() {
            return Some(path.to_path_buf());
        }
    }
    None
}

/// Find a file within search paths
pub fn find_file_in_dirs(dirs: &[impl AsRef<Path>], filename: &str) -> Option<PathBuf> {
    for dir in dirs {
        let path = dir.as_ref().join(filename);
        if path.exists() && path.is_file() {
            return Some(path);
        }
    }
    None
}

/// Check if a path exists and is a file
pub fn is_file(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    path.exists() && path.is_file()
}

/// Check if a path exists and is a directory
pub fn is_dir(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    path.exists() && path.is_dir()
}
