//! Decompiler Module - Ghidra Native Subprocess Integration
//!
//! Provides high-performance decompilation via native subprocess.
//! Two modes:
//! - DecompilerServer: Persistent server (single process, multiple requests)
//! - DecompilerPool: Multi-process pool (N workers for parallel decompilation)
//! - DecompilerNative: Direct FFI to libdecomp (in-process, feature-gated)

pub mod native;

#[cfg(feature = "native_decomp")]
pub mod ffi;

// Re-export native interfaces
pub use native::{
    create_pool, create_shared_server, DecompilerPool, DecompilerServer, SharedDecompilerPool,
    SharedDecompilerServer,
};
