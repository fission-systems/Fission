//! FFI Type Definitions
//!
//! C-compatible types for FFI boundary with libdecomp.

use std::os::raw::{c_char, c_int};

// ============================================================================
// Core FFI Types
// ============================================================================

/// Opaque handle to decompiler context (C struct)
#[repr(C)]
pub struct DecompContext {
    _private: [u8; 0],
}

/// Symbol information passed across FFI boundary (matches C struct)
#[repr(C)]
pub(super) struct DecompSymbolInfo {
    pub address: u64,
    pub size: u32,
    pub flags: u32,
    pub name: *const c_char,
    pub name_len: u32,
}

/// Field information for struct type registration (matches C struct)
#[repr(C)]
pub struct DecompFieldInfo {
    pub name: *const c_char,
    pub type_name: *const c_char,
    pub offset: u32,
    pub size: u32,
}

// ============================================================================
// Callback Type Definitions
// ============================================================================

/// Callback for finding a symbol at or near an address
pub(super) type DecompFindSymbolFn = extern "C" fn(
    userdata: *mut std::ffi::c_void,
    address: u64,
    size: u32,
    require_start: c_int,
    out: *mut DecompSymbolInfo,
) -> c_int;

/// Callback for finding a function at an address
pub(super) type DecompFindFunctionFn = extern "C" fn(
    userdata: *mut std::ffi::c_void,
    address: u64,
    out: *mut DecompSymbolInfo,
) -> c_int;

/// Symbol provider structure passed to C++ side
#[repr(C)]
pub(super) struct DecompSymbolProvider {
    pub userdata: *mut std::ffi::c_void,
    pub find_symbol: Option<DecompFindSymbolFn>,
    pub find_function: Option<DecompFindFunctionFn>,
    pub drop: Option<extern "C" fn(*mut std::ffi::c_void)>,
}

unsafe impl Send for DecompSymbolProvider {}

// ============================================================================
// Symbol Flags
// ============================================================================

pub(super) const SYMBOL_FLAG_FUNCTION: u32 = 1 << 0;
pub(super) const SYMBOL_FLAG_DATA: u32 = 1 << 1;
pub(super) const SYMBOL_FLAG_EXTERNAL: u32 = 1 << 2;
pub(super) const SYMBOL_FLAG_READONLY: u32 = 1 << 3;
#[allow(dead_code)]
pub(super) const SYMBOL_FLAG_VOLATILE: u32 = 1 << 4;

// ============================================================================
// Error Codes
// ============================================================================

/// Error codes from libdecomp
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecompError {
    Ok = 0,
    ErrInit = -1,
    ErrLoad = -2,
    ErrDecompile = -3,
    ErrInvalidContext = -4,
    ErrOutOfMemory = -5,
    ErrFidLoad = -6,
}

impl DecompError {
    pub fn is_ok(self) -> bool {
        self == DecompError::Ok
    }
}
