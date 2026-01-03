//! Native FFI bindings for libdecomp shared library
//!
//! This module provides Rust bindings to the Ghidra decompiler library,
//! enabling in-process decompilation without subprocess overhead.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::ptr;

use crate::core::prelude::*;

// ============================================================================
// FFI Type Definitions
// ============================================================================

/// Opaque handle to decompiler context (C struct)
#[repr(C)]
pub struct DecompContext {
    _private: [u8; 0],
}

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

// ============================================================================
// External FFI Function Declarations
// ============================================================================

#[cfg(feature = "native_decomp")]
#[link(name = "decomp")]
extern "C" {
    fn decomp_create(sla_dir: *const c_char) -> *mut DecompContext;
    fn decomp_destroy(ctx: *mut DecompContext);
    fn decomp_load_binary(
        ctx: *mut DecompContext,
        data: *const u8,
        len: usize,
        base_addr: u64,
        is_64bit: c_int,
    ) -> DecompError;
    fn decomp_add_symbol(ctx: *mut DecompContext, addr: u64, name: *const c_char);
    fn decomp_clear_symbols(ctx: *mut DecompContext);
    fn decomp_add_function(ctx: *mut DecompContext, addr: u64, name: *const c_char) -> DecompError;
    fn decomp_add_memory_block(
        ctx: *mut DecompContext,
        name: *const c_char,
        va_addr: u64,
        va_size: u64,
        file_offset: u64,
        file_size: u64,
        is_executable: c_int,
        is_writable: c_int,
    ) -> DecompError;
    fn decomp_function(ctx: *mut DecompContext, addr: u64) -> *mut c_char;
    fn decomp_free_string(s: *mut c_char);
    fn decomp_get_last_error(ctx: *mut DecompContext) -> *const c_char;
    fn decomp_set_gdt(ctx: *mut DecompContext, gdt_path: *const c_char) -> DecompError;
    fn decomp_set_feature(ctx: *mut DecompContext, feature: *const c_char, enabled: c_int);
    fn decomp_load_fid_db(ctx: *mut DecompContext, db_path: *const c_char) -> DecompError;
    fn decomp_get_fid_match(ctx: *mut DecompContext, addr: u64, len: usize) -> *mut c_char;
}

// ============================================================================
// Safe Rust Wrapper
// ============================================================================

/// Native decompiler interface using FFI to libdecomp
///
/// This provides direct in-process access to the Ghidra decompiler,
/// avoiding subprocess spawn overhead.
///
/// # Safety
///
/// This struct wraps a raw C++ pointer and is marked as Send to allow
/// use across threads. However, the underlying C++ object is NOT thread-safe.
/// Users must ensure:
/// - Only one thread accesses this instance at a time (use Mutex if sharing)
/// - The instance is properly dropped before the thread terminates
/// - No use-after-free by keeping references after drop
#[cfg(feature = "native_decomp")]
pub struct DecompilerNative {
    ctx: *mut DecompContext,
    sla_dir: String,
    // Track if context is valid to prevent use-after-free
    is_valid: bool,
}

#[cfg(feature = "native_decomp")]
unsafe impl Send for DecompilerNative {}

#[cfg(feature = "native_decomp")]
impl DecompilerNative {
    /// Create a new native decompiler instance
    pub fn new(sla_dir: &str) -> Result<Self> {
        if sla_dir.is_empty() {
            return Err(FissionError::decompiler("SLA directory cannot be empty"));
        }
        
        let sla_cstr = CString::new(sla_dir)
            .map_err(|_| FissionError::decompiler("Invalid SLA directory path (contains null byte)"))?;

        let ctx = unsafe { decomp_create(sla_cstr.as_ptr()) };
        if ctx.is_null() {
            return Err(FissionError::decompiler(
                "Failed to create decompiler context",
            ));
        }

        Ok(Self {
            ctx,
            sla_dir: sla_dir.to_string(),
            is_valid: true,
        })
    }
    
    /// Check if the decompiler context is still valid
    fn check_valid(&self) -> Result<()> {
        if !self.is_valid {
            return Err(FissionError::decompiler("Decompiler context has been invalidated"));
        }
        if self.ctx.is_null() {
            return Err(FissionError::decompiler("Decompiler context pointer is null"));
        }
        Ok(())
    }

    /// Load a binary into the decompiler context
    pub fn load_binary(&mut self, data: &[u8], base_addr: u64, is_64bit: bool) -> Result<()> {
        self.check_valid()?;
        
        if data.is_empty() {
            return Err(FissionError::decompiler("Cannot load empty binary"));
        }
        let result = unsafe {
            decomp_load_binary(
                self.ctx,
                data.as_ptr(),
                data.len(),
                base_addr,
                if is_64bit { 1 } else { 0 },
            )
        };

        if result.is_ok() {
            Ok(())
        } else {
            Err(FissionError::decompiler(self.get_last_error()))
        }
    }

    /// Add a symbol (function name) at the given address
    pub fn add_symbol(&mut self, addr: u64, name: &str) {
        if self.check_valid().is_err() || name.is_empty() {
            return;
        }
        if let Ok(name_cstr) = CString::new(name) {
            unsafe { decomp_add_symbol(self.ctx, addr, name_cstr.as_ptr()) };
        }
    }

    /// Add multiple symbols from IAT or symbol table
    pub fn add_symbols(&mut self, symbols: &HashMap<u64, String>) {
        if self.check_valid().is_err() {
            return;
        }
        for (addr, name) in symbols {
            self.add_symbol(*addr, name);
        }
    }

    /// Clear all symbols
    pub fn clear_symbols(&mut self) {
        unsafe { decomp_clear_symbols(self.ctx) };
    }

