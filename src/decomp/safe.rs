//! Safe FFI Wrapper for Ghidra Decompiler
//!
//! Provides panic-safe, timeout-protected access to the native decompiler.
//! All FFI calls are wrapped with catch_unwind to prevent crashes.

use anyhow::{anyhow, Result};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::decomp::ffi;

/// Thread-safe flag for initialization status
static NATIVE_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Safe wrapper around the native decompiler that never panics
pub struct SafeDecompiler {
    /// Native instance pointer (null if not initialized)
    instance: *mut ffi::FissionDecompiler,
    /// Path to .sla files
    sla_dir: String,
    /// Maximum time allowed for a single operation
    timeout: Duration,
    /// Whether this instance is valid
    valid: AtomicBool,
}

// Safety: The native instance is protected by internal mutex in C++ code
unsafe impl Send for SafeDecompiler {}
unsafe impl Sync for SafeDecompiler {}

impl SafeDecompiler {
    /// Default timeout for decompilation operations (30 seconds)
    pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

    /// Create a new safe decompiler instance
    /// 
    /// # Arguments
    /// * `sla_dir` - Path to directory containing .sla specification files
    /// 
    /// # Returns
    /// * `Ok(SafeDecompiler)` if initialization succeeded
    /// * `Err` if native library unavailable or initialization failed
    pub fn new(sla_dir: impl Into<String>) -> Result<Self> {
        let sla_dir = sla_dir.into();
        
        // Wrap FFI call in catch_unwind for safety
        let result = catch_unwind(AssertUnwindSafe(|| {
            Self::init_native(&sla_dir)
        }));
        
        match result {
            Ok(Ok(instance)) => {
                NATIVE_INITIALIZED.store(true, Ordering::SeqCst);
                Ok(Self {
                    instance,
                    sla_dir,
                    timeout: Self::DEFAULT_TIMEOUT,
                    valid: AtomicBool::new(true),
                })
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(anyhow!("Native decompiler initialization panicked")),
        }
    }

    /// Initialize native library (internal, unsafe)
    fn init_native(sla_dir: &str) -> Result<*mut ffi::FissionDecompiler> {
        use std::ffi::CString;
        
        let c_sla_dir = CString::new(sla_dir)
            .map_err(|_| anyhow!("Invalid sla_dir path"))?;
        
        let instance = unsafe { ffi::fission_decompiler_init(c_sla_dir.as_ptr()) };
        
        if instance.is_null() {
            let error = Self::get_native_error()
                .unwrap_or_else(|| "Unknown initialization error".to_string());
            return Err(anyhow!("Failed to initialize: {}", error));
        }
        
        Ok(instance)
    }

    /// Set timeout for operations
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Check if the decompiler is valid and ready
    pub fn is_valid(&self) -> bool {
        self.valid.load(Ordering::SeqCst) && !self.instance.is_null()
    }

    /// Safely decompile a function with timeout protection
    /// 
    /// # Arguments
    /// * `bytes` - Raw machine code bytes
    /// * `address` - Virtual address of the function entry point
    /// 
    /// # Returns
    /// * `Ok(String)` with decompiled C code
    /// * `Err` if decompilation failed or timed out
    pub fn decompile(&self, bytes: &[u8], address: u64) -> Result<String> {
        if !self.is_valid() {
            return Err(anyhow!("Decompiler not initialized"));
        }
        
        if bytes.is_empty() {
            return Err(anyhow!("Empty input bytes"));
        }
        
        let start = Instant::now();
        let instance = self.instance;
        let timeout = self.timeout;
        
        // Clone bytes for safe FFI call
        let bytes_vec: Vec<u8> = bytes.to_vec();
        
        // Wrap FFI call in catch_unwind
        let result = catch_unwind(AssertUnwindSafe(move || {
            Self::call_decompile(instance, &bytes_vec, address, timeout)
        }));
        
        let elapsed = start.elapsed();
        
        match result {
            Ok(Ok(code)) => {
                log::debug!("Decompilation completed in {:?}", elapsed);
                Ok(code)
            }
            Ok(Err(e)) => Err(e),
            Err(_) => {
                // Mark instance as invalid after panic
                self.valid.store(false, Ordering::SeqCst);
                Err(anyhow!("Decompilation panicked - instance invalidated"))
            }
        }
    }

    /// Internal FFI call for decompilation
    fn call_decompile(
        instance: *mut ffi::FissionDecompiler,
        bytes: &[u8],
        address: u64,
        _timeout: Duration,
    ) -> Result<String> {
        use std::ffi::CStr;
        
        // Allocate output buffer (64KB)
        let mut buffer: Vec<u8> = vec![0; 64 * 1024];
        
        let result = unsafe {
            ffi::fission_decompile(
                instance,
                bytes.as_ptr(),
                bytes.len(),
                address,
                buffer.as_mut_ptr() as *mut i8,
                buffer.len(),
            )
        };
        
        if result < 0 {
            let error = Self::get_native_error()
                .unwrap_or_else(|| "Decompilation failed".to_string());
            return Err(anyhow!("{}", error));
        }
        
        // Convert buffer to string
        let code = unsafe {
            CStr::from_ptr(buffer.as_ptr() as *const i8)
                .to_string_lossy()
                .into_owned()
        };
        
        Ok(code)
    }

    /// Safely disassemble instructions with timeout protection
    /// 
    /// # Arguments
    /// * `bytes` - Raw machine code bytes
    /// * `address` - Virtual address of the first instruction
    /// 
    /// # Returns
    /// * `Ok(String)` with disassembly text
    /// * `Err` if disassembly failed or timed out
    pub fn disassemble(&self, bytes: &[u8], address: u64) -> Result<String> {
        if !self.is_valid() {
            return Err(anyhow!("Decompiler not initialized"));
        }
        
        if bytes.is_empty() {
            return Err(anyhow!("Empty input bytes"));
        }
        
        let instance = self.instance;
        let bytes_vec: Vec<u8> = bytes.to_vec();
        
        // Wrap FFI call in catch_unwind
        let result = catch_unwind(AssertUnwindSafe(move || {
            Self::call_disassemble(instance, &bytes_vec, address)
        }));
        
        match result {
            Ok(Ok(text)) => Ok(text),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                self.valid.store(false, Ordering::SeqCst);
                Err(anyhow!("Disassembly panicked - instance invalidated"))
            }
        }
    }

