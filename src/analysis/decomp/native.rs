//! Native FFI bindings for Ghidra Decompiler.
//!
//! Loads the `fission_decompiler` shared library at runtime and provides
//! a safe Rust interface for high-performance decompilation.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::ffi::{CString, CStr};
use std::os::raw::{c_char, c_int};
use libloading::{Library, Symbol};
use anyhow::{anyhow, Result};

/// Opaque pointer to FissionDecompiler in C++
type DecompilerHandle = *mut std::ffi::c_void;

/// Function signatures matching wrapper.h
type FissionInitFn = unsafe extern "C" fn(sla_dir: *const c_char) -> DecompilerHandle;
type FissionDestroyFn = unsafe extern "C" fn(handle: DecompilerHandle);
type FissionDecompileFn = unsafe extern "C" fn(
    handle: DecompilerHandle,
    bytes: *const u8,
    bytes_len: usize,
    base_addr: u64,
    out_buffer: *mut c_char,
    out_len: usize
) -> c_int;
type FissionGetErrorFn = unsafe extern "C" fn() -> *const c_char;

/// Shared library interface
pub struct NativeDecompiler {
    lib: Library,
    handle: DecompilerHandle,
    // Store symbols to avoid lookup overhead
    f_decompile: Symbol<'static, FissionDecompileFn>,
    f_get_error: Symbol<'static, FissionGetErrorFn>,
    f_destroy: Symbol<'static, FissionDestroyFn>,
}

// Safety: FissionDecompiler has a mutex in C++ for thread safety
unsafe impl Send for NativeDecompiler {}
unsafe impl Sync for NativeDecompiler {}

impl NativeDecompiler {
    /// Load library and initialize decompiler
    pub fn new<P: AsRef<std::ffi::OsStr>>(lib_path: P, sla_dir: &str) -> Result<Self> {
        let lib = unsafe { Library::new(lib_path)? };
        
        let handle = unsafe {
            let f_init: Symbol<FissionInitFn> = lib.get(b"fission_decompiler_init")?;
            let c_sla_dir = CString::new(sla_dir)?;
            let h = f_init(c_sla_dir.as_ptr());
            if h.is_null() {
                let f_err: Symbol<FissionGetErrorFn> = lib.get(b"fission_get_error")?;
                let err_ptr = f_err();
                let msg = if err_ptr.is_null() {
                    "Unknown error during init".to_string()
                } else {
                    CStr::from_ptr(err_ptr).to_string_lossy().into_owned()
                };
                return Err(anyhow!("FFI Init Failed: {}", msg));
            }
            h
        };

        // Leak symbols to 'static lifetime as they are bound to the library's lifetime
        // and we own the library in this struct.
        let f_decompile = unsafe { std::mem::transmute(lib.get::<FissionDecompileFn>(b"fission_decompile")?) };
        let f_get_error = unsafe { std::mem::transmute(lib.get::<FissionGetErrorFn>(b"fission_get_error")?) };
        let f_destroy = unsafe { std::mem::transmute(lib.get::<FissionDestroyFn>(b"fission_decompiler_destroy")?) };

        Ok(Self {
            lib,
            handle,
            f_decompile,
            f_get_error,
            f_destroy,
        })
    }

    /// Decompile bytes
    pub fn decompile(&self, bytes: &[u8], base_addr: u64) -> Result<String> {
        let mut buffer = vec![0u8; 1024 * 512]; // 1MB buffer for C code
        
        let res = unsafe {
            (self.f_decompile)(
                self.handle,
                bytes.as_ptr(),
                bytes.len(),
                base_addr,
                buffer.as_mut_ptr() as *mut c_char,
                buffer.len()
            )
        };

        if res < 0 {
            let err_ptr = unsafe { (self.f_get_error)() };
            let msg = if err_ptr.is_null() {
                "Unknown error during decompilation".to_string()
            } else {
                unsafe { CStr::from_ptr(err_ptr).to_string_lossy().into_owned() }
            };
            return Err(anyhow!("Decompile error: {}", msg));
        }

        let code = unsafe { CStr::from_ptr(buffer.as_ptr() as *const c_char) }
            .to_string_lossy()
            .into_owned();
            
        Ok(code)
    }
}

impl Drop for NativeDecompiler {
    fn drop(&mut self) {
        unsafe {
            (self.f_destroy)(self.handle);
        }
    }
}

/// Helper to find the decompiler library in the project structure
pub fn find_library() -> Option<std::path::PathBuf> {
    let base = std::env::current_dir().ok()?;
    let lib_name = if cfg!(target_os = "windows") {
        "fission_decompiler.dll"
    } else {
        "libfission_decompiler.so"
    };

    let paths = [
        base.join("build/Release").join(lib_name),
        base.join("build/Debug").join(lib_name),
        base.join("build").join(lib_name),
        base.join(lib_name),
    ];

    for p in &paths {
        if p.exists() {
            return Some(p.clone());
        }
    }
    None
}

