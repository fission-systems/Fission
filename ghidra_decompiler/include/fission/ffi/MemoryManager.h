/**
 * Fission Memory Manager
 * 
 * Handles binary loading and memory block management.
 * Separated from libdecomp_ffi.cpp for better modularity.
 */

#ifndef FISSION_FFI_MEMORY_MANAGER_H
#define FISSION_FFI_MEMORY_MANAGER_H

#include "fission/ffi/DecompContext.h"
#include "fission/ffi/libdecomp_ffi.h"

namespace fission {
namespace ffi {

/**
 * Load a binary into the decompiler context
 * @param ctx Decompiler context
 * @param data Raw binary data
 * @param len Length of binary data
 * @param base_addr Base address (image base)
 * @param is_64bit True for 64-bit, false for 32-bit
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

} // namespace ffi
} // namespace fission

#endif // FISSION_FFI_MEMORY_MANAGER_H
