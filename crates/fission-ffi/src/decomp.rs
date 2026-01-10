//! Decompiler FFI Bridge - Native Ghidra Decompiler Bindings
//!
//! This module provides Rust bindings to the Ghidra decompiler library,
//! enabling in-process decompilation without subprocess overhead.
//!
//! All unsafe FFI operations for the decompiler are isolated here.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::ptr;

use fission_core::prelude::*;

// ============================================================================
// FFI Type Definitions
// ============================================================================

/// Opaque handle to decompiler context (C struct)
#[repr(C)]
pub struct DecompContext {
    _private: [u8; 0],
}

#[repr(C)]
struct DecompSymbolInfo {
    address: u64,
    size: u32,
    flags: u32,
    name: *const c_char,
    name_len: u32,
}

type DecompFindSymbolFn = extern "C" fn(
    userdata: *mut std::ffi::c_void,
    address: u64,
    size: u32,
    require_start: c_int,
    out: *mut DecompSymbolInfo,
) -> c_int;

type DecompFindFunctionFn = extern "C" fn(
    userdata: *mut std::ffi::c_void,
    address: u64,
    out: *mut DecompSymbolInfo,
) -> c_int;

#[repr(C)]
struct DecompSymbolProvider {
    userdata: *mut std::ffi::c_void,
    find_symbol: Option<DecompFindSymbolFn>,
    find_function: Option<DecompFindFunctionFn>,
    drop: Option<extern "C" fn(*mut std::ffi::c_void)>,
}

unsafe impl Send for DecompSymbolProvider {}

