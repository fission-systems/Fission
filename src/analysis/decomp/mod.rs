//! Decompiler Module - Ghidra Native FFI Integration
//!
//! Provides high-performance decompilation via native shared library (FFI).

pub mod native;

// Re-export native interface
pub use native::NativeDecompiler;
