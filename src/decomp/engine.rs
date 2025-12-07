//! Decompiler Engine - Ghidra Sleigh Backend
//!
//! Provides decompilation via the native Ghidra C++ engine.
//! No fallback - requires Ghidra native library to be linked.

use anyhow::{anyhow, Result};
use std::ffi::{CStr, CString};
use std::ptr;

use crate::decomp::ffi;

/// Represents a decompiled function
#[derive(Debug, Clone)]
pub struct DecompiledFunction {
    /// Function name (auto-generated from address if unknown)
    pub name: String,
    /// Entry point address
    pub address: u64,
    /// Function signature  
    pub signature: String,
    /// Decompiled C code body
    pub body: String,
}

/// Represents disassembled instructions
#[derive(Debug, Clone)]
pub struct DisassemblyResult {
    /// Base address of disassembly
    pub address: u64,
    /// Disassembly text output
    pub text: String,
    /// Number of instructions disassembled
    pub instruction_count: usize,
}

/// High-level decompiler interface using Ghidra Sleigh engine
pub struct Decompiler {
    /// Pointer to native C++ decompiler instance
    native_instance: *mut ffi::FissionDecompiler,
    /// Path to .sla specification files
    sla_dir: String,
    /// Whether the decompiler is initialized
    initialized: bool,
}

// SAFETY: The native instance is protected by internal mutex in C++ code
unsafe impl Send for Decompiler {}
unsafe impl Sync for Decompiler {}

impl Decompiler {
    /// Create a new decompiler with the specified .sla directory
    /// 
    /// # Arguments
    /// * `sla_dir` - Path to directory containing .sla specification files
    ///               (e.g., x86-64.sla from Ghidra installation)
    /// 
    /// # Returns
    /// * `Ok(Decompiler)` if initialization succeeded
    /// * `Err` if native library unavailable or initialization failed
    pub fn new(sla_dir: impl Into<String>) -> Result<Self> {
        let sla_dir = sla_dir.into();
        
        let c_sla_dir = CString::new(sla_dir.clone())
            .map_err(|_| anyhow!("Invalid sla_dir path"))?;
        
        let instance = unsafe { ffi::fission_decompiler_init(c_sla_dir.as_ptr()) };
        
        if instance.is_null() {
            let error = Self::get_last_error()
                .unwrap_or_else(|| "Unknown error initializing decompiler".to_string());
            return Err(anyhow!("Failed to initialize Ghidra decompiler: {}", error));
        }
        
        Ok(Self {
            native_instance: instance,
            sla_dir,
            initialized: true,
        })
    }

    /// Create a decompiler with default .sla directory (looks in current dir)
    pub fn with_default_path() -> Result<Self> {
        Self::new("./sla")
    }

