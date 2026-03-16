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
pub mod postprocess;
#[cfg(feature = "native_decomp")]
pub mod prepare;
#[cfg(feature = "native_decomp")]
pub mod preview_engine;

pub use postprocess::RustPostProcessOptions;
#[cfg(feature = "native_decomp")]
pub use prepare::{
    PrepareOptions, PrepareTimings, prepare_native_decompiler_for_binary,
    serialize_win_api_signatures_json,
};
#[cfg(feature = "native_decomp")]
pub use preview_engine::{
    PreviewEngineMode, PreviewSelection, PreviewSource, PreviewWorkerRequest,
    PreviewWorkerResponse, execute_preview_worker, rescue_preview_output, select_preview_output,
};

#[cfg(feature = "native_decomp")]
use self::cache::DecompilerCache;
#[cfg(feature = "native_decomp")]
use fission_loader::LoadedBinary;
#[cfg(feature = "native_decomp")]
use fission_loader::loader::types::{InferredFieldInfo, InferredTypeInfo};
#[cfg(feature = "native_decomp")]
use std::env;

#[cfg(feature = "native_decomp")]
pub type DecompilerNative = fission_ffi::DecompilerNative;

/// High-level decompiler with persistent caching
#[cfg(feature = "native_decomp")]
pub struct CachingDecompiler {
    inner: DecompilerNative,
    cache: DecompilerCache,
    inferred_types: Vec<fission_loader::loader::types::InferredTypeInfo>,
    dwarf_functions:
        std::collections::HashMap<u64, fission_loader::loader::types::DwarfFunctionInfo>,
    rust_postprocess_options: RustPostProcessOptions,
    string_map: std::collections::HashMap<u64, String>,
}

#[cfg(feature = "native_decomp")]
fn inferred_type_identity(ty: &InferredTypeInfo) -> (&str, u64, &str) {
    (&ty.name, ty.metadata_address, &ty.mangled_name)
}

#[cfg(feature = "native_decomp")]
fn merge_type_fields(existing: &mut Vec<InferredFieldInfo>, incoming: Vec<InferredFieldInfo>) {
    for field in incoming {
        if let Some(current) = existing
            .iter_mut()
            .find(|current| current.offset == field.offset)
        {
            if current.name.is_empty() && !field.name.is_empty() {
                current.name = field.name.clone();
            }
            if current.type_name.is_empty() && !field.type_name.is_empty() {
                current.type_name = field.type_name.clone();
            }
            if current.size == 0 && field.size != 0 {
                current.size = field.size;
            }
            continue;
        }
        existing.push(field);
    }
}

#[cfg(feature = "native_decomp")]
fn merge_inferred_types(
    function_types: Vec<InferredTypeInfo>,
    loader_types: &[InferredTypeInfo],
) -> Vec<InferredTypeInfo> {
    let mut merged: Vec<InferredTypeInfo> = Vec::new();

    for ty in function_types
        .into_iter()
        .chain(loader_types.iter().cloned())
    {
        if let Some(existing) = merged.iter_mut().find(|current| {
            let (name, metadata_address, mangled_name) = inferred_type_identity(current);
            let (incoming_name, incoming_metadata, incoming_mangled) = inferred_type_identity(&ty);
            metadata_address != 0 && incoming_metadata != 0 && metadata_address == incoming_metadata
                || (!mangled_name.is_empty()
                    && !incoming_mangled.is_empty()
                    && mangled_name == incoming_mangled)
                || (!name.is_empty() && !incoming_name.is_empty() && name == incoming_name)
        }) {
            if existing.kind.is_empty() && !ty.kind.is_empty() {
                existing.kind = ty.kind.clone();
            }
            if existing.mangled_name.is_empty() && !ty.mangled_name.is_empty() {
                existing.mangled_name = ty.mangled_name.clone();
            }
            if existing.metadata_address == 0 && ty.metadata_address != 0 {
                existing.metadata_address = ty.metadata_address;
            }
            if existing.size == 0 && ty.size != 0 {
                existing.size = ty.size;
            }
            merge_type_fields(&mut existing.fields, ty.fields);
            continue;
        }
        merged.push(ty);
    }

    merged
}

#[cfg(feature = "native_decomp")]
fn type_diag_enabled() -> bool {
    env::var_os("FISSION_TYPE_DIAG").is_some()
}

#[cfg(feature = "native_decomp")]
fn log_type_diag(
    address: u64,
    function_types: &[InferredTypeInfo],
    loader_types: &[InferredTypeInfo],
    merged_types: &[InferredTypeInfo],
) {
    if !type_diag_enabled() {
        return;
    }

    let count_fields =
        |items: &[InferredTypeInfo]| -> usize { items.iter().map(|ty| ty.fields.len()).sum() };
    let sample_names = |items: &[InferredTypeInfo]| -> String {
        items
            .iter()
            .take(5)
            .map(|ty| ty.name.as_str())
            .filter(|name| !name.is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    };

    eprintln!(
        "[TYPE-DIAG] addr=0x{:x} function_types={} function_fields={} loader_types={} loader_fields={} merged_types={} merged_fields={} samples=[{}]",
        address,
        function_types.len(),
        count_fields(function_types),
        loader_types.len(),
        count_fields(loader_types),
        merged_types.len(),
        count_fields(merged_types),
        sample_names(merged_types),
    );
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
        let inferred_types = binary.inferred_types.clone();
        let dwarf_functions = binary.dwarf_functions.clone();

        let string_map = binary.inner().string_map.clone();

        Ok(Self {
            inner,
            cache,
            inferred_types,
            dwarf_functions,
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
        // Decompiler types first so they take precedence for replace_field_offsets
        let function_types = result.inferred_types;
        let merged_types = merge_inferred_types(function_types.clone(), &self.inferred_types);
        log_type_diag(
            address,
            &function_types,
            &self.inferred_types,
            &merged_types,
        );

        // 4. Post-process with merged inferred types, DWARF debug info, and string map
        let dwarf_info = self.dwarf_functions.get(&address).cloned();
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
