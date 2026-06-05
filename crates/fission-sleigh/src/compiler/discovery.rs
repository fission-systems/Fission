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

    utils_root
}

pub fn sleigh_languages_root() -> PathBuf {
    sleigh_specs_root().join("languages")
}

/// Root of checked-in Ghidra compiled `.sla` artifacts (`utils/sleigh-specs/compiled/`).
pub fn sleigh_compiled_root() -> PathBuf {
    sleigh_specs_root().join("compiled")
}

fn normalize_sleigh_specs_root(path: PathBuf) -> PathBuf {
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "languages")
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

/// Returns true when checked-in compiled `.sla` artifacts exist under
/// `utils/sleigh-specs/compiled/`.
#[must_use]
pub fn checked_in_compiled_sla_available() -> bool {
    let root = sleigh_compiled_root();
    root.is_dir()
        && root
            .read_dir()
            .ok()
            .is_some_and(|mut entries| entries.flatten().next().is_some())
}

/// Resolve the checked-in packaged `.sla` for an entry spec stem under `compiled/<arch>/`.
pub fn packaged_sla_for_entry_spec(entry_spec: &Path) -> Result<Option<PathBuf>> {
    let stem = entry_id_from_path(entry_spec)?;
    let arch = infer_arch_from_entry_spec(entry_spec)?;
    let path = sleigh_compiled_root()
        .join(&arch)
        .join(format!("{stem}.sla"));
    Ok(path.is_file().then_some(path))
}

/// Same as [`packaged_sla_for_entry_spec`], but required for production lift frontends.
pub fn require_packaged_sla_for_entry_spec(entry_spec: &Path) -> Result<PathBuf> {
    let stem = entry_id_from_path(entry_spec)?;
    let arch = infer_arch_from_entry_spec(entry_spec)?;
    let path = sleigh_compiled_root()
        .join(&arch)
        .join(format!("{stem}.sla"));
    if path.is_file() {
        Ok(path)
    } else {
        Err(anyhow!(
            "missing checked-in compiled .sla for {} (expected {})",
            entry_spec.display(),
            path.display()
        ))
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
        .is_some_and(|name| name == entry_id)
    {
        return Ok(output_root.to_path_buf());
    }
    if output_root
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == arch)
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
        language_aliases: Vec::new(),
        processor_spec: None,
    })
}
