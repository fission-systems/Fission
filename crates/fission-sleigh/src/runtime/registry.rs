use std::collections::BTreeSet;
use std::sync::OnceLock;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use crate::compiler::{EntrySpec, GhidraLanguageManifest, GhidraLanguageManifestEntry};
use crate::runtime::{RuntimeEndian, RuntimeFrontendStatus};

const GHIDRA_LANGUAGE_MANIFEST_JSON: &str =
    include_str!("../../specs/ghidra_language_manifest.json");

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
    GeneratedX86_64,
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
    pub compatibility_aliases: Vec<String>,
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
    pub compatibility_aliases: Vec<String>,
    pub generated_path: String,
    pub status: RuntimeFrontendStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledRuntimeRegistry {
    processors: Vec<ProcessorDescriptor>,
    variants: Vec<RuntimeVariantDescriptor>,
    frontends: Vec<RuntimeFrontendDescriptor>,
}

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
                || frontend.compatibility_aliases.iter().any(|alias| {
                    alias == language_name || alias.eq_ignore_ascii_case(language_name)
                })
        })
    }
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
            compatibility_aliases: entry.compatibility_aliases.clone(),
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
            compatibility_aliases: variant.compatibility_aliases.clone(),
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
    match (entry.processor.as_str(), entry.entry_id.as_str()) {
        ("x86", "x86-64") => Some(ExecutionEngineKey::GeneratedX86_64),
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
        .map(|ch| ch.is_ascii_digit())
        .unwrap_or(false)
    {
        format!("p_{normalized}")
    } else {
        normalized
    }
}