    /// Check if the decompiler is properly initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized && !self.native_instance.is_null()
    }

    /// Decompile a function from raw bytes
    /// 
    /// # Arguments
    /// * `bytes` - Raw machine code bytes
    /// * `address` - Virtual address of the function entry point
    /// 
    /// # Returns
    /// * `Ok(DecompiledFunction)` with decompiled C code
    /// * `Err` if decompilation failed
    pub fn decompile(&self, bytes: &[u8], address: u64) -> Result<DecompiledFunction> {
        if !self.is_initialized() {
            return Err(anyhow!("Decompiler not initialized"));
        }
        
        if bytes.is_empty() {
            return Err(anyhow!("Empty input bytes"));
        }
        
        // Allocate output buffer (16KB should be enough for most functions)
        let mut buffer: Vec<u8> = vec![0; 16 * 1024];
        
        let result = unsafe {
            ffi::fission_decompile(
                self.native_instance,
                bytes.as_ptr(),
                bytes.len(),
                address,
                buffer.as_mut_ptr() as *mut i8,
                buffer.len(),
            )
        };
        
        if result < 0 {
            let error = Self::get_last_error()
                .unwrap_or_else(|| "Decompilation failed".to_string());
            return Err(anyhow!("{}", error));
        }
        
        // Convert buffer to string
        let body = unsafe {
            CStr::from_ptr(buffer.as_ptr() as *const i8)
                .to_string_lossy()
                .into_owned()
        };
        
        Ok(DecompiledFunction {
            name: format!("func_{:x}", address),
            address,
            signature: format!("void func_{:x}(void)", address),
            body,
        })
    }

    /// Disassemble instructions from raw bytes
    /// 
    /// # Arguments
    /// * `bytes` - Raw machine code bytes
    /// * `address` - Virtual address of the first instruction
    /// 
    /// # Returns
    /// * `Ok(DisassemblyResult)` with disassembly text
    /// * `Err` if disassembly failed
    pub fn disassemble(&self, bytes: &[u8], address: u64) -> Result<DisassemblyResult> {
        if !self.is_initialized() {
            return Err(anyhow!("Decompiler not initialized"));
        }
        
        if bytes.is_empty() {
            return Err(anyhow!("Empty input bytes"));
        }
        
        // Allocate output buffer
        let mut buffer: Vec<u8> = vec![0; 32 * 1024];
        
        let result = unsafe {
            ffi::fission_disassemble(
                self.native_instance,
                bytes.as_ptr(),
                bytes.len(),
                address,
                buffer.as_mut_ptr() as *mut i8,
                buffer.len(),
            )
        };
        
        if result < 0 {
            let error = Self::get_last_error()
                .unwrap_or_else(|| "Disassembly failed".to_string());
            return Err(anyhow!("{}", error));
        }
        
        // Convert buffer to string
        let text = unsafe {
            CStr::from_ptr(buffer.as_ptr() as *const i8)
                .to_string_lossy()
                .into_owned()
        };
        
        // Count instructions (each line is one instruction)
        let instruction_count = text.lines().count();
        
        Ok(DisassemblyResult {
            address,
            text,
            instruction_count,
        })
    }

    /// Get the .sla directory path
    pub fn sla_dir(&self) -> &str {
        &self.sla_dir
    }

    /// Get the last error message from the native library
    fn get_last_error() -> Option<String> {
        unsafe {
            let err_ptr = ffi::fission_get_error();
            if err_ptr.is_null() {
                None
            } else {
                Some(CStr::from_ptr(err_ptr).to_string_lossy().into_owned())
            }
        }
    }

    /// Check if the native library is available
    pub fn is_native_available() -> bool {
        unsafe { ffi::fission_is_available() != 0 }
    }
}

impl Drop for Decompiler {
    fn drop(&mut self) {
        if !self.native_instance.is_null() {
            unsafe {
                ffi::fission_decompiler_destroy(self.native_instance);
            }
            self.native_instance = ptr::null_mut();
        }
    }
}

/// Stub decompiler for when native library is not linked
/// This will be used during development before C++ integration is complete
pub struct StubDecompiler;

impl StubDecompiler {
    pub fn new() -> Self {
        Self
    }

    /// Generate placeholder decompilation output
    pub fn decompile(&self, _bytes: &[u8], address: u64) -> DecompiledFunction {
        DecompiledFunction {
            name: format!("func_{:x}", address),
            address,
            signature: format!("void func_{:x}(void)", address),
            body: format!(
                "// [Stub] Native Ghidra decompiler not linked\n\
                 // Build with: cargo build --features native_decomp\n\
                 // Requires .sla files from Ghidra installation\n\n\
                 void func_{:x}(void) {{\n\
                 \t// Decompilation unavailable\n\
                 }}\n",
                address
            ),
        }
    }

    /// Generate placeholder disassembly output
    pub fn disassemble(&self, bytes: &[u8], address: u64) -> DisassemblyResult {
        let mut text = String::from("; [Stub] Native Ghidra disassembler not linked\n");
        text.push_str("; Raw bytes:\n");
        
        for (i, chunk) in bytes.chunks(16).enumerate() {
            let addr = address + (i * 16) as u64;
            text.push_str(&format!("{:08x}:  ", addr));
            for byte in chunk {
                text.push_str(&format!("{:02x} ", byte));
            }
            text.push('\n');
        }
        
        DisassemblyResult {
            address,
            text,
            instruction_count: 0,
        }
    }
}

impl Default for StubDecompiler {
    fn default() -> Self {
        Self::new()
    }
}
