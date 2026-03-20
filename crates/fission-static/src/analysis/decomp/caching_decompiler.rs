use super::RustPostProcessOptions;
use super::cache::DecompilerCache;
use super::facts::{FactStore, log_type_diag};
use super::postprocess::PostProcessor;
use fission_loader::LoadedBinary;

pub type DecompilerNative = fission_ffi::DecompilerNative;

/// High-level decompiler with persistent caching
pub struct CachingDecompiler {
    inner: DecompilerNative,
    cache: DecompilerCache,
    fact_store: FactStore,
    rust_postprocess_options: RustPostProcessOptions,
    string_map: std::collections::HashMap<u64, String>,
}

impl CachingDecompiler {
    /// Create a new caching decompiler
    pub fn new(
        binary: &LoadedBinary,
        sla_dir: &str,
        cache_size: usize,
    ) -> fission_core::Result<Self> {
        let mut inner = DecompilerNative::new(sla_dir)?;
        let config = fission_core::config::Config::default();
        inner.set_log_verbose(config.decompiler.log_verbose);
        if !config.decompiler.log_file.is_empty() {
            inner.set_log_file(&config.decompiler.log_file);
        }
        let cache = DecompilerCache::new(&binary.hash, cache_size)?;
        let fact_store = FactStore::from_binary(binary);
        let string_map = binary.inner().string_map.clone();

        Ok(Self {
            inner,
            cache,
            fact_store,
            rust_postprocess_options: RustPostProcessOptions::default(),
            string_map,
        })
    }

    /// Decompile a function with caching
    pub fn decompile(&mut self, address: u64) -> fission_core::Result<String> {
        if let Some(code) = self.cache.get(address) {
            return Ok(code);
        }

        let result = self.inner.decompile_with_metadata(address)?;

        let function_types = result.inferred_types;
        self.fact_store
            .ingest_native_function_types(address, function_types.clone());
        let merged_types = self.fact_store.merged_inferred_types(address);
        log_type_diag(
            address,
            &function_types,
            self.fact_store.loader_type_facts(),
            &merged_types,
        );

        let dwarf_info = self.fact_store.preferred_debug_function(address).cloned();
        let processor = PostProcessor::new()
            .with_options(self.rust_postprocess_options.clone())
            .with_inferred_types(merged_types)
            .with_dwarf_info(dwarf_info)
            .with_string_map(Some(self.string_map.clone()));
        let code = processor.process(&result.code);

        self.cache.put(address, code.clone());

        Ok(code)
    }

    /// Access the underlying native decompiler
    pub fn inner_mut(&mut self) -> &mut DecompilerNative {
        &mut self.inner
    }

    /// Clear the decompiler cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    pub fn fact_store(&self) -> &FactStore {
        &self.fact_store
    }

    pub fn fact_store_mut(&mut self) -> &mut FactStore {
        &mut self.fact_store
    }

    /// Set Rust post-processing options
    pub fn set_rust_postprocess_options(&mut self, options: RustPostProcessOptions) {
        self.rust_postprocess_options = options;
    }

    /// Get current Rust post-processing options
    pub fn rust_postprocess_options(&self) -> &RustPostProcessOptions {
        &self.rust_postprocess_options
    }
}

/// Recommended decompiler type (CachingDecompiler when native is available)
pub type RecommendedDecompiler = CachingDecompiler;
