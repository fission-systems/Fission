//! Decompiler FFI Bridge - Native Ghidra Decompiler Bindings
//!
//! This module provides Rust bindings to the Ghidra decompiler library,
//! enabling in-process decompilation without subprocess overhead.
//!
//! All unsafe FFI operations for the decompiler are isolated here.
//!
//! # Architecture
//!
//! - [`types`] - C-compatible type definitions
//! - [`ffi`] - External FFI function declarations
//! - [`symbols`] - Symbol provider implementation
//! - [`wrapper`] - Safe Rust wrapper (DecompilerNative)
//!
//! # Public API
//!
//! - [`DecompilerNative`](wrapper::DecompilerNative) - Main decompiler interface
//! - [`is_native_available`](ffi::is_native_available) - Feature detection
//! - [`DecompContext`](types::DecompContext) - Opaque context handle
//! - [`DecompFieldInfo`](types::DecompFieldInfo) - Field information for type registration

mod ffi;
mod symbols;
mod types;
pub mod wrapper;

// Re-export public API
pub use ffi::is_native_available;
pub use types::{DecompContext, DecompError, DecompFieldInfo};
#[cfg(feature = "native_decomp")]
pub use wrapper::DecompilerNative;
