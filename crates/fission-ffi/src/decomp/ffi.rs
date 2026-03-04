//! FFI Function Declarations
//!
//! External C function declarations for libdecomp.

use super::types::*;
use std::os::raw::{c_char, c_int};

// ============================================================================
// External FFI Function Declarations
// ============================================================================

#[cfg(feature = "native_decomp")]
#[link(name = "decomp")]
unsafe extern "C" {
    pub(super) fn decomp_create(sla_dir: *const c_char) -> *mut DecompContext;
    pub(super) fn decomp_destroy(ctx: *mut DecompContext);
    pub(super) fn decomp_load_binary(
        ctx: *mut DecompContext,
        data: *const u8,
        len: usize,
        base_addr: u64,
        is_64bit: c_int,
        sleigh_id: *const c_char,
        compiler_id: *const c_char,
    ) -> DecompError;
    pub(super) fn decomp_add_symbol(ctx: *mut DecompContext, addr: u64, name: *const c_char);
    pub(super) fn decomp_clear_symbols(ctx: *mut DecompContext);
    pub(super) fn decomp_add_global_symbol(ctx: *mut DecompContext, addr: u64, name: *const c_char);
    pub(super) fn decomp_clear_global_symbols(ctx: *mut DecompContext);
    pub(super) fn decomp_set_symbol_provider(
        ctx: *mut DecompContext,
        provider: *const DecompSymbolProvider,
    );
    pub(super) fn decomp_reset_symbol_provider(ctx: *mut DecompContext);
    pub(super) fn decomp_add_function(
        ctx: *mut DecompContext,
        addr: u64,
        name: *const c_char,
    ) -> DecompError;
    pub(super) fn decomp_add_memory_block(
        ctx: *mut DecompContext,
        name: *const c_char,
        va_addr: u64,
        va_size: u64,
        file_offset: u64,
        file_size: u64,
        is_executable: c_int,
        is_writable: c_int,
    ) -> DecompError;
    pub(super) fn decomp_function(ctx: *mut DecompContext, addr: u64) -> *mut c_char;
    pub(super) fn decomp_function_pcode(ctx: *mut DecompContext, addr: u64) -> *mut c_char;
    pub(super) fn decomp_free_string(s: *mut c_char);
    pub(super) fn decomp_get_last_error(ctx: *mut DecompContext) -> *const c_char;
    pub(super) fn decomp_set_gdt(ctx: *mut DecompContext, gdt_path: *const c_char) -> DecompError;
    pub(super) fn decomp_set_feature(
        ctx: *mut DecompContext,
        feature: *const c_char,
        enabled: c_int,
    );
    pub(super) fn decomp_load_fid_db(
        ctx: *mut DecompContext,
        db_path: *const c_char,
    ) -> DecompError;
    pub(super) fn decomp_get_fid_match(
        ctx: *mut DecompContext,
        addr: u64,
        len: usize,
    ) -> *mut c_char;

    // Pcode optimization bridge initialisation.
    // Registers Rust function pointers with the C++ side so dlsym is not needed.
    pub(super) fn decomp_init_pcode_bridge(
        optimize_fn: Option<unsafe extern "C" fn(*const c_char, usize) -> *mut c_char>,
        free_fn: Option<unsafe extern "C" fn(*mut c_char)>,
    );

    // Batch symbol registration (reduced FFI overhead)
    pub(super) fn decomp_add_symbols_batch(
        ctx: *mut DecompContext,
        addrs: *const u64,
        names: *const *const c_char,
        count: usize,
    );
    pub(super) fn decomp_add_global_symbols_batch(
        ctx: *mut DecompContext,
        addrs: *const u64,
        names: *const *const c_char,
        count: usize,
    );

    // Type registration for metadata-driven type recovery
    pub(super) fn decomp_register_struct_type(
        ctx: *mut DecompContext,
        name: *const c_char,
        size: u32,
        fields: *const DecompFieldInfo,
        field_count: usize,
    ) -> DecompError;

    pub(super) fn decomp_apply_struct_to_param(
        ctx: *mut DecompContext,
        func_addr: u64,
        param_index: c_int,
        struct_name: *const c_char,
    ) -> DecompError;

    // Per-function and prototype options (Ghidra OptionInline / OptionNoReturn / etc.)
    pub(super) fn decomp_set_function_inline(
        ctx: *mut DecompContext,
        addr: u64,
        enabled: c_int,
    ) -> DecompError;
    pub(super) fn decomp_set_function_noreturn(
        ctx: *mut DecompContext,
        addr: u64,
        enabled: c_int,
    ) -> DecompError;
    pub(super) fn decomp_set_function_extrapop(
        ctx: *mut DecompContext,
        addr: u64,
        extrapop: i32,
    ) -> DecompError;
    pub(super) fn decomp_set_default_prototype(
        ctx: *mut DecompContext,
        model_name: *const c_char,
    ) -> DecompError;
    pub(super) fn decomp_set_protoeval_current(
        ctx: *mut DecompContext,
        model_name: *const c_char,
    ) -> DecompError;
    pub(super) fn decomp_set_protoeval_called(
        ctx: *mut DecompContext,
        model_name: *const c_char,
    ) -> DecompError;
}

// ============================================================================
// Public API
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
