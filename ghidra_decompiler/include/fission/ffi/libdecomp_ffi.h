/**
 * Fission Decompiler FFI Interface
 * 
 * C-compatible interface for calling the Ghidra decompiler from Rust.
 * This header defines the public API for libdecomp shared library.
 */

#ifndef FISSION_LIBDECOMP_FFI_H
#define FISSION_LIBDECOMP_FFI_H

#include <stdint.h>
#include <stddef.h>

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

// Opaque handle to decompiler context
typedef struct DecompContext DecompContext;

// Error codes
typedef enum DecompError {
    DECOMP_OK = 0,
    DECOMP_ERR_INIT = -1,
    DECOMP_ERR_LOAD = -2,
    DECOMP_ERR_DECOMPILE = -3,
    DECOMP_ERR_INVALID_CONTEXT = -4,
    DECOMP_ERR_OUT_OF_MEMORY = -5,
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
    int is_64bit
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
 * @param feature Feature name (e.g., "infer_pointers", "analyze_loops")
 * @param enabled Non-zero to enable, zero to disable
 */
DECOMP_API void decomp_set_feature(
    DecompContext* ctx,
    const char* feature,
    int enabled
);

#ifdef __cplusplus
}
#endif

#endif // FISSION_LIBDECOMP_FFI_H
