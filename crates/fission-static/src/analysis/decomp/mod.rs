//! Decompiler Module - Ghidra Integration
//!
//! Provides high-performance decompilation via Ghidra engine.
//!
//! ## Architecture
//!
//! The decompiler uses FFI bindings provided by `fission-ffi` crate
//! to communicate with the native Ghidra decompiler library.
//!
//! ```text
//! fission-analysis/decomp (safe wrapper)
//!         ↓
//! fission-ffi (unsafe FFI boundary)
//!         ↓
//! libdecomp.so (native Ghidra)
//! ```
//!
//! ## Usage
//!
//! Use the `fission-ffi` crate directly for decompilation:
//!
//! ```rust,ignore
//! use fission_ffi::DecompilerNative;
//!
//! let decomp = DecompilerNative::new(binary)?;
//! let result = decomp.decompile_function(addr)?;
//! ```

// NOTE: FFI bindings have been moved to fission-ffi crate
// This module now provides high-level safe wrappers only

pub mod cache;
pub mod facts;
pub mod postprocess;
pub(crate) mod preview_context;
#[cfg(feature = "native_decomp")]
pub mod prepare;
#[cfg(feature = "native_decomp")]
pub mod preview_engine;

pub use facts::{FactProvenance, FactStore, FunctionFacts, NameFact, TypeFact, log_type_diag};
pub use postprocess::RustPostProcessOptions;
#[cfg(feature = "native_decomp")]
pub use prepare::{
    PrepareOptions, PrepareTimings, prepare_native_decompiler_for_binary,
    serialize_win_api_signatures_json,
};
#[cfg(feature = "native_decomp")]
pub use preview_engine::{
    PreviewEngineMode, PreviewRoutingDecision, PreviewRoutingResolver, PreviewSelection,
    PreviewSource, PreviewSurfaceKind, PreviewWorkerRequest, PreviewWorkerResponse,
    classify_native_failure_kind, execute_preview_worker, fallback_reason_with_kind,
    native_failure_routing_decision, rescue_preview_output, rescue_preview_output_with_facts,
    select_preview_output, select_preview_output_with_facts,
};

#[cfg(feature = "native_decomp")]
use self::cache::DecompilerCache;
#[cfg(feature = "native_decomp")]
use fission_loader::LoadedBinary;

#[cfg(feature = "native_decomp")]
pub type DecompilerNative = fission_ffi::DecompilerNative;

/// High-level decompiler with persistent caching
#[cfg(feature = "native_decomp")]
pub struct CachingDecompiler {
    inner: DecompilerNative,
    cache: DecompilerCache,
    fact_store: FactStore,
    rust_postprocess_options: RustPostProcessOptions,
    string_map: std::collections::HashMap<u64, String>,
}

#[cfg(feature = "native_decomp")]
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
        // 1. Check cache
        if let Some(code) = self.cache.get(address) {
            return Ok(code);
        }

        // 2. Decompile using native engine (with metadata for StructureAnalyzer types)
        let result = self.inner.decompile_with_metadata(address)?;

        // 3. Merge inferred types: loader (DWARF/RTTI) + per-function (StructureAnalyzer)
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

        // 4. Post-process with merged inferred types, DWARF debug info, and string map
        let dwarf_info = self.fact_store.dwarf_function(address).cloned();
        let processor = self::postprocess::PostProcessor::new()
            .with_options(self.rust_postprocess_options.clone())
            .with_inferred_types(merged_types)
            .with_dwarf_info(dwarf_info)
            .with_string_map(Some(self.string_map.clone()));
        let code = processor.process(&result.code);

        // 5. Store in cache
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
#[cfg(feature = "native_decomp")]
pub type RecommendedDecompiler = CachingDecompiler;
