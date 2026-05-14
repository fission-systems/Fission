use super::*;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::runtime::native::NativeBackend;
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone)]
struct RuntimeFrontendArtifacts {
    compiled: Option<CompiledFrontend>,
    native_backend: Option<Arc<NativeBackend>>,
}

static RUNTIME_FRONTEND_ARTIFACT_CACHE: OnceLock<Mutex<HashMap<String, RuntimeFrontendArtifacts>>> =
    OnceLock::new();

pub(super) fn entry_matches_language_name(entry: &EntrySpec, language_name: &str) -> bool {
    entry.entry_id == language_name
        || entry.entry_spec == format!("{language_name}.slaspec")
        || entry.entry_id.eq_ignore_ascii_case(language_name)
        || entry.arch.eq_ignore_ascii_case(language_name)
        || entry
            .language_ids
            .iter()
            .any(|id| id == language_name || id.eq_ignore_ascii_case(language_name))
        || entry
            .language_aliases
            .iter()
            .any(|alias| alias == language_name || alias.eq_ignore_ascii_case(language_name))
}

impl RuntimeSleighFrontend {
    pub(super) fn from_entry(entry: EntrySpec, language: String) -> Result<Self> {
        let status = registry::status_for_entry(&entry);
        let artifacts = Self::runtime_artifacts_for_entry(&entry, status)?;

        Ok(Self {
            language,
            entry,
            status,
            compiled: artifacts.compiled,
            native_backend: artifacts.native_backend,
        })
    }

    fn runtime_artifacts_for_entry(
        entry: &EntrySpec,
        status: RuntimeFrontendStatus,
    ) -> Result<RuntimeFrontendArtifacts> {
        let cache = RUNTIME_FRONTEND_ARTIFACT_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        if let Some(artifacts) = cache
            .lock()
            .expect("runtime artifact cache poisoned")
            .get(&entry.entry_id)
            .cloned()
        {
            return Ok(artifacts);
        }

        let artifacts = Self::load_runtime_artifacts_for_entry(entry, status)?;
        cache
            .lock()
            .expect("runtime artifact cache poisoned")
            .insert(entry.entry_id.clone(), artifacts.clone());
        Ok(artifacts)
    }

    fn load_runtime_artifacts_for_entry(
        entry: &EntrySpec,
        status: RuntimeFrontendStatus,
    ) -> Result<RuntimeFrontendArtifacts> {
        let compiled = if status == RuntimeFrontendStatus::ExecutableCandidate {
            Some(compile_frontend_for_entry_spec(&entry.path)?)
        } else {
            None
        };

        let native_backend = if compiled.is_some() {
            let spec_root = crate::compiler::generated_root_for_entry_spec(&entry.path).ok();
            let dylib_name = crate::compiler::native_backend_library_name();

            spec_root.and_then(|root| {
                let path = root.join(dylib_name);
                if path.exists() {
                    match NativeBackend::load(&path) {
                        Ok(backend) => Some(Arc::new(backend)),
                        Err(e) => {
                            tracing::error!(
                                "Failed to load native backend at {}: {}",
                                path.display(),
                                e
                            );
                            None
                        }
                    }
                } else {
                    None
                }
            })
        } else {
            None
        };

        Ok(RuntimeFrontendArtifacts {
            compiled,
            native_backend,
        })
    }

    fn exact_entry_for_id(entry_id: &str) -> Result<EntrySpec> {
        discover_all_entry_specs()?
            .into_iter()
            .find(|entry| entry.entry_id == entry_id)
            .ok_or_else(|| anyhow!("Sleigh runtime entry '{entry_id}' is not registered"))
    }

    pub fn spec_dir() -> PathBuf {
        crate::compiler::sleigh_languages_root()
    }

    pub fn find_spec_path_for(language_name: &str) -> Option<PathBuf> {
        discover_all_entry_specs()
            .ok()?
            .into_iter()
            .find(|entry| entry_matches_language_name(entry, language_name))
            .map(|entry| entry.path)
    }

    pub fn spec_path_for(language_name: &str) -> PathBuf {
        Self::find_spec_path_for(language_name)
            .unwrap_or_else(|| Self::spec_dir().join(format!("{}.slaspec", language_name)))
    }

    pub fn new_for_language(language_name: &str) -> Result<Self> {
        let entry = discover_all_entry_specs()?
            .into_iter()
            .find(|entry| entry_matches_language_name(entry, language_name))
            .ok_or_else(|| {
                anyhow!("Sleigh runtime frontend not registered for '{language_name}'")
            })?;
        Self::from_entry(entry, language_name.to_string())
    }

    pub fn new_for_load_spec(load_spec: &BinaryLoadSpec) -> Result<Self> {
        let registry = CompiledRuntimeRegistry::discover()?;
        let selection = registry.resolve_from_load_spec(load_spec)?;
        let entry = Self::exact_entry_for_id(&selection.entry_id)?;
        Self::from_entry(entry, selection.entry_id)
    }

    pub fn new_candidate_frontends_for_load_spec(load_spec: &BinaryLoadSpec) -> Result<Vec<Self>> {
        let registry = CompiledRuntimeRegistry::discover()?;
        let entry_ids = registry.executable_sibling_entry_ids_for_load_spec(load_spec)?;
        entry_ids
            .into_iter()
            .map(|entry_id| {
                let entry = Self::exact_entry_for_id(&entry_id)?;
                Self::from_entry(entry, entry_id)
            })
            .collect()
    }

    pub fn new(spec_path: &Path) -> Result<Self> {
        let language = spec_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| anyhow!("Invalid Sleigh spec path: {}", spec_path.display()))?;
        Self::new_for_language(language)
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    pub fn entry(&self) -> &EntrySpec {
        &self.entry
    }

    pub fn status(&self) -> RuntimeFrontendStatus {
        self.status
    }

    pub fn compiled_frontend(&self) -> Option<&CompiledFrontend> {
        self.compiled.as_ref()
    }

    pub fn compile_language_runtime(&self) -> Result<LanguageRuntime> {
        LanguageRuntime::compile(&self.entry)
    }

    pub fn runtime_attempt_report(&self) -> Result<RuntimeAttemptReport> {
        Ok(self.compile_language_runtime()?.attempt_report())
    }
}
