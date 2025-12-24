//! Decompiler Module - Ghidra Native FFI Integration
//!
//! Provides high-performance decompilation via native subprocess.
//! Two modes: NativeDecompiler (single-shot) and DecompilerServer (persistent).

pub mod native;

// Re-export native interfaces
pub use native::{NativeDecompiler, DecompilerServer, SharedDecompilerServer, create_shared_server};
