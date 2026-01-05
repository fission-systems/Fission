//! Decompiler Module - Ghidra Integration
//!
//! Provides high-performance decompilation via Ghidra engine.
//! 
//! Modes:
//! - **DecompilerNative (FFI)**: Direct in-process FFI to libdecomp (recommended, feature-gated)
//!
//! The FFI mode is the primary approach as it provides:
//! - Better performance (no subprocess overhead)
//! - FID (Function ID) database support
//! - Lower memory footprint
//! - Simpler error handling

#[cfg(feature = "native_decomp")]
pub mod ffi;

// Re-export FFI as primary interface
#[cfg(feature = "native_decomp")]
pub use ffi::DecompilerNative;

/// Recommended decompiler type
#[cfg(feature = "native_decomp")]
pub type RecommendedDecompiler = DecompilerNative;
