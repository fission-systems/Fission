//! Fission FFI - Foreign Function Interface Layer
//!
//! This crate provides the FFI boundary between Rust and C/C++.
//! All unsafe FFI interactions are isolated here.
//!
//! ## Architecture
//!
//! ```text
//! C/C++ Decompiler
//!        ↕ (unsafe FFI)
//!    fission-ffi (this crate)
//!        ↓ (safe Rust)
//!  fission-pcode, fission-analysis
//! ```
//!
//! ## Modules
//!
//! - **pcode**: Raw C FFI for pcode optimization (unsafe)
//! - **pcode_safe**: Safe Rust wrapper for pcode operations
//! - **decomp**: Raw C FFI for decompiler and its Safe Wrapper (requires native_decomp feature)
//!
//! ## Exported Functions
//!
//! - **Pcode Optimization**: `fission_optimize_pcode_json`, `fission_free_string`
//! - **Decompiler Bridge**: Native decompiler context management (requires native_decomp feature)
//!
//! ## Safety
//!
//! All FFI functions follow these safety rules:
//! - Null pointer checks
//! - UTF-8 validation
//! - Memory ownership tracking
//! - Error reporting via return codes or null pointers

// Unsafe FFI modules (C ABI)
pub mod pcode;

#[cfg(feature = "native_decomp")]
pub mod decomp;

// Safe Rust wrappers
pub mod pcode_safe;

// Re-export main FFI functions for C/C++ consumers
pub use pcode::*;

#[cfg(feature = "native_decomp")]
pub use decomp::*;

// Re-export safe interfaces for Rust consumers
pub use pcode_safe::*;
