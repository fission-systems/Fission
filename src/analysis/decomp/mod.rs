//! Decompiler Module - Ghidra Native Subprocess Integration
//!
//! Provides high-performance decompilation via native subprocess.
//! Two modes:
//! - DecompilerServer: Persistent server (single process, multiple requests)
//! - DecompilerPool: Multi-process pool (N workers for parallel decompilation)

pub mod native;

// Re-export native interfaces
pub use native::{
    DecompilerServer, 
    DecompilerPool,
    SharedDecompilerServer, 
    SharedDecompilerPool,
    create_shared_server,
    create_pool,
};
