use super::*;

pub fn spec_root_for_arch(arch: &str) -> PathBuf {
    let arch = canonical_processor_name(arch).unwrap_or_else(|| arch.to_string());
    sleigh_languages_root().join(arch)
}

pub fn ghidra_language_manifest_path() -> PathBuf {
    sleigh_specs_root().join(GHIDRA_LANGUAGE_MANIFEST_FILE)
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("fission-sleigh crate should live under repo/crates/fission-sleigh")
        .to_path_buf()
}

pub fn sleigh_specs_root() -> PathBuf {
    if let Some(path) = env::var_os("FISSION_SLEIGH_SPEC_DIR") {
        let path = PathBuf::from(path);
        return normalize_sleigh_specs_root(path);
    }

    let repo_root = repo_root();
    let utils_root = repo_root.join("utils").join("sleigh-specs");
    if utils_root.join("languages").exists() {
        return utils_root;
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("specs")
}

pub fn sleigh_languages_root() -> PathBuf {
    sleigh_specs_root().join("languages")
}

fn normalize_sleigh_specs_root(path: PathBuf) -> PathBuf {
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name == "languages")
        .unwrap_or(false)
    {
        return path.parent().unwrap_or(&path).to_path_buf();
    }
    path
}

pub fn sleigh_build_cache_root() -> PathBuf {
    if let Some(path) = env::var_os("FISSION_SLEIGH_CACHE_DIR") {
        return PathBuf::from(path);
    }

    if let Some(path) = env::var_os("CARGO_TARGET_DIR") {
        let target = PathBuf::from(path);
        return if target.is_absolute() {
            target.join("fission-sleigh")
        } else {
            repo_root().join(target).join("fission-sleigh")
        };
    }

    repo_root().join("target").join("fission-sleigh")
}

pub fn generated_root() -> PathBuf {
    sleigh_build_cache_root().join("generated")
}

pub fn generated_root_for_arch(arch: &str) -> PathBuf {
    generated_root().join(canonical_processor_name(arch).unwrap_or_else(|| arch.to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhidraInstallPaths {
    pub install_root: PathBuf,
    pub processors_root: PathBuf,
}

pub fn resolve_ghidra_install_paths() -> Option<GhidraInstallPaths> {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)?
        .to_path_buf();
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("FISSION_GHIDRA_DIR") {
        candidates.push(PathBuf::from(path));
    }
    if let Some(path) = env::var_os("GHIDRA_INSTALL_DIR") {
        candidates.push(PathBuf::from(path));
    }
    candidates.extend([
        repo_root.join("vendor/ghidra/ghidra_12.0.4_PUBLIC"),
        repo_root.join("vendor/ghidra/ghidra-Ghidra_12.0.4_build"),
        repo_root.join("ghidra_12.0.4_PUBLIC"),
        repo_root.join("ghidra-Ghidra_12.0.4_build"),
    ]);

    candidates.into_iter().find_map(|candidate| {
        let install_root = normalize_ghidra_install_root(candidate);
        let processors_root = install_root.join("Ghidra").join("Processors");
        if processors_root.exists() {
            Some(GhidraInstallPaths {
                install_root,
                processors_root,
            })
        } else {
            None
        }
    })
}

fn normalize_ghidra_install_root(path: PathBuf) -> PathBuf {
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name == "Ghidra")
        .unwrap_or(false)
        && path.join("Processors").exists()
    {
        path.parent().unwrap_or(&path).to_path_buf()
    } else {
        path
    }
}

pub fn entry_id_from_path(entry_spec: &Path) -> Result<String> {
    let stem = entry_spec
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| anyhow!("entry spec {} has no UTF-8 file stem", entry_spec.display()))?;
    Ok(stem.to_string())
}

pub fn generated_root_for_entry_spec(entry_spec: &Path) -> Result<PathBuf> {
    let arch = infer_arch_from_entry_spec(entry_spec)?;
    let entry_id = entry_id_from_path(entry_spec)?;
    Ok(generated_root_for_arch(&arch).join(entry_id))
}

pub(super) fn generated_output_root_for_entry_spec(
    entry_spec: &Path,
    output_root: &Path,
) -> Result<PathBuf> {
    let arch = infer_arch_from_entry_spec(entry_spec)?;
    let entry_id = entry_id_from_path(entry_spec)?;
    if output_root
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name == entry_id)
        .unwrap_or(false)
    {
        return Ok(output_root.to_path_buf());
    }
    if output_root
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name == arch)
        .unwrap_or(false)
    {
        return Ok(output_root.join(entry_id));
    }
    Ok(output_root.join(arch).join(entry_id))
}

pub fn infer_arch_from_entry_spec(entry_spec: &Path) -> Result<String> {
    let parent = entry_spec.parent().ok_or_else(|| {
        anyhow!(
            "entry spec {} has no parent directory",
            entry_spec.display()
        )
    })?;
    let languages_roots = [
        sleigh_languages_root(),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("specs")
            .join("languages"),
    ];
    let mut last_error_root = None;
    for languages_root in languages_roots {
        last_error_root = Some(languages_root.clone());
        if let Ok(relative) = parent.strip_prefix(&languages_root) {
            let arch_dir = relative.components().next().ok_or_else(|| {
                anyhow!(
                    "missing arch directory for entry spec {}",
                    entry_spec.display()
                )
            })?;
            return Ok(arch_dir.as_os_str().to_string_lossy().into_owned());
        }
    }
    let languages_root = last_error_root.unwrap_or_else(sleigh_languages_root);
    bail!(
        "entry spec {} is outside compiler spec root {}",
        entry_spec.display(),
        languages_root.display()
    )
}

pub fn entry_spec_from_path(entry_spec: PathBuf) -> Result<EntrySpec> {
    let arch = infer_arch_from_entry_spec(&entry_spec)?;
    let entry_id = entry_id_from_path(&entry_spec)?;
    let entry_spec_name = entry_spec
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("entry spec {} has no UTF-8 file name", entry_spec.display()))?
        .to_string();
    Ok(EntrySpec {
        arch,
        path: entry_spec,
        entry_spec: entry_spec_name,
        entry_id,
        language_ids: Vec::new(),
        compatibility_aliases: Vec::new(),
    })
}
