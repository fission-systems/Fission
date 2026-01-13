/**
 * Fission Memory Manager Implementation
 */

#include "fission/ffi/MemoryManager.h"
#include "fission/core/ContextServices.h"

using namespace fission::ffi;

DecompError fission::ffi::load_binary(
    DecompContext* ctx,
    const uint8_t* data,
    size_t len,
    uint64_t base_addr,
    bool is_64bit,
    const char* sleigh_id,
    const char* compiler_id
) {
    return fission::core::load_binary(ctx, data, len, base_addr, is_64bit, sleigh_id, compiler_id);
}

DecompError fission::ffi::add_memory_block(
    DecompContext* ctx,
    const char* name,
    uint64_t va_addr,
    uint64_t va_size,
    uint64_t file_offset,
    uint64_t file_size,
    bool is_executable,
    bool is_writable
) {
    return fission::core::add_memory_block(
        ctx,
        name,
        va_addr,
        va_size,
        file_offset,
        file_size,
        is_executable,
        is_writable
    );
}
