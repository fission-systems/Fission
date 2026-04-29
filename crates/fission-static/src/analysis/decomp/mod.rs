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
#[cfg(feature = "native_decomp")]
pub type DecompilerNative = fission_ffi::DecompilerNative;
pub mod facts;
#[cfg(feature = "native_decomp")]
pub mod prepare;

pub use facts::{FactProvenance, FactStore, FunctionFacts, NameFact, TypeFact, log_type_diag};

#[cfg(feature = "native_decomp")]
pub use prepare::{
    PrepareOptions, PrepareTimings, prepare_native_decompiler_for_binary,
    serialize_api_signatures_json,
};
