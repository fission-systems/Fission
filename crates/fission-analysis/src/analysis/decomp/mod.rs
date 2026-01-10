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

#[cfg(feature = "native_decomp")]
pub type DecompilerNative = fission_ffi::DecompilerNative;

/// Recommended decompiler type (re-exported from fission-ffi)
#[cfg(feature = "native_decomp")]
pub type RecommendedDecompiler = fission_ffi::DecompilerNative;