const SYMBOL_FLAG_FUNCTION: u32 = 1 << 0;
const SYMBOL_FLAG_DATA: u32 = 1 << 1;
const SYMBOL_FLAG_EXTERNAL: u32 = 1 << 2;
const SYMBOL_FLAG_READONLY: u32 = 1 << 3;
#[allow(dead_code)]
const SYMBOL_FLAG_VOLATILE: u32 = 1 << 4;

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
unsafe extern "C" {
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
    fn decomp_add_global_symbol(ctx: *mut DecompContext, addr: u64, name: *const c_char);
    fn decomp_clear_global_symbols(ctx: *mut DecompContext);
    fn decomp_set_symbol_provider(ctx: *mut DecompContext, provider: *const DecompSymbolProvider);
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
    fn decomp_function_pcode(ctx: *mut DecompContext, addr: u64) -> *mut c_char;
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
    pointer_size: Option<u32>,
    symbol_provider_state: Option<Box<SymbolProviderState>>,
    symbol_provider_callbacks: Option<DecompSymbolProvider>,
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
            pointer_size: None,
            symbol_provider_state: None,
            symbol_provider_callbacks: None,
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

    /// Add multiple symbols from IAT or symbol table
    pub fn add_symbols(&mut self, symbols: &HashMap<u64, String>) {
        if self.check_valid().is_err() {
            return;
        }
        for (addr, name) in symbols {
            self.add_symbol(*addr, name);
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

    /// Add multiple global data symbols
    pub fn add_global_symbols(&mut self, symbols: &HashMap<u64, String>) {
        if self.check_valid().is_err() {
            return;
        }
        for (addr, name) in symbols {
            self.add_global_symbol(*addr, name);
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
struct SymbolProviderEntry {
    name: CString,
    size: u32,
    flags: u32,
}

#[cfg(feature = "native_decomp")]
struct SymbolProviderRange {
    start: u64,
    end: u64,
    entry_addr: u64,
}

#[cfg(feature = "native_decomp")]
struct SymbolProviderState {
    functions: HashMap<u64, SymbolProviderEntry>,
    data: HashMap<u64, SymbolProviderEntry>,
    function_ranges: Vec<SymbolProviderRange>,
    data_ranges: Vec<SymbolProviderRange>,
}

#[cfg(feature = "native_decomp")]
impl SymbolProviderState {
    fn new(
        functions: &[fission_loader::loader::FunctionInfo],
        data_symbols: &HashMap<u64, String>,
        sections: &[fission_loader::loader::SectionInfo],
        pointer_size: Option<u32>,
    ) -> Self {
        let mut function_map = HashMap::new();
        let mut function_ranges = Vec::new();
        let mut function_addrs: Vec<u64> = functions
            .iter()
            .filter_map(|func| {
                if func.address == 0 {
                    None
                } else {
                    Some(func.address)
                }
            })
            .collect();
        function_addrs.sort_unstable();
        function_addrs.dedup();

        let mut function_sizes = HashMap::new();
        for (idx, addr) in function_addrs.iter().enumerate() {
            let next_addr = function_addrs.get(idx + 1).copied();
            let section = match find_executable_section_for_address(*addr, sections) {
                Some(section) => section,
                None => continue,
            };
            let (_, end) = match section_range(section) {
                Some(range) => range,
                None => continue,
            };
            if let Some(next) = next_addr {
                if next > *addr && next < end {
                    let size = next - *addr;
                    if let Ok(size_u32) = u32::try_from(size) {
                        if size_u32 > 0 {
                            function_sizes.insert(*addr, size_u32);
                        }
                    }
                }
            }
        }
        for func in functions {
            if func.address == 0 || func.name.is_empty() {
                continue;
            }
            if let Ok(name) = CString::new(func.name.as_str()) {
                let mut size = func.size.min(u32::MAX as u64) as u32;
                if size == 0 {
                    if let Some(estimated) = function_sizes.get(&func.address) {
                        size = *estimated;
                    }
                }
                let mut flags = SYMBOL_FLAG_FUNCTION;
                if func.is_import {
                    flags |= SYMBOL_FLAG_EXTERNAL;
                }
                function_map.insert(
                    func.address,
                    SymbolProviderEntry {
                        name,
                        size,
                        flags,
                    },
                );

                if size > 0 {
                    if let Some(range) = build_range(func.address, size as u64) {
                        function_ranges.push(range);
                    }
                }
            }
        }

        let mut data_map = HashMap::new();
        let mut data_ranges = Vec::new();
        let mut data_addrs: Vec<u64> = data_symbols.keys().copied().collect();
        data_addrs.sort_unstable();
        let mut data_sizes = HashMap::new();
        for (idx, addr) in data_addrs.iter().enumerate() {
            let next_addr = data_addrs.get(idx + 1).copied();
            let mut size = estimate_data_size(*addr, next_addr, sections).unwrap_or(1);
            if size == 0 {
                size = 1;
            }
            data_sizes.insert(*addr, size);
        }
        for (addr, name) in data_symbols {
            if *addr == 0 || name.is_empty() {
                continue;
            }
            if let Ok(name_cstr) = CString::new(name.as_str()) {
                let mut flags = data_flags_for_address(*addr, sections);
                let lower = name.to_ascii_lowercase();
                let is_import = lower.starts_with("__imp_") || lower.starts_with("__imp__");
                if is_import {
                    flags |= SYMBOL_FLAG_EXTERNAL;
                }
                let mut size = data_sizes.get(addr).copied().unwrap_or(1);
                if let Some(ptr_size) = pointer_size {
                    if is_import && ptr_size > 0 {
                        size = ptr_size;
                    }
                }
                data_map.insert(
                    *addr,
                    SymbolProviderEntry {
                        name: name_cstr,
                        size,
                        flags,
                    },
                );

                if let Some(range) = build_range(*addr, size as u64) {
                    data_ranges.push(range);
                }
            }
        }

        function_ranges.sort_by_key(|range| range.start);
        data_ranges.sort_by_key(|range| range.start);

        Self {
            functions: function_map,
            data: data_map,
            function_ranges,
            data_ranges,
        }
    }
}

#[cfg(feature = "native_decomp")]
fn data_flags_for_address(
    addr: u64,
    sections: &[fission_loader::loader::SectionInfo],
) -> u32 {
    if let Some(section) = find_section_for_address(addr, sections) {
        let mut flags = SYMBOL_FLAG_DATA;
        if !section.is_writable {
            flags |= SYMBOL_FLAG_READONLY;
        }
        return flags;
    }

    SYMBOL_FLAG_DATA
}

#[cfg(feature = "native_decomp")]
fn estimate_data_size(
    addr: u64,
    next_addr: Option<u64>,
    sections: &[fission_loader::loader::SectionInfo],
) -> Option<u32> {
    let section = find_section_for_address(addr, sections)?;
    let (_, end) = section_range(section)?;
    if let Some(next) = next_addr {
        if next > addr && next < end {
            let delta = next - addr;
            if let Ok(delta_u32) = u32::try_from(delta) {
                if delta_u32 > 0 {
                    return Some(delta_u32);
                }
            }
        }
    }
    None
}

#[cfg(feature = "native_decomp")]
fn find_section_for_address<'a>(
    addr: u64,
    sections: &'a [fission_loader::loader::SectionInfo],
) -> Option<&'a fission_loader::loader::SectionInfo> {
    for section in sections {
        if let Some((start, end)) = section_range(section) {
            if addr >= start && addr < end {
                return Some(section);
            }
        }
    }
    None
}

#[cfg(feature = "native_decomp")]
fn find_executable_section_for_address<'a>(
    addr: u64,
    sections: &'a [fission_loader::loader::SectionInfo],
) -> Option<&'a fission_loader::loader::SectionInfo> {
    for section in sections {
        if !section.is_executable {
            continue;
        }
        if let Some((start, end)) = section_range(section) {
            if addr >= start && addr < end {
                return Some(section);
            }
        }
    }
    None
}

#[cfg(feature = "native_decomp")]
fn section_range(section: &fission_loader::loader::SectionInfo) -> Option<(u64, u64)> {
    let size = if section.virtual_size > 0 {
        section.virtual_size
    } else {
        section.file_size
    };
    if size == 0 {
        return None;
    }
    let start = section.virtual_address;
    let end = start.saturating_add(size);
    if end <= start {
        return None;
    }
    Some((start, end))
}

#[cfg(feature = "native_decomp")]
fn build_range(start: u64, size: u64) -> Option<SymbolProviderRange> {
    if size == 0 {
        return None;
    }

    let mut end = start.saturating_add(size);
    if end <= start {
        end = start.saturating_add(1);
    }

    Some(SymbolProviderRange {
        start,
        end,
        entry_addr: start,
    })
}

#[cfg(feature = "native_decomp")]
fn find_range_entry(ranges: &[SymbolProviderRange], address: u64) -> Option<u64> {
    if ranges.is_empty() {
        return None;
    }

    let idx = ranges.partition_point(|range| range.start <= address);
    if idx == 0 {
        return None;
    }

    let range = &ranges[idx - 1];
    if address < range.end {
        Some(range.entry_addr)
    } else {
        None
    }
}

#[cfg(feature = "native_decomp")]
extern "C" fn symbol_provider_find_symbol(
    userdata: *mut std::ffi::c_void,
    address: u64,
    _size: u32,
    require_start: c_int,
    out: *mut DecompSymbolInfo,
) -> c_int {
    if userdata.is_null() || out.is_null() {
        return 0;
    }

    let state = unsafe { &*(userdata as *const SymbolProviderState) };
    let entry = match state.data.get(&address) {
        Some(entry) => entry,
        None => {
            if require_start == 0 {
                if let Some(start) = find_range_entry(&state.data_ranges, address) {
                    match state.data.get(&start) {
                        Some(entry) => entry,
                        None => return 0,
                    }
                } else {
                    return 0;
                }
            } else {
                return 0;
            }
        }
    };

    unsafe {
        (*out).address = address;
        (*out).size = entry.size;
        (*out).flags = entry.flags;
        (*out).name = entry.name.as_ptr();
        (*out).name_len = entry.name.as_bytes().len().min(u32::MAX as usize) as u32;
    }

    1
}

#[cfg(feature = "native_decomp")]
extern "C" fn symbol_provider_find_function(
    userdata: *mut std::ffi::c_void,
    address: u64,
    out: *mut DecompSymbolInfo,
) -> c_int {
    if userdata.is_null() || out.is_null() {
        return 0;
    }

    let state = unsafe { &*(userdata as *const SymbolProviderState) };
    let entry = match state.functions.get(&address) {
        Some(entry) => entry,
        None => {
            if let Some(start) = find_range_entry(&state.function_ranges, address) {
                match state.functions.get(&start) {
                    Some(entry) => entry,
                    None => return 0,
                }
            } else {
                return 0;
            }
        }
    };

    unsafe {
        (*out).address = address;
        (*out).size = entry.size;
        (*out).flags = entry.flags;
        (*out).name = entry.name.as_ptr();
        (*out).name_len = entry.name.as_bytes().len().min(u32::MAX as usize) as u32;
    }

    1
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
