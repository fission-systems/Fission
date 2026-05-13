use super::*;

/// Explicit stems (`*.slaspec` basename without extension) supported as packaged-`.sla` runtime lifts.
///
/// This must **not** be derived by re-reading `ghidra_language_manifest.json`'s `runtime_status`:
/// manifest regeneration (`build_ghidra_language_manifest`) used that path historically and could
/// permanently promote every variant if the checked-in manifest was corrupted once.
const EXECUTABLE_CANDIDATE_ENTRY_IDS: &[&str] = &[
    // x86
    "x86-64",
    "x86",
    // AARCH64
    "AARCH64",
    "AARCH64BE",
    "AARCH64_AppleSilicon",
    // ARM (16)
    "ARM4_be",
    "ARM4_le",
    "ARM4t_be",
    "ARM4t_le",
    "ARM5_be",
    "ARM5_le",
    "ARM5t_be",
    "ARM5t_le",
    "ARM6_be",
    "ARM6_le",
    "ARM7_be",
    "ARM7_le",
    "ARM8_be",
    "ARM8_le",
    "ARM8m_be",
    "ARM8m_le",
    // MIPS (6)
    "mips32be",
    "mips32le",
    "mips32R6be",
    "mips32R6le",
    "mips64be",
    "mips64le",
    // RISCV (`andestar_v5` stays compile-only)
    "riscv.ilp32d",
    "riscv.lp64d",
    // LoongArch
    "loongarch32_f32",
    "loongarch32_f64",
    "loongarch64_f32",
    "loongarch64_f64",
    // PowerPC
    "ppc_32_be",
    "ppc_32_le",
    "ppc_64_le",
    // SPARC
    "SparcV9_64",
    // eBPF
    "eBPF_be",
    "eBPF_le",
];

pub(super) fn runtime_status_for_entry(entry_id: &str) -> Result<String> {
    Ok(if is_executable_candidate_entry(entry_id)? {
        "executable_candidate".to_string()
    } else {
        "registered_compile_only".to_string()
    })
}

pub(super) fn is_executable_candidate_entry(entry_id: &str) -> Result<bool> {
    Ok(EXECUTABLE_CANDIDATE_ENTRY_IDS
        .iter()
        .any(|&id| id == entry_id))
}

pub(super) fn language_aliases_for(processor: &str) -> Vec<String> {
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
        aliases.extend(entry.language_aliases);
    }
    aliases.into_iter().collect()
}

pub(super) fn canonical_processor_name(name: &str) -> Option<String> {
    let manifest_path = ghidra_language_manifest_path();
    let contents = fs::read_to_string(manifest_path).ok()?;
    let manifest = serde_json::from_str::<GhidraLanguageManifest>(&contents).ok()?;
    manifest.entries.into_iter().find_map(|entry| {
        if entry.processor == name || entry.language_aliases.iter().any(|alias| alias == name) {
            Some(entry.processor)
        } else {
            None
        }
    })
}
