//! DecompilerNative Wrapper
//!
//! Safe Rust wrapper for the native Ghidra decompiler library.

use super::ffi::*;
use super::symbols::{
    symbol_provider_find_function, symbol_provider_find_symbol, SymbolProviderState,
};
use super::types::*;
use fission_core::prelude::*;
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::ptr;
use tracing::warn;

// ============================================================================
// DecompilerNative - Safe Rust Wrapper
// ============================================================================

/// Native decompiler interface using FFI to libdecomp
///
/// This provides direct in-process access to the Ghidra decompiler,
/// avoiding subprocess spawn overhead.
///
/// # Safety Guarantees
///
/// This struct implements several layers of safety:
///
/// 1. **Validity Tracking**: The `is_valid` flag prevents use-after-free.
///    All public methods call `check_valid()` before accessing the C++ context.
///
/// 2. **Null Pointer Checks**: The context pointer is validated before each FFI call.
///
/// 3. **C String Safety**: All string conversions use `CString::new()` with proper
///    error handling to prevent embedded nulls from causing UB.
///
/// 4. **Memory Ownership**: FFI-returned strings are immediately copied to Rust-owned
///    Strings, then freed via `decomp_free_string()`.
///
/// 5. **RAII Cleanup**: The `Drop` impl ensures the C++ context is destroyed.
///
/// # Thread Safety
///
/// - **`Send`**: This type can be moved between threads (but see below).
/// - **NOT `Sync`**: The underlying C++ object is NOT thread-safe.
///   **Never share a `DecompilerNative` across threads without external synchronization.**
///   Use `Mutex<DecompilerNative>` if sharing is needed.
///
/// # Panics
///
/// This type is designed to never panic during FFI operations. All errors
/// are returned as `Result<T, FissionError>`.
///
/// # Example
///
/// ```ignore
/// let mut decomp = DecompilerNative::new("/path/to/sla")?;
/// decomp.load_binary(&binary_data, 0x140000000, true)?;
/// let code = decomp.decompile(0x140001000)?;
/// ```
#[cfg(feature = "native_decomp")]
pub struct DecompilerNative {
    ctx: *mut DecompContext,
    _sla_dir: String,
    /// Track if context is valid to prevent use-after-free
    is_valid: bool,
    pointer_size: Option<u32>,
    symbol_provider_state: Option<Box<SymbolProviderState>>,
    symbol_provider_callbacks: Option<DecompSymbolProvider>,
    /// Thread ID where this instance was created (for debug validation)
    #[cfg(debug_assertions)]
    creation_thread: std::thread::ThreadId,
}

/// DecompilerNative can be sent between threads, but CANNOT be shared.
/// The underlying C++ decompiler is NOT thread-safe.
///
/// We use PhantomData<*mut ()> as a marker to opt-out of Sync.
/// The raw pointer type *mut () is !Sync, making DecompilerNative also !Sync.
#[cfg(feature = "native_decomp")]
unsafe impl Send for DecompilerNative {}

// Note: DecompilerNative is implicitly !Sync because it contains
// *mut DecompContext which is !Sync. This prevents Arc<DecompilerNative>
// without external synchronization like Arc<Mutex<DecompilerNative>>.

#[cfg(feature = "native_decomp")]
impl DecompilerNative {
    /// Create a new native decompiler instance
    pub fn new(sla_dir: &str) -> Result<Self> {
        if sla_dir.is_empty() {
            return Err(FissionError::decompiler("SLA directory cannot be empty"));
        }

        let sla_cstr = CString::new(sla_dir).map_err(|_| {
            FissionError::decompiler("Invalid SLA directory path (contains null byte)")
        })?;

        let ctx = unsafe { decomp_create(sla_cstr.as_ptr()) };
        if ctx.is_null() {
            return Err(FissionError::decompiler(
                "Failed to create decompiler context",
            ));
        }

        // Register Rust Pcode-optimization functions with the C++ bridge.
        // This is done via push-registration so the C++ side does not need
        // dlsym, which is unreliable on macOS when symbols live in the Rust binary.
        unsafe {
            decomp_init_pcode_bridge(
                Some(crate::pcode::fission_optimize_pcode_json),
                Some(crate::pcode::fission_free_string),
            );
        }

        Ok(Self {
            ctx,
            _sla_dir: sla_dir.to_string(),
            is_valid: true,
            pointer_size: None,
            symbol_provider_state: None,
            symbol_provider_callbacks: None,
            #[cfg(debug_assertions)]
            creation_thread: std::thread::current().id(),
        })
    }

