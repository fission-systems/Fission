/**
 * Fission Decompiler FFI Interface
 * 
 * C-compatible interface for calling the Ghidra decompiler from Rust.
 * This header defines the public API for libdecomp shared library.
 */

#ifndef FISSION_LIBDECOMP_FFI_H
#define FISSION_LIBDECOMP_FFI_H

#include <cstdint>
#include <stdint.h>
#include <stddef.h>
#include "fission/ffi/SymbolProviderFfi.h"

#ifdef __cplusplus
extern "C" {
#endif

// Platform-specific export macro
#if defined(_WIN32) || defined(_WIN64)
    #ifdef DECOMP_EXPORTS
        #define DECOMP_API __declspec(dllexport)
    #else
        #define DECOMP_API __declspec(dllimport)
    #endif
#else
    #define DECOMP_API __attribute__((visibility("default")))
#endif

// Forward declaration - actual definition in DecompContext.h
namespace fission {
namespace ffi {
    struct DecompContext;
}
}

// Use fission::ffi::DecompContext in C API
using DecompContext = fission::ffi::DecompContext;

// Error codes
typedef enum DecompError {
    DECOMP_OK = 0,
    DECOMP_ERR_INIT = -1,
    DECOMP_ERR_LOAD = -2,
    DECOMP_ERR_DECOMPILE = -3,
    DECOMP_ERR_INVALID_CONTEXT = -4,
    DECOMP_ERR_OUT_OF_MEMORY = -5,
    DECOMP_ERR_FID_LOAD = -6,
} DecompError;

// ============================================================================
// Lifecycle Management
// ============================================================================

/**
 * Create a new decompiler context.
 * 
 * @param sla_dir Path to directory containing .sla files (Sleigh specs)
 * @return New context handle, or NULL on failure
 */
DECOMP_API DecompContext* decomp_create(const char* sla_dir);

/**
 * Destroy a decompiler context and free all resources.
 * 
 * @param ctx Context to destroy (safe to pass NULL)
 */
DECOMP_API void decomp_destroy(DecompContext* ctx);

// ============================================================================
// Binary Loading
// ============================================================================

/**
 * Load a complete binary into the decompiler context.
 * This establishes the memory image for all subsequent decompilations.
 * 
 * @param ctx Decompiler context
 * @param data Raw binary data
 * @param len Length of binary data in bytes
 * @param base_addr Base address (image base) for the binary
 * @param is_64bit Non-zero for 64-bit, zero for 32-bit
 * @return DECOMP_OK on success, error code on failure
 */
DECOMP_API DecompError decomp_load_binary(
    DecompContext* ctx,
    const uint8_t* data,
    size_t len,
    uint64_t base_addr,
    int is_64bit,
    const char* sleigh_id,
    const char* compiler_id
);

// ============================================================================
// Symbol Management
// ============================================================================

/**
 * Add a symbol (function name) at the given address.
 * Used for IAT symbols, user renames, etc.
 * 
 * @param ctx Decompiler context
 * @param addr Address of the symbol
 * @param name Symbol name (will be copied internally)
 */
DECOMP_API void decomp_add_symbol(
    DecompContext* ctx,
    uint64_t addr,
    const char* name
);

/**
 * Clear all symbols from the context.
 * 
 * @param ctx Decompiler context
 */
DECOMP_API void decomp_clear_symbols(DecompContext* ctx);

/**
 * Add a global data symbol at the given address.
 * Used for refptr/pg renames and data symbol cleanup.
 *
 * @param ctx Decompiler context
 * @param addr Address of the symbol
 * @param name Symbol name (will be copied internally)
 */
DECOMP_API void decomp_add_global_symbol(
    DecompContext* ctx,
    uint64_t addr,
    const char* name
);

/**
 * Clear all global data symbols from the context.
 *
 * @param ctx Decompiler context
 */
DECOMP_API void decomp_clear_global_symbols(DecompContext* ctx);

/**
 * Add multiple symbols in a single FFI call (batch optimization).
 * This reduces FFI overhead when adding many symbols at once.
 *
 * @param ctx Decompiler context
 * @param addrs Array of symbol addresses
 * @param names Array of symbol names (each will be copied internally)
 * @param count Number of symbols to add
 */
DECOMP_API void decomp_add_symbols_batch(
    DecompContext* ctx,
    const uint64_t* addrs,
    const char* const* names,
    size_t count
);

/**
 * Add multiple global data symbols in a single FFI call (batch optimization).
 *
 * @param ctx Decompiler context
 * @param addrs Array of symbol addresses
 * @param names Array of symbol names (each will be copied internally)
 * @param count Number of symbols to add
 */
DECOMP_API void decomp_add_global_symbols_batch(
    DecompContext* ctx,
    const uint64_t* addrs,
    const char* const* names,
    size_t count
);

/**
 * Set a symbol provider callback for on-demand symbol queries.
 *
 * @param ctx Decompiler context
 * @param provider Pointer to provider callbacks (NULL to disable)
 */
DECOMP_API void decomp_set_symbol_provider(
    DecompContext* ctx,
    const DecompSymbolProvider* provider
);

/**
 * Reset the symbol provider to the default map-backed provider.
 * This does not clear existing symbol/global symbol maps.
 *
 * @param ctx Decompiler context
 */
DECOMP_API void decomp_reset_symbol_provider(DecompContext* ctx);

/**
 * Declare a function at the given address.
 * This helps Ghidra recognize function boundaries and improves
 * decompilation quality by pre-defining known function locations.
 * 
 * @param ctx Decompiler context
 * @param addr Address where the function starts
 * @param name Optional function name (NULL for auto-generated)
 * @return DECOMP_OK on success, error code on failure
 */
DECOMP_API DecompError decomp_add_function(
    DecompContext* ctx,
    uint64_t addr,
    const char* name
);

/**
 * Add a memory block (section) to the decompiler context.
 * This helps Ghidra understand the memory layout and distinguish
 * between code and data sections, and maps virtual addresses to file offsets.
 * 
 * @param ctx Decompiler context
 * @param name Section name (e.g., ".text", ".data")
 * @param va_addr Virtual address of the section
 * @param va_size Size of the section in virtual memory
 * @param file_offset Offset of section in PE file
 * @param file_size Size of section in PE file
 * @param is_executable Whether this section contains executable code
 * @param is_writable Whether this section is writable
 * @return DECOMP_OK on success, error code on failure
 */
DECOMP_API DecompError decomp_add_memory_block(
    DecompContext* ctx,
    const char* name,
    uint64_t va_addr,
    uint64_t va_size,
    uint64_t file_offset,
    uint64_t file_size,
    int is_executable,
    int is_writable
);

// ============================================================================
// Decompilation
// ============================================================================

/**
 * Decompile a function at the given address.
 * 
 * @param ctx Decompiler context (must have binary loaded)
 * @param addr Start address of the function
 * @return Allocated C string with decompiled code, or NULL on error.
 *         Caller must free with decomp_free_string().
 */
DECOMP_API char* decomp_function(DecompContext* ctx, uint64_t addr);

/**
 * Generate Pcode JSON for a function at the given address.
 *
 * @param ctx Decompiler context (must have binary loaded)
 * @param addr Start address of the function
 * @return Allocated C string with Pcode JSON, or NULL on error.
 *         Caller must free with decomp_free_string().
 */
DECOMP_API char* decomp_function_pcode(DecompContext* ctx, uint64_t addr);

/**
 * Get the last error message.
 * 
 * @param ctx Decompiler context
 * @return Error message string (do NOT free this, it's internal)
 */
DECOMP_API const char* decomp_get_last_error(DecompContext* ctx);

// ============================================================================
// Memory Management
// ============================================================================

/**
 * Free a string returned by decomp_function().
 * 
 * @param str String to free (safe to pass NULL)
 */
DECOMP_API void decomp_free_string(char* str);

// ============================================================================
// Configuration
// ============================================================================

/**
 * Set GDT (Ghidra Data Type) file path for type information.
 * 
 * @param ctx Decompiler context
 * @param gdt_path Path to .gdt file
 * @return DECOMP_OK on success
 */
DECOMP_API DecompError decomp_set_gdt(DecompContext* ctx, const char* gdt_path);

/**
 * Enable or disable specific analysis passes.
 * 
 * @param ctx Decompiler context
 * @param feature Feature name (e.g., "infer_pointers", "analyze_loops",
 *                "readonly_propagate", "record_jumploads",
 *                "disable_toomanyinstructions_error")
 * @param enabled Non-zero to enable, zero to disable
 */
DECOMP_API void decomp_set_feature(
    DecompContext* ctx,
    const char* feature,
    int enabled
);

/**
 * Set per-function inline flag (OptionInline-like behavior).
 *
 * @param ctx Decompiler context
 * @param addr Function start address
 * @param enabled Non-zero to mark inline, zero to clear inline
 * @return DECOMP_OK on success, error code on failure
 */
DECOMP_API DecompError decomp_set_function_inline(
    DecompContext* ctx,
    uint64_t addr,
    int enabled
);

/**
 * Set per-function noreturn flag (OptionNoReturn-like behavior).
 *
 * @param ctx Decompiler context
 * @param addr Function start address
 * @param enabled Non-zero to mark noreturn, zero to clear noreturn
 * @return DECOMP_OK on success, error code on failure
 */
DECOMP_API DecompError decomp_set_function_noreturn(
    DecompContext* ctx,
    uint64_t addr,
    int enabled
);

/**
 * Set per-function extrapop value in bytes (OptionExtraPop-like behavior).
 *
 * @param ctx Decompiler context
 * @param addr Function start address
 * @param extrapop Stack cleanup bytes after call
 * @return DECOMP_OK on success, error code on failure
 */
DECOMP_API DecompError decomp_set_function_extrapop(
    DecompContext* ctx,
    uint64_t addr,
    int32_t extrapop
);

/**
 * Set architecture default prototype model (OptionDefaultPrototype-like behavior).
 *
 * @param ctx Decompiler context
 * @param model_name Prototype model name (e.g. "default", "__cdecl", "__fastcall")
 * @return DECOMP_OK on success, error code on failure
 */
DECOMP_API DecompError decomp_set_default_prototype(
    DecompContext* ctx,
    const char* model_name
);

/**
 * Set prototype evaluation model for current function analysis (OptionProtoEval-like behavior).
 *
 * @param ctx Decompiler context
 * @param model_name Prototype model name, or "default"
 * @return DECOMP_OK on success, error code on failure
 */
DECOMP_API DecompError decomp_set_protoeval_current(
    DecompContext* ctx,
    const char* model_name
);

/**
 * Set prototype evaluation model for called function analysis.
 *
 * @param ctx Decompiler context
 * @param model_name Prototype model name, or "default"
 * @return DECOMP_OK on success, error code on failure
 */
DECOMP_API DecompError decomp_set_protoeval_called(
    DecompContext* ctx,
    const char* model_name
);

// ============================================================================
// FID (Function ID) Analysis
// ============================================================================

/**
 * Load a Function ID database (.fidbf file).
 * 
 * @param ctx Decompiler context
 * @param db_path Path to .fidbf file
 * @return DECOMP_OK on success, DECOMP_ERR_FID_LOAD on failure
 */
DECOMP_API DecompError decomp_load_fid_db(DecompContext* ctx, const char* db_path);

/**
 * Try to match a function using FID.
 * 
 * @param ctx Decompiler context
 * @param addr Address of the function start
 * @param len Length of the function code in bytes
 * @return Allocated string with matched function name, or NULL if no match.
 *         Caller must free with decomp_free_string().
 */
DECOMP_API char* decomp_get_fid_match(DecompContext* ctx, uint64_t addr, size_t len);

// ============================================================================
// Type Registration (for metadata-driven type recovery)
// ============================================================================

/**
 * Field information for struct registration
 */
typedef struct DecompFieldInfo {
    const char* name;      ///< Field name
    const char* type_name; ///< Field type name (e.g., "int", "char*")
    uint32_t offset;       ///< Offset in bytes from struct start
    uint32_t size;         ///< Field size in bytes
} DecompFieldInfo;

/**
 * Register a struct type with the decompiler's type system.
 * This enables proper field access rendering (e.g., ptr->field).
 * 
 * @param ctx Decompiler context
 * @param name Struct type name (e.g., "FissionSwiftTester")
 * @param size Total size of the struct in bytes
 * @param fields Array of field descriptors
 * @param field_count Number of fields
 * @return DECOMP_OK on success, error code on failure
 */
DECOMP_API DecompError decomp_register_struct_type(
    DecompContext* ctx,
    const char* name,
    uint32_t size,
    const DecompFieldInfo* fields,
    size_t field_count
);

/**
 * Apply a registered struct type to a function parameter.
 * This marks the first parameter of the function as a pointer to the given struct.
 * 
 * @param ctx Decompiler context  
 * @param func_addr Address of the function
 * @param param_index Parameter index (0 for first param, typically 'self'/'this')
 * @param struct_name Name of the struct type to apply
 * @return DECOMP_OK on success, error code on failure
 */
DECOMP_API DecompError decomp_apply_struct_to_param(
    DecompContext* ctx,
    uint64_t func_addr,
    int param_index,
    const char* struct_name
);

/**
 * Register Rust Pcode-optimization function pointers with the C++ bridge.
 *
 * Call this once at process startup (from Rust, before any decompilation)
 * to supply the actual function pointers instead of relying on dlsym.
 * Passing NULL for either argument disables Pcode optimization.
 *
 * @param optimize_fn  Pointer to fission_optimize_pcode_json, or NULL
 * @param free_fn      Pointer to fission_free_string, or NULL
 */
DECOMP_API void decomp_init_pcode_bridge(
    char* (*optimize_fn)(const char*, size_t),
    void  (*free_fn)(char*)
);

#ifdef __cplusplus
}
#endif

#endif // FISSION_LIBDECOMP_FFI_H

