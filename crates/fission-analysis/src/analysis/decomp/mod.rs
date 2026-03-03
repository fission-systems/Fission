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

pub use postprocess::RustPostProcessOptions;

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
    inferred_types: Vec<fission_loader::loader::types::InferredTypeInfo>,
    dwarf_functions:
        std::collections::HashMap<u64, fission_loader::loader::types::DwarfFunctionInfo>,
    rust_postprocess_options: RustPostProcessOptions,
}

#[cfg(feature = "native_decomp")]
impl CachingDecompiler {
    /// Create a new caching decompiler
    pub fn new(
        binary: &LoadedBinary,
        sla_dir: &str,
        cache_size: usize,
    ) -> fission_core::Result<Self> {
        let inner = DecompilerNative::new(sla_dir)?;
        let cache = DecompilerCache::new(&binary.hash, cache_size)?;
        let inferred_types = binary.inferred_types.clone();
        let dwarf_functions = binary.dwarf_functions.clone();

        Ok(Self {
            inner,
            cache,
            inferred_types,
            dwarf_functions,
            rust_postprocess_options: RustPostProcessOptions::default(),
        })
    }

    /// Decompile a function with caching
    pub fn decompile(&mut self, address: u64) -> fission_core::Result<String> {
        // 1. Check cache
        if let Some(code) = self.cache.get(address) {
            return Ok(code);
        }

        // 2. Decompile using native engine
        let raw_code = self.inner.decompile(address)?;

        // 3. Post-process with inferred types and DWARF debug info
        let dwarf_info = self.dwarf_functions.get(&address).cloned();
        let processor = self::postprocess::PostProcessor::new()
            .with_options(self.rust_postprocess_options.clone())
            .with_inferred_types(self.inferred_types.clone())
            .with_dwarf_info(dwarf_info);
        let code = processor.process(&raw_code);

        // 4. Store in cache
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
