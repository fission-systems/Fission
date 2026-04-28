use super::*;

pub(super) fn manifest_entry_for_entry_id(
    entry_id: &str,
) -> Result<Option<GhidraLanguageManifestEntry>> {
    let manifest_path = ghidra_language_manifest_path();
    if !manifest_path.exists() {
        return Ok(None);
    }
    let manifest: GhidraLanguageManifest = serde_json::from_str(
        &fs::read_to_string(&manifest_path)
            .with_context(|| format!("read manifest {}", manifest_path.display()))?,
    )
    .with_context(|| format!("parse manifest {}", manifest_path.display()))?;
    Ok(manifest
        .entries
        .into_iter()
        .find(|entry| entry.entry_id == entry_id))
}

pub(super) fn runtime_status_for_entry(entry_id: &str) -> Result<String> {
    Ok(manifest_entry_for_entry_id(entry_id)?
        .map(|entry| entry.runtime_status)
        .unwrap_or_else(|| "registered_compile_only".to_string()))
}

pub(super) fn is_executable_candidate_entry(entry_id: &str) -> Result<bool> {
    Ok(runtime_status_for_entry(entry_id)? == "executable_candidate")
}

pub(super) fn compatibility_aliases_for(processor: &str) -> Vec<String> {
    let manifest_path = ghidra_language_manifest_path();
    if !manifest_path.exists() {
        return Vec::new();
    }
    let Ok(contents) = fs::read_to_string(&manifest_path) else {
        return Vec::new();
    };
    let Ok(manifest) = serde_json::from_str::<GhidraLanguageManifest>(&contents) else {
        return Vec::new();
    };
    let mut aliases = BTreeSet::new();
    for entry in manifest
        .entries
        .into_iter()
        .filter(|entry| entry.processor == processor)
    {
        aliases.extend(entry.compatibility_aliases);
    }
    aliases.into_iter().collect()
}

pub(super) fn canonical_processor_name(name: &str) -> Option<String> {
    let manifest_path = ghidra_language_manifest_path();
    let contents = fs::read_to_string(manifest_path).ok()?;
    let manifest = serde_json::from_str::<GhidraLanguageManifest>(&contents).ok()?;
    manifest.entries.into_iter().find_map(|entry| {
        if entry.processor == name
            || entry
                .compatibility_aliases
                .iter()
                .any(|alias| alias == name)
        {
            Some(entry.processor)
        } else {
            None
        }
    })
}
