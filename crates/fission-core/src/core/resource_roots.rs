//! Runtime resource bundle roots (`fission-data/`, installs, user data) ahead of workspace-relative signatures.
//!
//! CLI [`set_cli_resource_bundle_root`] must run before the first [`crate::PATHS`] access when using `--resource-root`.

use crate::core::path_config::PathConfig;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

static CLI_RESOURCE_BUNDLE_ROOT: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Process-global override from `--resource-root`. Must be set before the first [`crate::PATHS`] dereference if used.
pub fn set_cli_resource_bundle_root(path: Option<PathBuf>) {
    let mut guard = CLI_RESOURCE_BUNDLE_ROOT
        .write()
        .expect("resource root lock poisoned");
    *guard = path;
}

#[must_use]
pub fn cli_resource_bundle_root() -> Option<PathBuf> {
    CLI_RESOURCE_BUNDLE_ROOT
        .read()
        .expect("resource root lock poisoned")
        .clone()
}

#[must_use]
pub fn env_fission_resource_root() -> Option<PathBuf> {
    std::env::var_os("FISSION_RESOURCE_ROOT").map(PathBuf::from)
}

/// Ordered bundle roots tried before workspace-relative signatures (CLI → env → exe → user).
#[must_use]
pub fn prioritized_bundle_roots() -> Vec<PathBuf> {
    explicit_bundle_roots()
        .into_iter()
        .chain(ambient_bundle_roots())
        .collect()
}

/// Operator-provided bundle roots. These are allowed to override workspace resources.
#[must_use]
pub fn explicit_bundle_roots() -> Vec<PathBuf> {
    let mut seen = Vec::new();
    let mut push_unique = |p: PathBuf| {
        if seen.iter().any(|x: &PathBuf| x == &p) {
            return;
        }
        seen.push(p);
    };

    if let Some(p) = cli_resource_bundle_root() {
        push_unique(p);
    }
    if let Some(p) = env_fission_resource_root() {
        push_unique(p);
    }
    seen
}

/// Auto-discovered install/user bundle roots. Use after workspace resources to
/// keep repo-local development deterministic and avoid slow user/system probes
/// when `utils/signatures` is available.
#[must_use]
pub fn ambient_bundle_roots() -> Vec<PathBuf> {
    let mut seen = Vec::new();
    let mut push_unique = |p: PathBuf| {
        if seen.iter().any(|x: &PathBuf| x == &p) {
            return;
        }
        seen.push(p);
    };

    for p in exe_adjacent_bundle_roots() {
        push_unique(p);
    }
    for p in user_data_bundle_roots() {
        push_unique(p);
    }
    seen
}

#[must_use]
pub fn signatures_base_from_bundle_root(bundle_root: &Path) -> Option<PathBuf> {
    let nested = bundle_root.join("signatures");
    if nested.is_dir() {
        return Some(nested);
    }
    // Tolerate layouts where the bundle root already points at the signatures tree.
    if bundle_root.join("fid").is_dir()
        || bundle_root.join("typeinfo").is_dir()
        || bundle_root.join("die").is_dir()
    {
        return Some(bundle_root.to_path_buf());
    }
    None
}

#[must_use]
pub fn resolve_signatures_base_from_bundles() -> Option<PathBuf> {
    resolve_signatures_base_from_roots(prioritized_bundle_roots())
}

