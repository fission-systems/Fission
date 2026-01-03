//! Decompiler Module - Ghidra Integration
//!
//! Provides high-performance decompilation via Ghidra engine.
//! Two modes:
//! - **DecompilerNative (FFI)**: Direct in-process FFI to libdecomp (recommended, feature-gated)
//! - DecompilerServer: Persistent subprocess (legacy, single process)
//! - DecompilerPool: Multi-process pool (legacy, parallel decompilation)
//!
//! The FFI mode is now the recommended approach as it provides:
//! - Better performance (no subprocess overhead)
//! - FID (Function ID) database support
//! - Lower memory footprint
//! - Simpler error handling

pub mod native;

#[cfg(feature = "native_decomp")]
pub mod ffi;

// Re-export FFI as primary interface
#[cfg(feature = "native_decomp")]
pub use ffi::DecompilerNative;

// Re-export legacy subprocess interfaces
pub use native::{
    create_pool, create_shared_server, DecompilerPool, DecompilerServer, SharedDecompilerPool,
    SharedDecompilerServer,
};

/// Recommended decompiler type (FFI when available, otherwise subprocess)
#[cfg(feature = "native_decomp")]
pub type RecommendedDecompiler = DecompilerNative;

#[cfg(not(feature = "native_decomp"))]
pub type RecommendedDecompiler = DecompilerServer;
