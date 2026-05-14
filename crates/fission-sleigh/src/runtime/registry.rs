use std::collections::BTreeSet;
use std::fmt;
use std::sync::OnceLock;

use anyhow::{anyhow, Result};
use fission_core::architecture::BinaryLoadSpec;
use serde::{Deserialize, Serialize};

use crate::compiler::{EntrySpec, GhidraLanguageManifest, GhidraLanguageManifestEntry};
use crate::runtime::{RuntimeEndian, RuntimeFrontendStatus};

const GHIDRA_LANGUAGE_MANIFEST_JSON: &str =
    include_str!("../../../../utils/sleigh-specs/ghidra_language_manifest.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeSupportLevel {
    RegisteredCompileOnly,
    ExecutableCandidate,
}

impl RuntimeSupportLevel {
    pub const fn as_frontend_status(self) -> RuntimeFrontendStatus {
        match self {
            Self::RegisteredCompileOnly => RuntimeFrontendStatus::RegisteredCompileOnly,
            Self::ExecutableCandidate => RuntimeFrontendStatus::ExecutableCandidate,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionEngineKey {
    CompiledTable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessorDescriptor {
    pub ghidra_processor: String,
    pub module_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeVariantDescriptor {
    pub processor: String,
    pub module_name: String,
    pub entry_spec: String,
    pub entry_id: String,
    pub language_ids: Vec<String>,
    pub language_aliases: Vec<String>,
    pub processor_spec: Option<String>,
    pub generated_path: String,
    pub endian: RuntimeEndian,
    pub support_level: RuntimeSupportLevel,
    pub execution_engine_key: Option<ExecutionEngineKey>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeFrontendDescriptor {
    pub arch: String,
    pub processor: String,
    pub entry_spec: String,
    pub entry_id: String,
    pub language_ids: Vec<String>,
    pub language_aliases: Vec<String>,
    pub processor_spec: Option<String>,
    pub generated_path: String,
    pub status: RuntimeFrontendStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledRuntimeRegistry {
    processors: Vec<ProcessorDescriptor>,
    variants: Vec<RuntimeVariantDescriptor>,
    frontends: Vec<RuntimeFrontendDescriptor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEntrySelectionSource {
    LoadSpecLanguageId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEntrySelection {
    pub language_id: String,
    pub compiler_spec_id: Option<String>,
    pub entry_id: String,
    pub processor: String,
    pub runtime_status: RuntimeFrontendStatus,
    pub selection_source: RuntimeEntrySelectionSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeEntrySelectionError {
    UnknownLanguageId {
        language_id: String,
        compiler_spec_id: Option<String>,
    },
    AmbiguousRuntimeEntry {
        language_id: String,
        compiler_spec_id: Option<String>,
        candidates: Vec<String>,
    },
    CompileOnlySelection {
        language_id: String,
        compiler_spec_id: Option<String>,
        entry_id: String,
    },
    ExecutableEntryMissing {
        language_id: String,
        compiler_spec_id: Option<String>,
    },
}

impl fmt::Display for RuntimeEntrySelectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownLanguageId {
                language_id,
                compiler_spec_id,
            } => match compiler_spec_id {
                Some(compiler_spec_id) => write!(
                    f,
                    "unknown runtime language id '{language_id}' for compiler spec '{compiler_spec_id}'"
                ),
                None => write!(f, "unknown runtime language id '{language_id}'"),
            },
            Self::AmbiguousRuntimeEntry {
                language_id,
                compiler_spec_id,
                candidates,
            } => match compiler_spec_id {
                Some(compiler_spec_id) => write!(
                    f,
                    "ambiguous runtime entry for '{language_id}' / '{compiler_spec_id}': {}",
                    candidates.join(", ")
                ),
                None => write!(
                    f,
                    "ambiguous runtime entry for '{language_id}': {}",
                    candidates.join(", ")
                ),
            },
            Self::CompileOnlySelection {
                language_id,
                compiler_spec_id,
                entry_id,
            } => match compiler_spec_id {
                Some(compiler_spec_id) => write!(
                    f,
                    "runtime selection '{language_id}' / '{compiler_spec_id}' resolved to compile-only entry '{entry_id}'"
                ),
                None => write!(
                    f,
                    "runtime selection '{language_id}' resolved to compile-only entry '{entry_id}'"
                ),
            },
            Self::ExecutableEntryMissing {
                language_id,
                compiler_spec_id,
            } => match compiler_spec_id {
                Some(compiler_spec_id) => write!(
                    f,
                    "runtime selection '{language_id}' / '{compiler_spec_id}' has no executable entry"
                ),
                None => write!(
                    f,
                    "runtime selection '{language_id}' has no executable entry"
                ),
            },
        }
    }
}

impl std::error::Error for RuntimeEntrySelectionError {}

#[derive(Debug, Clone)]
struct RegistryData {
    processors: Vec<ProcessorDescriptor>,
    variants: Vec<RuntimeVariantDescriptor>,
    frontends: Vec<RuntimeFrontendDescriptor>,
}

static REGISTRY_DATA: OnceLock<RegistryData> = OnceLock::new();

impl CompiledRuntimeRegistry {
    pub fn discover() -> Result<Self> {
        let data = registry_data();
        Ok(Self {
            processors: data.processors.clone(),
            variants: data.variants.clone(),
            frontends: data.frontends.clone(),
        })
    }

    pub fn processors(&self) -> &[ProcessorDescriptor] {
        &self.processors
    }

    pub fn frontends(&self) -> &[RuntimeFrontendDescriptor] {
        &self.frontends
    }

    pub fn lookup(&self, language_name: &str) -> Option<&RuntimeFrontendDescriptor> {
        self.frontends.iter().find(|frontend| {
            frontend.entry_id == language_name
                || frontend.entry_spec == format!("{language_name}.slaspec")
                || frontend.processor == language_name
                || frontend.entry_id.eq_ignore_ascii_case(language_name)
                || frontend.processor.eq_ignore_ascii_case(language_name)
                || frontend
                    .language_ids
                    .iter()
                    .any(|id| id == language_name || id.eq_ignore_ascii_case(language_name))
                || frontend.language_aliases.iter().any(|alias| {
                    alias == language_name || alias.eq_ignore_ascii_case(language_name)
                })
        })
    }

    pub fn resolve_from_load_spec(
        &self,
        load_spec: &BinaryLoadSpec,
    ) -> std::result::Result<RuntimeEntrySelection, RuntimeEntrySelectionError> {
        self.resolve_from_language_pair(
            load_spec.pair.language_id.as_str(),
            Some(load_spec.pair.compiler_spec_id.as_str()),
        )
    }

    pub fn executable_sibling_entry_ids_for_load_spec(
        &self,
        load_spec: &BinaryLoadSpec,
    ) -> std::result::Result<Vec<String>, RuntimeEntrySelectionError> {
        let primary = self.resolve_from_load_spec(load_spec)?;
        let primary_variant = self
            .variants
            .iter()
            .find(|variant| variant.entry_id == primary.entry_id)
            .ok_or_else(|| RuntimeEntrySelectionError::ExecutableEntryMissing {
                language_id: load_spec.pair.language_id.as_str().to_string(),
                compiler_spec_id: Some(load_spec.pair.compiler_spec_id.as_str().to_string()),
            })?;
        let Some(primary_facts) = variant_language_facts(primary_variant) else {
            return Ok(vec![primary.entry_id]);
        };

        let mut entry_ids = Vec::new();
        entry_ids.push(primary.entry_id.clone());
        for variant in &self.variants {
            if variant.entry_id == primary.entry_id
                || variant.support_level != RuntimeSupportLevel::ExecutableCandidate
            {
                continue;
            }
            if variant_language_facts(variant) == Some(primary_facts.clone()) {
                entry_ids.push(variant.entry_id.clone());
            }
        }
        Ok(entry_ids)
    }

    pub fn resolve_from_language_pair(
        &self,
        language_id: &str,
        compiler_spec_id: Option<&str>,
    ) -> std::result::Result<RuntimeEntrySelection, RuntimeEntrySelectionError> {
        let matched = self
            .variants
            .iter()
            .filter(|variant| {
                variant
                    .language_ids
                    .iter()
                    .any(|id| id == language_id || id.eq_ignore_ascii_case(language_id))
            })
            .collect::<Vec<_>>();
        if matched.is_empty() {
            return Err(RuntimeEntrySelectionError::UnknownLanguageId {
                language_id: language_id.to_string(),
                compiler_spec_id: compiler_spec_id.map(str::to_string),
            });
        }

        let executable = matched
            .iter()
            .copied()
            .filter(|variant| variant.support_level == RuntimeSupportLevel::ExecutableCandidate)
            .collect::<Vec<_>>();
        match executable.as_slice() {
            [variant] => Ok(RuntimeEntrySelection {
                language_id: language_id.to_string(),
                compiler_spec_id: compiler_spec_id.map(str::to_string),
                entry_id: variant.entry_id.clone(),
                processor: variant.processor.clone(),
                runtime_status: variant.support_level.as_frontend_status(),
                selection_source: RuntimeEntrySelectionSource::LoadSpecLanguageId,
            }),
            [] if matched.len() == 1 => Err(RuntimeEntrySelectionError::CompileOnlySelection {
                language_id: language_id.to_string(),
                compiler_spec_id: compiler_spec_id.map(str::to_string),
                entry_id: matched[0].entry_id.clone(),
            }),
            [] => Err(RuntimeEntrySelectionError::ExecutableEntryMissing {
                language_id: language_id.to_string(),
                compiler_spec_id: compiler_spec_id.map(str::to_string),
            }),
            _ => Err(RuntimeEntrySelectionError::AmbiguousRuntimeEntry {
                language_id: language_id.to_string(),
                compiler_spec_id: compiler_spec_id.map(str::to_string),
                candidates: executable
                    .iter()
                    .map(|variant| variant.entry_id.clone())
                    .collect(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LanguageIdFacts {
    processor: String,
    endian: String,
    bitness: String,
}

fn language_id_facts(language_id: &str) -> Option<LanguageIdFacts> {
    let mut parts = language_id.split(':');
    let processor = parts.next()?.to_string();
    let endian = parts.next()?.to_string();
    let bitness = parts.next()?.to_string();
    if processor.is_empty() || endian.is_empty() || bitness.is_empty() {
        return None;
    }
    Some(LanguageIdFacts {
        processor,
        endian,
        bitness,
    })
}

fn variant_language_facts(variant: &RuntimeVariantDescriptor) -> Option<LanguageIdFacts> {
    variant
        .language_ids
        .iter()
        .find_map(|language_id| language_id_facts(language_id))
}

pub fn runtime_variant_for_entry(entry: &EntrySpec) -> Result<RuntimeVariantDescriptor> {
    registry_data()
        .variants
        .iter()
        .find(|variant| {
            variant.processor == entry.arch
                && variant.entry_spec == entry.entry_spec
                && variant.entry_id == entry.entry_id
        })
        .cloned()
        .ok_or_else(|| {
            anyhow!(
                "runtime variant descriptor missing for {} ({})",
                entry.entry_id,
                entry.arch
            )
        })
}

pub fn status_for_entry(entry: &EntrySpec) -> RuntimeFrontendStatus {
    runtime_variant_for_entry(entry)
        .map(|variant| variant.support_level.as_frontend_status())
        .unwrap_or(RuntimeFrontendStatus::RegisteredCompileOnly)
}

pub fn executable_engine_key_for_entry(entry: &EntrySpec) -> Option<ExecutionEngineKey> {
    runtime_variant_for_entry(entry)
        .ok()
        .and_then(|variant| variant.execution_engine_key)
}

fn registry_data() -> &'static RegistryData {
    REGISTRY_DATA.get_or_init(load_registry_data)
}

fn load_registry_data() -> RegistryData {
    let manifest: GhidraLanguageManifest = serde_json::from_str(GHIDRA_LANGUAGE_MANIFEST_JSON)
        .expect("parse checked-in language manifest");

    let mut processor_names = BTreeSet::new();
    let mut variants = Vec::with_capacity(manifest.entries.len());
    let mut frontends = Vec::with_capacity(manifest.entries.len());
    for entry in &manifest.entries {
        processor_names.insert(entry.processor.clone());
        let module_name = module_name_for_processor(&entry.processor);
        let support_level = support_level_for_manifest_entry(entry);
        let execution_engine_key = execution_engine_key_for_manifest_entry(entry);
        let variant = RuntimeVariantDescriptor {
            processor: entry.processor.clone(),
            module_name: module_name.clone(),
            entry_spec: entry.entry_spec.clone(),
            entry_id: entry.entry_id.clone(),
            language_ids: entry.language_ids.clone(),
            language_aliases: entry.language_aliases.clone(),
            processor_spec: entry.processor_spec.clone(),
            generated_path: format!("{}/{}", entry.processor, entry.entry_id),
            endian: endian_from_manifest(entry),
            support_level,
            execution_engine_key,
        };
        frontends.push(RuntimeFrontendDescriptor {
            arch: variant.processor.clone(),
            processor: variant.processor.clone(),
            entry_spec: variant.entry_spec.clone(),
            entry_id: variant.entry_id.clone(),
            language_ids: variant.language_ids.clone(),
            language_aliases: variant.language_aliases.clone(),
            processor_spec: variant.processor_spec.clone(),
            generated_path: variant.generated_path.clone(),
            status: variant.support_level.as_frontend_status(),
        });
        variants.push(variant);
    }

    let processors = processor_names
        .into_iter()
        .map(|processor| ProcessorDescriptor {
            module_name: module_name_for_processor(&processor),
            ghidra_processor: processor,
        })
        .collect::<Vec<_>>();

    assert_eq!(manifest.processor_count, processors.len());
    assert_eq!(manifest.variant_count, variants.len());

    RegistryData {
        processors,
        variants,
        frontends,
    }
}

fn support_level_for_manifest_entry(entry: &GhidraLanguageManifestEntry) -> RuntimeSupportLevel {
    match entry.runtime_status.as_str() {
        "executable_candidate" => RuntimeSupportLevel::ExecutableCandidate,
        _ => RuntimeSupportLevel::RegisteredCompileOnly,
    }
}

fn execution_engine_key_for_manifest_entry(
    entry: &GhidraLanguageManifestEntry,
) -> Option<ExecutionEngineKey> {
    match entry.runtime_status.as_str() {
        "executable_candidate" => Some(ExecutionEngineKey::CompiledTable),
        _ => None,
    }
}

fn endian_from_manifest(entry: &GhidraLanguageManifestEntry) -> RuntimeEndian {
    match entry.endian.as_deref() {
        Some("little") => RuntimeEndian::Little,
        Some("big") => RuntimeEndian::Big,
        _ => RuntimeEndian::Unknown,
    }
}

pub fn module_name_for_processor(processor: &str) -> String {
    let lowered = processor
        .chars()
        .map(|ch| match ch {
            'A'..='Z' => ch.to_ascii_lowercase(),
            'a'..='z' | '0'..='9' => ch,
            _ => '_',
        })
        .collect::<String>();
    let normalized = lowered
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if normalized
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_digit())
    {
        format!("p_{normalized}")
    } else {
        normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_arm_and_aarch64_variants_as_executable_candidates() {
        let registry = CompiledRuntimeRegistry::discover().expect("discover runtime registry");

        for (language_id, compiler_spec_id, expected_entry_id) in [
            ("AARCH64:LE:64:v8A", Some("gcc"), "AARCH64"),
            ("AARCH64:BE:64:v8A", Some("gcc"), "AARCH64BE"),
            (
                "AARCH64:LE:64:AppleSilicon",
                Some("default"),
                "AARCH64_AppleSilicon",
            ),
            ("ARM:LE:32:v7", Some("gcc"), "ARM7_le"),
            ("ARM:BE:32:v7", Some("gcc"), "ARM7_be"),
        ] {
            let selection = registry
                .resolve_from_language_pair(language_id, compiler_spec_id)
                .unwrap_or_else(|error| panic!("resolve {language_id:?} failed: {error}"));
            assert_eq!(selection.entry_id, expected_entry_id);
            assert_eq!(
                selection.runtime_status,
                RuntimeFrontendStatus::ExecutableCandidate
            );
        }
    }

    #[test]
    fn resolves_riscv_as_executable_candidate() {
        let registry = CompiledRuntimeRegistry::discover().expect("discover runtime registry");
        let selection = registry
            .resolve_from_language_pair("RISCV:LE:64:default", Some("gcc"))
            .expect("RISCV should now resolve as executable candidate");
        assert_eq!(selection.entry_id, "riscv.lp64d");
        assert_eq!(
            selection.runtime_status,
            RuntimeFrontendStatus::ExecutableCandidate
        );
    }

    #[test]
    fn powerpc_default_runtime_status_tracks_promoted_entries() {
        let registry = CompiledRuntimeRegistry::discover().expect("discover runtime registry");

        for (language_id, expected_entry_id) in [
            ("PowerPC:BE:32:default", "ppc_32_be"),
            ("PowerPC:LE:32:default", "ppc_32_le"),
            ("PowerPC:BE:64:default", "ppc_64_be"),
            ("PowerPC:LE:64:default", "ppc_64_le"),
        ] {
            let selection = registry
                .resolve_from_language_pair(language_id, Some("gcc"))
                .unwrap_or_else(|error| panic!("resolve {language_id:?} failed: {error}"));
            assert_eq!(selection.entry_id, expected_entry_id);
            assert_eq!(
                selection.runtime_status,
                RuntimeFrontendStatus::ExecutableCandidate
            );
        }

        for (language_id, expected_entry_id) in [
            ("PowerPC:BE:64:A2-32addr", "ppc_64_isa_be"),
            ("PowerPC:LE:64:A2-32addr", "ppc_64_isa_le"),
            ("PowerPC:BE:64:A2ALT", "ppc_64_isa_altivec_be"),
            ("PowerPC:LE:64:A2ALT", "ppc_64_isa_altivec_le"),
        ] {
            let selection = registry
                .resolve_from_language_pair(language_id, Some("gcc"))
                .unwrap_or_else(|error| panic!("resolve {language_id:?} failed: {error}"));
            assert_eq!(selection.entry_id, expected_entry_id);
            assert_eq!(
                selection.runtime_status,
                RuntimeFrontendStatus::ExecutableCandidate
            );
        }
    }

    #[test]
    fn powerpc64_load_spec_exposes_executable_isa_siblings() {
        let registry = CompiledRuntimeRegistry::discover().expect("discover runtime registry");
        let load_spec =
            BinaryLoadSpec::new("ELF", 0, "PowerPC:LE:64:default", "gcc", "registry-test");
        let entry_ids = registry
            .executable_sibling_entry_ids_for_load_spec(&load_spec)
            .expect("PowerPC64 LE sibling frontends");
        assert_eq!(entry_ids.first().map(String::as_str), Some("ppc_64_le"));
        assert!(
            entry_ids
                .iter()
                .any(|entry_id| entry_id == "ppc_64_isa_altivec_le"),
            "expected PowerISA Altivec LE sibling in {entry_ids:?}"
        );
    }

    #[test]
    fn runtime_registry_preserves_ldefs_processor_spec_metadata() {
        let registry = CompiledRuntimeRegistry::discover().expect("discover runtime registry");

        let arm8m = registry
            .frontends()
            .iter()
            .find(|frontend| frontend.entry_id == "ARM8m_le")
            .expect("ARM8m_le frontend");
        assert_eq!(arm8m.processor_spec.as_deref(), Some("ARMCortex.pspec"));

        let arm8 = registry
            .frontends()
            .iter()
            .find(|frontend| frontend.entry_id == "ARM8_le")
            .expect("ARM8_le frontend");
        assert_eq!(arm8.processor_spec.as_deref(), None);

        let load_spec = BinaryLoadSpec::new("ELF", 0, "ARM:LE:32:v8-m", "gcc", "registry-test");
        let selected = registry
            .resolve_from_load_spec(&load_spec)
            .expect("resolve ARM8m from ldefs language id");
        assert_eq!(selected.entry_id, "ARM8m_le");
    }
}