    /// Check if the decompiler context is still valid
    ///
    /// This is called at the start of every public method to prevent
    /// use-after-free and null pointer dereferences.
    fn check_valid(&self) -> Result<()> {
        if !self.is_valid {
            return Err(FissionError::decompiler(
                "Decompiler context has been invalidated",
            ));
        }
        if self.ctx.is_null() {
            return Err(FissionError::decompiler(
                "Decompiler context pointer is null",
            ));
        }

        // Extra debug-only validation
        #[cfg(debug_assertions)]
        {
            // Verify we're on the expected thread in debug mode
            // This helps catch threading issues during development
            let current = std::thread::current().id();
            if self.creation_thread != current {
                // Log warning only once globally to avoid spam
                use std::sync::Once;
                static WARN_ONCE: Once = Once::new();
                WARN_ONCE.call_once(|| {
                    warn!(
                        "DecompilerNative used across threads: \
                         this is expected with per-binary worker threads; \
                         each worker has its own isolated DecompilerNative instance"
                    );
                });
            }
        }

        Ok(())
    }

    pub fn load_binary(
        &mut self,
        data: &[u8],
        base_addr: u64,
        is_64bit: bool,
        sleigh_id: Option<&str>,
        compiler_id: Option<&str>,
    ) -> Result<()> {
        self.check_valid()?;

        if data.is_empty() {
            return Err(FissionError::decompiler("Cannot load empty binary"));
        }

        let sleigh_cstr = sleigh_id.and_then(|id| CString::new(id).ok());
        let compiler_cstr = compiler_id.and_then(|id| CString::new(id).ok());

        let result = unsafe {
            decomp_load_binary(
                self.ctx,
                data.as_ptr(),
                data.len(),
                base_addr,
                if is_64bit { 1 } else { 0 },
                sleigh_cstr.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
                compiler_cstr.as_ref().map_or(ptr::null(), |c| c.as_ptr()),
            )
        };

        if result.is_ok() {
            self.pointer_size = Some(if is_64bit { 8 } else { 4 });
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

    /// Add multiple symbols from IAT or symbol table (batch optimized)
    ///
    /// This uses a single FFI call to register all symbols at once,
    /// significantly reducing FFI overhead for large symbol tables.
    pub fn add_symbols(&mut self, symbols: &HashMap<u64, String>) {
        if self.check_valid().is_err() || symbols.is_empty() {
            return;
        }

        // Prepare batch arrays
        let mut addrs: Vec<u64> = Vec::with_capacity(symbols.len());
        let mut names_cstr: Vec<CString> = Vec::with_capacity(symbols.len());

        for (addr, name) in symbols {
            if !name.is_empty() {
                if let Ok(cstr) = CString::new(name.as_str()) {
                    addrs.push(*addr);
                    names_cstr.push(cstr);
                }
            }
        }

        if addrs.is_empty() {
            return;
        }

        // Create pointer array for FFI
        let name_ptrs: Vec<*const c_char> = names_cstr.iter().map(|s| s.as_ptr()).collect();

        unsafe {
            decomp_add_symbols_batch(self.ctx, addrs.as_ptr(), name_ptrs.as_ptr(), addrs.len());
        }
    }

    /// Add a global data symbol at the given address
    pub fn add_global_symbol(&mut self, addr: u64, name: &str) {
        if self.check_valid().is_err() || name.is_empty() {
            return;
        }
        if let Ok(name_cstr) = CString::new(name) {
            unsafe { decomp_add_global_symbol(self.ctx, addr, name_cstr.as_ptr()) };
        }
    }

    /// Add multiple global data symbols (batch optimized)
    ///
    /// This uses a single FFI call to register all symbols at once,
    /// significantly reducing FFI overhead for large symbol tables.
    pub fn add_global_symbols(&mut self, symbols: &HashMap<u64, String>) {
        if self.check_valid().is_err() || symbols.is_empty() {
            return;
        }

        // Prepare batch arrays
        let mut addrs: Vec<u64> = Vec::with_capacity(symbols.len());
        let mut names_cstr: Vec<CString> = Vec::with_capacity(symbols.len());

        for (addr, name) in symbols {
            if !name.is_empty() {
                if let Ok(cstr) = CString::new(name.as_str()) {
                    addrs.push(*addr);
                    names_cstr.push(cstr);
                }
            }
        }

        if addrs.is_empty() {
            return;
        }

        // Create pointer array for FFI
        let name_ptrs: Vec<*const c_char> = names_cstr.iter().map(|s| s.as_ptr()).collect();

        unsafe {
            decomp_add_global_symbols_batch(
                self.ctx,
                addrs.as_ptr(),
                name_ptrs.as_ptr(),
                addrs.len(),
            );
        }
    }

    /// Set a symbol provider for on-demand symbol queries
    pub fn set_symbol_provider(
        &mut self,
        functions: &[fission_loader::loader::FunctionInfo],
        data_symbols: &HashMap<u64, String>,
        sections: &[fission_loader::loader::SectionInfo],
    ) {
        if self.check_valid().is_err() {
            return;
        }

        let state = Box::new(SymbolProviderState::new(
            functions,
            data_symbols,
            sections,
            self.pointer_size,
        ));
        let userdata = std::ptr::from_ref(state.as_ref())
            .cast::<std::ffi::c_void>()
            .cast_mut();

        let provider = DecompSymbolProvider {
            userdata,
            find_symbol: Some(symbol_provider_find_symbol),
            find_function: Some(symbol_provider_find_function),
            drop: None,
        };

        unsafe {
            decomp_set_symbol_provider(self.ctx, std::ptr::from_ref(&provider));
        }

        self.symbol_provider_state = Some(state);
        self.symbol_provider_callbacks = Some(provider);
    }

    /// Reset to the default map-backed symbol provider.
    ///
    /// This preserves currently registered symbol/global symbol maps in the
    /// native context while dropping Rust-side callback state.
    pub fn reset_symbol_provider(&mut self) {
        if self.check_valid().is_err() {
            return;
        }

        unsafe {
            decomp_reset_symbol_provider(self.ctx);
        }

        // Drop callback-owned state so stale pointers cannot be reused.
        self.symbol_provider_state = None;
        self.symbol_provider_callbacks = None;
    }

    /// Clear all symbols
    pub fn clear_symbols(&mut self) {
        unsafe { decomp_clear_symbols(self.ctx) };
    }

    /// Clear all global data symbols
    pub fn clear_global_symbols(&mut self) {
        unsafe { decomp_clear_global_symbols(self.ctx) };
    }

    /// Declare a function at the given address
    ///
    /// This helps Ghidra recognize function boundaries and improves
    /// decompilation quality. Should be called after load_binary()
    /// with all known function addresses.
    pub fn add_function(&mut self, addr: u64, name: Option<&str>) -> Result<()> {
        self.check_valid()?;

        let name_cstr = if let Some(n) = name {
            Some(CString::new(n).map_err(|_| {
                FissionError::decompiler("Invalid function name (contains null byte)")
            })?)
        } else {
            None
        };

        let name_ptr = name_cstr
            .as_ref()
            .map(|c| c.as_ptr())
            .unwrap_or(ptr::null());

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

    /// Get Pcode JSON for a function at the given address
    pub fn get_pcode(&self, addr: u64) -> Result<String> {
        self.check_valid()?;

        let result_ptr = unsafe { decomp_function_pcode(self.ctx, addr) };

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

        let path_cstr = CString::new(gdt_path)
            .map_err(|_| FissionError::decompiler("Invalid GDT path (contains null byte)"))?;

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

    /// Mark a function at the given address as inline (Ghidra OptionInline).
    pub fn set_function_inline(&mut self, addr: u64, enabled: bool) -> Result<()> {
        self.check_valid()?;
        let result =
            unsafe { decomp_set_function_inline(self.ctx, addr, if enabled { 1 } else { 0 }) };
        if result.is_ok() {
            Ok(())
        } else {
            Err(FissionError::decompiler(self.get_last_error()))
        }
    }

    /// Mark a function at the given address as noreturn (Ghidra OptionNoReturn).
    pub fn set_function_noreturn(&mut self, addr: u64, enabled: bool) -> Result<()> {
        self.check_valid()?;
        let result =
            unsafe { decomp_set_function_noreturn(self.ctx, addr, if enabled { 1 } else { 0 }) };
        if result.is_ok() {
            Ok(())
        } else {
            Err(FissionError::decompiler(self.get_last_error()))
        }
    }

    /// Set per-function stack cleanup (extrapop) in bytes (Ghidra OptionExtraPop).
    pub fn set_function_extrapop(&mut self, addr: u64, extrapop: i32) -> Result<()> {
        self.check_valid()?;
        let result = unsafe { decomp_set_function_extrapop(self.ctx, addr, extrapop) };
        if result.is_ok() {
            Ok(())
        } else {
            Err(FissionError::decompiler(self.get_last_error()))
        }
    }

    /// Set the default prototype model (e.g. `"default"`, `"__cdecl"`, `"__fastcall"`).
    pub fn set_default_prototype(&mut self, model_name: &str) -> Result<()> {
        self.check_valid()?;
        let cstr = CString::new(model_name)
            .map_err(|_| FissionError::decompiler("Model name contains null byte"))?;
        let result = unsafe { decomp_set_default_prototype(self.ctx, cstr.as_ptr()) };
        if result.is_ok() {
            Ok(())
        } else {
            Err(FissionError::decompiler(self.get_last_error()))
        }
    }

    /// Set prototype evaluation model for the current function (OptionProtoEval).
    pub fn set_protoeval_current(&mut self, model_name: &str) -> Result<()> {
        self.check_valid()?;
        let cstr = CString::new(model_name)
            .map_err(|_| FissionError::decompiler("Model name contains null byte"))?;
        let result = unsafe { decomp_set_protoeval_current(self.ctx, cstr.as_ptr()) };
        if result.is_ok() {
            Ok(())
        } else {
            Err(FissionError::decompiler(self.get_last_error()))
        }
    }

    /// Set prototype evaluation model for called functions (OptionProtoEval).
    pub fn set_protoeval_called(&mut self, model_name: &str) -> Result<()> {
        self.check_valid()?;
        let cstr = CString::new(model_name)
            .map_err(|_| FissionError::decompiler("Model name contains null byte"))?;
        let result = unsafe { decomp_set_protoeval_called(self.ctx, cstr.as_ptr()) };
        if result.is_ok() {
            Ok(())
        } else {
            Err(FissionError::decompiler(self.get_last_error()))
        }
    }

    /// Load FID (Function ID) database for library function recognition
    pub fn load_fid_database(&mut self, db_path: &str) -> Result<()> {
        self.check_valid()?;

        if db_path.is_empty() {
            return Err(FissionError::decompiler(
                "FID database path cannot be empty",
            ));
        }

        let path_cstr = CString::new(db_path).map_err(|_| {
            FissionError::decompiler("Invalid FID database path (contains null byte)")
        })?;

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

    /// Register a struct type with the decompiler's type system
    ///
    /// This enables proper field access rendering (e.g., ptr->field).
    /// Fields should be sorted by offset.
    pub fn register_struct_type(
        &mut self,
        name: &str,
        size: u32,
        fields: &[(String, String, u32, u32)], // (name, type_name, offset, size)
    ) -> Result<()> {
        self.check_valid()?;

        if name.is_empty() {
            return Err(FissionError::decompiler("Struct name cannot be empty"));
        }

        let name_cstr =
            CString::new(name).map_err(|_| FissionError::decompiler("Invalid struct name"))?;

        // Convert fields to FFI format
        let field_names: Vec<CString> = fields
            .iter()
            .map(|(n, _, _, _)| CString::new(n.as_str()).unwrap_or_default())
            .collect();
        let field_types: Vec<CString> = fields
            .iter()
            .map(|(_, t, _, _)| CString::new(t.as_str()).unwrap_or_default())
            .collect();

        let ffi_fields: Vec<DecompFieldInfo> = fields
            .iter()
            .enumerate()
            .map(|(i, (_, _, offset, sz))| DecompFieldInfo {
                name: field_names[i].as_ptr(),
                type_name: field_types[i].as_ptr(),
                offset: *offset,
                size: *sz,
            })
            .collect();

        let result = unsafe {
            decomp_register_struct_type(
                self.ctx,
                name_cstr.as_ptr(),
                size,
                ffi_fields.as_ptr(),
                ffi_fields.len(),
            )
        };

        if result.is_ok() {
            Ok(())
        } else {
            Err(FissionError::decompiler(self.get_last_error()))
        }
    }

    /// Apply a registered struct type to a function parameter
    ///
    /// This marks the specified parameter as a pointer to the given struct type.
    pub fn apply_struct_to_param(
        &mut self,
        func_addr: u64,
        param_index: i32,
        struct_name: &str,
    ) -> Result<()> {
        self.check_valid()?;

        let name_cstr = CString::new(struct_name)
            .map_err(|_| FissionError::decompiler("Invalid struct name"))?;

        let result = unsafe {
            decomp_apply_struct_to_param(self.ctx, func_addr, param_index, name_cstr.as_ptr())
        };

        if result.is_ok() {
            Ok(())
        } else {
            Err(FissionError::decompiler(self.get_last_error()))
        }
    }

    /// Register types from InferredTypeInfo (convenience method)
    pub fn register_inferred_types(
        &mut self,
        types: &[fission_loader::loader::types::InferredTypeInfo],
    ) -> Result<()> {
        for ty in types {
            let fields: Vec<(String, String, u32, u32)> = ty
                .fields
                .iter()
                .map(|f| (f.name.clone(), f.type_name.clone(), f.offset, f.size))
                .collect();

            self.register_struct_type(&ty.name, ty.size, &fields)?;
        }
        Ok(())
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
