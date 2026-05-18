//! Path utilities

use std::path::{Path, PathBuf};

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
    if let Ok(cwd) = std::env::current_dir()
        && let Some(root) = find_workspace_root_from(&cwd)
    {
        return Some(root);
    }

    // 3. GUI/app launches often have a cwd outside the repo, while the dev
    // executable still lives under target/{debug,release}. Search from it too.
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
        && let Some(root) = find_workspace_root_from(exe_dir)
    {
        return Some(root);
    }

    None
}

fn find_workspace_root_from(start: &Path) -> Option<PathBuf> {
    let mut current = start;

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

#[cfg(test)]
mod tests {
    use super::find_workspace_root_from;
    use std::path::Path;

    #[test]
    fn workspace_root_resolves_from_nested_crate_path() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("workspace root");
        let nested = root.join("crates").join("fission-core").join("src");
        assert_eq!(find_workspace_root_from(&nested).as_deref(), Some(root));
    }
}
