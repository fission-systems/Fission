/**
 * Fission Decompiler FFI - Unified Entry Point
 * 
 * Consolidated FFI interface combining:
 * - SymbolManager: Symbol table and function declarations
 * - FidManager: FID database loading and matching
 * - MemoryManager: Binary loading and memory block management
 * 
 * This replaces the previous modular approach with a single, streamlined interface.
 */

#ifndef FISSION_FFI_DECOMPILER_FFI_H
#define FISSION_FFI_DECOMPILER_FFI_H

#include <cstdint>
#include "fission/ffi/DecompContext.h"
#include "fission/ffi/libdecomp_ffi.h"

namespace fission {
namespace ffi {

// ===========================================================================
// Memory Management API
// ===========================================================================

/**
 * Load a binary into the decompiler context
 * @param ctx Decompiler context
 * @param data Raw binary data
 * @param len Length of binary data
 * @param base_addr Base address (image base)
 * @param is_64bit True for 64-bit, false for 32-bit
 * @param sleigh_id Sleigh language ID (optional)
 * @param compiler_id Compiler spec ID (optional)
 * @return DECOMP_OK on success, error code otherwise
 */
DecompError load_binary(
    DecompContext* ctx,
    const uint8_t* data,
    size_t len,
    uint64_t base_addr,
    bool is_64bit,
    const char* sleigh_id = nullptr,
    const char* compiler_id = nullptr
);

/**
 * Add a memory block (section) to the context
 * @param ctx Decompiler context
 * @param name Section name
 * @param va_addr Virtual address
 * @param va_size Virtual size
 * @param file_offset File offset
 * @param file_size File size
 * @param is_executable Is executable section
 * @param is_writable Is writable section
 * @return DECOMP_OK on success, error code otherwise
 */
DecompError add_memory_block(
    DecompContext* ctx,
    const char* name,
    uint64_t va_addr,
    uint64_t va_size,
    uint64_t file_offset,
    uint64_t file_size,
    bool is_executable,
    bool is_writable
);

// ===========================================================================
// Symbol Management API
// ===========================================================================

/**
 * Add a symbol to the context
 * @param ctx Decompiler context
 * @param addr Symbol address
 * @param name Symbol name
 */
void add_symbol(DecompContext* ctx, uint64_t addr, const char* name);

/**
 * Clear all symbols from the context
 * @param ctx Decompiler context
 */
void clear_symbols(DecompContext* ctx);

/**
 * Add a global data symbol to the context
 * @param ctx Decompiler context
 * @param addr Symbol address
 * @param name Symbol name
 */
void add_global_symbol(DecompContext* ctx, uint64_t addr, const char* name);

/**
 * Clear all global data symbols from the context
 * @param ctx Decompiler context
 */
void clear_global_symbols(DecompContext* ctx);

/**
 * Add a function declaration
 * @param ctx Decompiler context
 * @param addr Function address
 * @param name Function name (optional, will generate if null)
 * @return DECOMP_OK on success, error code otherwise
 */
DecompError add_function(DecompContext* ctx, uint64_t addr, const char* name);

// ===========================================================================
// FID (Function ID) Management API
// ===========================================================================

/**
 * Load a FID database
 * @param ctx Decompiler context
 * @param db_path Path to FID database file
 * @return DECOMP_OK on success, error code otherwise
 */
DecompError load_fid_database(DecompContext* ctx, const char* db_path);

/**
 * Get FID match for a function
 * @param ctx Decompiler context
 * @param addr Function address
 * @param len Function length
 * @return Matched function name (caller must free) or nullptr
 */
char* get_fid_match(DecompContext* ctx, uint64_t addr, size_t len);

} // namespace ffi
} // namespace fission

#endif // FISSION_FFI_DECOMPILER_FFI_H
