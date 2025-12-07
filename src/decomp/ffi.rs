//! FFI Bindings for Ghidra Decompiler
//!
//! Unsafe C bindings to the Ghidra decompiler wrapper.
//! When native_decomp feature is not enabled, these are stub functions.

use std::ffi::{c_char, c_int};

/// Opaque pointer to the C++ FissionDecompiler instance
#[repr(C)]
pub struct FissionDecompiler {
    _private: [u8; 0],
}

// Only link native library when feature is enabled
#[cfg(feature = "native_decomp")]
#[link(name = "ghidra_decomp", kind = "static")]
extern "C" {
    pub fn fission_decompiler_init(sla_dir: *const c_char) -> *mut FissionDecompiler;
    pub fn fission_decompiler_destroy(decomp: *mut FissionDecompiler);
    pub fn fission_decompile(
        decomp: *mut FissionDecompiler,
        bytes: *const u8,
        bytes_len: usize,
        base_addr: u64,
        out_buffer: *mut c_char,
        out_len: usize,
    ) -> c_int;
    pub fn fission_disassemble(
        decomp: *mut FissionDecompiler,
        bytes: *const u8,
        bytes_len: usize,
        base_addr: u64,
        out_buffer: *mut c_char,
        out_len: usize,
    ) -> c_int;
    pub fn fission_get_error() -> *const c_char;
    pub fn fission_is_available() -> c_int;
}

// Stub implementations when native is disabled
#[cfg(not(feature = "native_decomp"))]
pub unsafe fn fission_decompiler_init(_sla_dir: *const c_char) -> *mut FissionDecompiler {
    std::ptr::null_mut()
}

#[cfg(not(feature = "native_decomp"))]
pub unsafe fn fission_decompiler_destroy(_decomp: *mut FissionDecompiler) {}

#[cfg(not(feature = "native_decomp"))]
pub unsafe fn fission_decompile(
    _decomp: *mut FissionDecompiler,
    _bytes: *const u8,
    _bytes_len: usize,
    _base_addr: u64,
    _out_buffer: *mut c_char,
    _out_len: usize,
) -> c_int {
    -1
}

#[cfg(not(feature = "native_decomp"))]
pub unsafe fn fission_disassemble(
    _decomp: *mut FissionDecompiler,
    _bytes: *const u8,
    _bytes_len: usize,
    _base_addr: u64,
    _out_buffer: *mut c_char,
    _out_len: usize,
) -> c_int {
    -1
}

#[cfg(not(feature = "native_decomp"))]
pub unsafe fn fission_get_error() -> *const c_char {
    std::ptr::null()
}

#[cfg(not(feature = "native_decomp"))]
pub unsafe fn fission_is_available() -> c_int {
    0
}
