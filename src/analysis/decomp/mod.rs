//! Decompiler Module - Ghidra Native FFI Integration
//!
//! Provides high-performance decompilation via native subprocess.
//! Three modes:
//! - NativeDecompiler: Single-shot (spawns new process each time)
//! - DecompilerServer: Persistent server (single process, multiple requests)
//! - DecompilerPool: Multi-process pool (N workers for parallel decompilation)

pub mod native;

// Re-export native interfaces
pub use native::{
    NativeDecompiler, 
    DecompilerServer, 
    DecompilerPool,
    SharedDecompilerServer, 
    SharedDecompilerPool,
    create_shared_server,
    create_pool,
};
