//! Decompiler Module - Ghidra gRPC Integration
//!
//! Provides decompilation and advanced disassembly via gRPC communication
//! with the native Ghidra C++ server.

pub mod client;

#[cfg(test)]
mod tests;

// Re-export the gRPC client for external use
pub use client::GhidraClient;
