//! Decompiler Module - Ghidra Sleigh Integration
//!
//! Provides decompilation and advanced disassembly via the Ghidra C++ engine.
//! Falls back to stub output when native library unavailable.

pub mod engine;
pub mod ffi;

#[cfg(test)]
mod tests;

pub use engine::{Decompiler, StubDecompiler, DecompiledFunction, DisassemblyResult};