#[must_use]
pub fn resolve_signatures_base_from_roots(
    roots: impl IntoIterator<Item = PathBuf>,
) -> Option<PathBuf> {
    roots
        .into_iter()
        .filter(|root| root.exists())
        .find_map(|root| signatures_base_from_bundle_root(&root))
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceRootProbe {
    pub kind: String,
    pub path: String,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceAvailability {
    pub signatures_base: Option<String>,
    pub workspace_root: Option<String>,
    #[serde(rename = "win32_typeinfo")]
    pub win32_typeinfo_dir: Option<String>,
    pub win32_typeinfo_present: bool,
    pub fid_dir: Option<String>,
    pub fid_present: bool,
    pub die_dir: Option<String>,
    pub die_corpus_present: bool,
    pub patterns_dir: Option<String>,
    pub patterns_present: bool,
    pub die_pe_signatures_json: Option<String>,
    pub die_pe_json_present: bool,
    pub win_api_pipe_text: Option<String>,
    pub win_api_pipe_text_present: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResourceStatusSnapshot {
    pub resource_roots: Vec<ResourceRootProbe>,
    pub resources: ResourceAvailability,
}

#[must_use]
pub fn probe_resource_root_candidates() -> Vec<ResourceRootProbe> {
    let mut out = Vec::new();

    if let Some(p) = cli_resource_bundle_root() {
        push_probe(&mut out, "cli_resource_root", p);
    }

    if let Some(raw) = std::env::var_os("FISSION_RESOURCE_ROOT") {
        let p = PathBuf::from(raw);
        push_probe(&mut out, "FISSION_RESOURCE_ROOT", p);
    }

    for (label, p) in exe_adjacent_bundle_roots_labeled() {
        push_probe(&mut out, label, p);
    }

    for (label, p) in user_data_bundle_roots_labeled() {
        push_probe(&mut out, label, p);
    }

    out
}

fn push_probe(out: &mut Vec<ResourceRootProbe>, kind: &str, path: PathBuf) {
    let exists = path.exists();
    out.push(ResourceRootProbe {
        kind: kind.to_string(),
        path: path.display().to_string(),
        exists,
    });
}

#[must_use]
pub fn resource_status_snapshot() -> ResourceStatusSnapshot {
    let cfg = PathConfig::detect();
    let roots = probe_resource_root_candidates();
    let die_pe = cfg.get_die_signatures_path();
    let die_corpus_present = cfg.die_dir.as_ref().is_some_and(|p| p.exists()) || die_pe.is_some();
    let win_api = cfg.get_win_api_signatures_path();
    let resources = ResourceAvailability {
        signatures_base: cfg.signatures_base.as_ref().map(path_display),
        workspace_root: cfg.workspace_root.as_ref().map(path_display),
        win32_typeinfo_dir: cfg.gdt_dir.as_ref().map(path_display),
        win32_typeinfo_present: cfg.gdt_dir.as_ref().is_some_and(|p| p.exists()),
        fid_dir: cfg.fid_dir.as_ref().map(path_display),
        fid_present: cfg.fid_dir.as_ref().is_some_and(|p| p.exists()),
        die_dir: cfg.die_dir.as_ref().map(path_display),
        die_corpus_present,
        patterns_dir: cfg.patterns_dir.as_ref().map(path_display),
        patterns_present: cfg.patterns_dir.as_ref().is_some_and(|p| p.exists()),
        die_pe_signatures_json: die_pe.as_ref().map(path_display),
        die_pe_json_present: die_pe.is_some(),
        win_api_pipe_text: win_api.as_ref().map(path_display),
        win_api_pipe_text_present: win_api.is_some(),
    };
    ResourceStatusSnapshot {
        resource_roots: roots,
        resources,
    }
}

fn path_display(p: impl AsRef<Path>) -> String {
    p.as_ref().display().to_string()
}

fn exe_adjacent_bundle_roots_labeled() -> Vec<(&'static str, PathBuf)> {
    let Ok(exe) = std::env::current_exe() else {
        return Vec::new();
    };
    let Some(parent) = exe.parent() else {
        return Vec::new();
    };

    let mut v = Vec::new();
    v.push(("exe_dir/fission-data", parent.join("fission-data")));
    v.push((
        "exe_dir/share/fission",
        parent.join("share").join("fission"),
    ));
    #[cfg(unix)]
    {
        if let Some(gp) = parent.parent() {
            v.push((
                "exe_parent/../share/fission",
                gp.join("share").join("fission"),
            ));
        }
        v.push((
            "system_/usr/share/fission",
            PathBuf::from("/usr/share/fission"),
        ));
        v.push((
            "system_/usr/local/share/fission",
            PathBuf::from("/usr/local/share/fission"),
        ));
    }
    #[cfg(windows)]
    {
        if let Some(gp) = parent.parent() {
            v.push(("exe_parent/share/fission", gp.join("share").join("fission")));
        }
    }
    v
}

fn exe_adjacent_bundle_roots() -> Vec<PathBuf> {
    exe_adjacent_bundle_roots_labeled()
        .into_iter()
        .map(|(_, p)| p)
        .collect()
}

fn user_data_bundle_roots_labeled() -> Vec<(&'static str, PathBuf)> {
    let mut v = Vec::new();

    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        let root = PathBuf::from(xdg).join("fission").join("resources");
        v.push(("XDG_DATA_HOME/fission/resources", root));
    }

    if let Some(d) = dirs::data_local_dir() {
        let root = d.join("fission").join("resources");
        v.push(("dirs_data_local/fission/resources", root));
    }

    #[cfg(windows)]
    if let Some(d) = dirs::data_dir() {
        let root = d.join("Fission").join("resources");
        v.push(("dirs_data_dir/Fission/resources", root));
    }

    #[cfg(not(windows))]
    if let Some(d) = dirs::data_dir() {
        let root = d.join("fission").join("resources");
        v.push(("dirs_data_dir/fission/resources", root));
    }

    v
}

fn user_data_bundle_roots() -> Vec<PathBuf> {
    user_data_bundle_roots_labeled()
        .into_iter()
        .map(|(_, p)| p)
        .collect()
}

#[cfg(test)]
mod resource_bundle_tests {
    use super::*;
    use crate::core::path_config::PathConfig;
    use std::fs;
    use std::sync::Mutex;

    /// Serialize tests that touch [`PathConfig::detect`] / env / CLI override global state.
    static DETECT_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn bundle_root_nested_signatures_resolves() {
        let dir = tempfile::tempdir().unwrap();
        let sig = dir.path().join("signatures");
        fs::create_dir_all(sig.join("fid")).unwrap();
        let got = signatures_base_from_bundle_root(dir.path()).expect("signatures base");
        assert_eq!(got, sig);
    }

    #[test]
    fn cli_override_selects_signatures_base() {
        let _guard = DETECT_LOCK.lock().expect("detect lock poisoned");

        let dir = tempfile::tempdir().unwrap();
        let sig = dir.path().join("signatures");
        fs::create_dir_all(&sig).unwrap();

        set_cli_resource_bundle_root(Some(dir.path().to_path_buf()));
        let cfg = PathConfig::detect();
        assert_eq!(cfg.signatures_base.as_ref(), Some(&sig));

        set_cli_resource_bundle_root(None);
    }

    #[test]
    fn env_fission_resource_root_selects_bundle() {
        let _guard = DETECT_LOCK.lock().expect("detect lock poisoned");

        set_cli_resource_bundle_root(None);

        let dir = tempfile::tempdir().unwrap();
        let sig = dir.path().join("signatures");
        fs::create_dir_all(&sig).unwrap();

        // SAFETY: serialized by `DETECT_LOCK`; no concurrent readers of this env var in tests.
        unsafe {
            std::env::set_var("FISSION_RESOURCE_ROOT", dir.path());
        }
        let cfg = PathConfig::detect();
        assert_eq!(cfg.signatures_base.as_ref(), Some(&sig));

        unsafe {
            std::env::remove_var("FISSION_RESOURCE_ROOT");
        }
    }
}
