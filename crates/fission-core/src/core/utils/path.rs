//! Path utilities

use std::path::PathBuf;

/// Find project workspace root by looking for markers
pub fn find_workspace_root(env_var: &str) -> Option<PathBuf> {
    // 1. Check environment variable
    if let Ok(root) = std::env::var(env_var) {
        let path = PathBuf::from(root);
        if path.exists() {
            return Some(path);
        }
    }

    // 2. Search upward from current directory
    let cwd = std::env::current_dir().ok()?;
    let mut current = cwd.as_path();

    loop {
        // Check for Fission workspace markers
        if current.join("Cargo.toml").exists() && current.join("crates").is_dir() {
            return Some(current.to_path_buf());
        }
        if current.join("ghidra_decompiler").is_dir() && current.join("utils").is_dir() {
            return Some(current.to_path_buf());
        }

        current = current.parent()?;
    }
}
