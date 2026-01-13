#ifndef FISSION_CORE_CONTEXT_SERVICES_H
#define FISSION_CORE_CONTEXT_SERVICES_H

#include <cstddef>
#include <cstdint>

#include "fission/ffi/DecompContext.h"
#include "fission/ffi/libdecomp_ffi.h"

namespace fission {
namespace core {

void add_symbol(fission::ffi::DecompContext* ctx, uint64_t addr, const char* name);
void clear_symbols(fission::ffi::DecompContext* ctx);

void add_global_symbol(fission::ffi::DecompContext* ctx, uint64_t addr, const char* name);
void clear_global_symbols(fission::ffi::DecompContext* ctx);

DecompError add_function(fission::ffi::DecompContext* ctx, uint64_t addr, const char* name);

void set_symbol_provider(fission::ffi::DecompContext* ctx, const DecompSymbolProvider* provider);
void reset_symbol_provider(fission::ffi::DecompContext* ctx);

DecompError load_binary(
    fission::ffi::DecompContext* ctx,
    const uint8_t* data,
    size_t len,
    uint64_t base_addr,
    bool is_64bit,
    const char* sleigh_id = nullptr,
    const char* compiler_id = nullptr
);

DecompError add_memory_block(
    fission::ffi::DecompContext* ctx,
    const char* name,
    uint64_t va_addr,
    uint64_t va_size,
    uint64_t file_offset,
    uint64_t file_size,
    bool is_executable,
    bool is_writable
);

} // namespace core
} // namespace fission

#endif // FISSION_CORE_CONTEXT_SERVICES_H