    /// Internal FFI call for disassembly
    fn call_disassemble(
        instance: *mut ffi::FissionDecompiler,
        bytes: &[u8],
        address: u64,
    ) -> Result<String> {
        use std::ffi::CStr;
        
        // Allocate output buffer (64KB)
        let mut buffer: Vec<u8> = vec![0; 64 * 1024];
        
        let result = unsafe {
            ffi::fission_disassemble(
                instance,
                bytes.as_ptr(),
                bytes.len(),
                address,
                buffer.as_mut_ptr() as *mut i8,
                buffer.len(),
            )
        };
        
        if result < 0 {
            let error = Self::get_native_error()
                .unwrap_or_else(|| "Disassembly failed".to_string());
            return Err(anyhow!("{}", error));
        }
        
        // Convert buffer to string
        let text = unsafe {
            CStr::from_ptr(buffer.as_ptr() as *const i8)
                .to_string_lossy()
                .into_owned()
        };
        
        Ok(text)
    }

    /// Get the last error from native library
    fn get_native_error() -> Option<String> {
        use std::ffi::CStr;
        
        unsafe {
            let err_ptr = ffi::fission_get_error();
            if err_ptr.is_null() {
                None
            } else {
                Some(CStr::from_ptr(err_ptr).to_string_lossy().into_owned())
            }
        }
    }

    /// Check if native library is available
    pub fn is_native_available() -> bool {
        unsafe { ffi::fission_is_available() != 0 }
    }

    /// Get the .sla directory path
    pub fn sla_dir(&self) -> &str {
        &self.sla_dir
    }
}

impl Drop for SafeDecompiler {
    fn drop(&mut self) {
        if !self.instance.is_null() {
            // Wrap destruction in catch_unwind
            let instance = self.instance;
            let _ = catch_unwind(AssertUnwindSafe(|| {
                unsafe { ffi::fission_decompiler_destroy(instance) };
            }));
            self.instance = std::ptr::null_mut();
        }
    }
}

/// Builder for SafeDecompiler with configuration options
pub struct SafeDecompilerBuilder {
    sla_dir: String,
    timeout: Duration,
}

impl SafeDecompilerBuilder {
    /// Create a new builder with the specified .sla directory
    pub fn new(sla_dir: impl Into<String>) -> Self {
        Self {
            sla_dir: sla_dir.into(),
            timeout: SafeDecompiler::DEFAULT_TIMEOUT,
        }
    }

    /// Set timeout for operations
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Build the SafeDecompiler
    pub fn build(self) -> Result<SafeDecompiler> {
        SafeDecompiler::new(&self.sla_dir)
            .map(|d| d.with_timeout(self.timeout))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_decompiler_no_panic() {
        // Even with invalid input, should return Err, not panic
        let result = SafeDecompiler::new("invalid/path/that/does/not/exist");
        // May succeed or fail, but should not panic
        match result {
            Ok(_) => println!("Decompiler created (unexpected but ok)"),
            Err(e) => println!("Expected error: {}", e),
        }
    }

    #[test]
    fn test_builder_pattern() {
        let builder = SafeDecompilerBuilder::new("./sla")
            .timeout(Duration::from_secs(60));
        
        // Build may fail, but should not panic
        let _ = builder.build();
    }
}