    /// Declare a function at the given address
    /// 
    /// This helps Ghidra recognize function boundaries and improves
    /// decompilation quality. Should be called after load_binary()
    /// with all known function addresses.
    pub fn add_function(&mut self, addr: u64, name: Option<&str>) -> Result<()> {
        self.check_valid()?;
        
        let name_cstr = if let Some(n) = name {
            Some(CString::new(n)
                .map_err(|_| FissionError::decompiler("Invalid function name (contains null byte)"))?)
        } else {
            None
        };

        let name_ptr = name_cstr.as_ref().map(|c| c.as_ptr()).unwrap_or(ptr::null());

        let result = unsafe { decomp_add_function(self.ctx, addr, name_ptr) };

        if result.is_ok() {
            Ok(())
        } else {
            Err(FissionError::decompiler(self.get_last_error()))
        }
    }

    /// Add a memory block (section) to help Ghidra understand memory layout
    /// 
    /// This distinguishes between code and data sections, improving
    /// analysis accuracy. Should be called after load_binary().
    pub fn add_memory_block(
        &mut self,
        name: &str,
        va_addr: u64,
        va_size: u64,
        file_offset: u64,
        file_size: u64,
        is_executable: bool,
        is_writable: bool,
    ) -> Result<()> {
        self.check_valid()?;
        
        if name.is_empty() {
            return Err(FissionError::decompiler("Section name cannot be empty"));
        }
        
        let name_cstr = CString::new(name)
            .map_err(|_| FissionError::decompiler("Invalid section name (contains null byte)"))?;

        let result = unsafe {
            decomp_add_memory_block(
                self.ctx,
                name_cstr.as_ptr(),
                va_addr,
                va_size,
                file_offset,
                file_size,
                is_executable as c_int,
                is_writable as c_int,
            )
        };

        if result.is_ok() {
            Ok(())
        } else {
            Err(FissionError::decompiler(self.get_last_error()))
        }
    }

    /// Decompile a function at the given address
    pub fn decompile(&self, addr: u64) -> Result<String> {
        self.check_valid()?;
        
        let result_ptr = unsafe { decomp_function(self.ctx, addr) };

        if result_ptr.is_null() {
            return Err(FissionError::decompiler(self.get_last_error()));
        }

        let result = unsafe {
            let cstr = CStr::from_ptr(result_ptr);
            let string = cstr.to_string_lossy().into_owned();
            decomp_free_string(result_ptr);
            string
        };

        Ok(result)
    }

    /// Set GDT (Ghidra Data Type) file for type information
    pub fn set_gdt(&mut self, gdt_path: &str) -> Result<()> {
        self.check_valid()?;
        
        if gdt_path.is_empty() {
            return Err(FissionError::decompiler("GDT path cannot be empty"));
        }
        
        let path_cstr =
            CString::new(gdt_path).map_err(|_| FissionError::decompiler("Invalid GDT path (contains null byte)"))?;

        let result = unsafe { decomp_set_gdt(self.ctx, path_cstr.as_ptr()) };

        if result.is_ok() {
            Ok(())
        } else {
            Err(FissionError::decompiler("Failed to set GDT"))
        }
    }

    /// Enable or disable a decompiler feature
    pub fn set_feature(&mut self, feature: &str, enabled: bool) {
        if self.check_valid().is_err() || feature.is_empty() {
            return;
        }
        if let Ok(feat_cstr) = CString::new(feature) {
            unsafe {
                decomp_set_feature(self.ctx, feat_cstr.as_ptr(), if enabled { 1 } else { 0 });
            }
        }
    }

    /// Load FID (Function ID) database for library function recognition
    pub fn load_fid_database(&mut self, db_path: &str) -> Result<()> {
        self.check_valid()?;
        
        if db_path.is_empty() {
            return Err(FissionError::decompiler("FID database path cannot be empty"));
        }
        
        let path_cstr = CString::new(db_path)
            .map_err(|_| FissionError::decompiler("Invalid FID database path (contains null byte)"))?;

        let result = unsafe { decomp_load_fid_db(self.ctx, path_cstr.as_ptr()) };

        if result.is_ok() {
            Ok(())
        } else {
            Err(FissionError::decompiler(format!(
                "Failed to load FID database: {}",
                db_path
            )))
        }
    }

    /// Try to match function at address using FID database
    pub fn match_function_by_fid(&self, addr: u64, len: usize) -> Option<String> {
        let result_ptr = unsafe { decomp_get_fid_match(self.ctx, addr, len) };

        if result_ptr.is_null() {
            return None;
        }

        let result = unsafe {
            let cstr = CStr::from_ptr(result_ptr);
            let string = cstr.to_string_lossy().into_owned();
            decomp_free_string(result_ptr);
            string
        };

        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Get the last error message
    pub fn get_last_error(&self) -> String {
        let err_ptr = unsafe { decomp_get_last_error(self.ctx) };
        if err_ptr.is_null() {
            return "Unknown error".to_string();
        }

        unsafe { CStr::from_ptr(err_ptr).to_string_lossy().into_owned() }
    }
}

#[cfg(feature = "native_decomp")]
impl Drop for DecompilerNative {
    fn drop(&mut self) {
        // Invalidate context first to prevent use-after-free
        self.is_valid = false;
        
        if !self.ctx.is_null() {
            unsafe { decomp_destroy(self.ctx) };
            self.ctx = ptr::null_mut();
        }
    }
}

// ============================================================================
// Feature-gated re-export
// ============================================================================

/// Check if native decompiler is available
pub fn is_native_available() -> bool {
    #[cfg(feature = "native_decomp")]
    {
        true
    }
    #[cfg(not(feature = "native_decomp"))]
    {
        false
    }
}
