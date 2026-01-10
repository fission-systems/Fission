/**
 * Fission Decompiler FFI Implementation
 * 
 * C++ implementation of the FFI interface defined in libdecomp_ffi.h.
 * Refactored to use modular components.
 */

#include "fission/ffi/libdecomp_ffi.h"
#include "fission/ffi/DecompContext.h"
#include "fission/ffi/MemoryManager.h"
#include "fission/ffi/SymbolManager.h"
#include "fission/ffi/SymbolProviderManager.h"
#include "fission/ffi/FidManager.h"
#include "fission/ffi/DecompilerCore.h"

#include <cstring>

using namespace fission::ffi;

// ============================================================================
// Lifecycle Management
// ============================================================================

extern "C" DECOMP_API DecompContext* decomp_create(const char* sla_dir) {
    return create_context(sla_dir);
}

extern "C" DECOMP_API void decomp_destroy(DecompContext* ctx) {
    destroy_context(ctx);
}

// ============================================================================
// Binary Loading
// ============================================================================

extern "C" DECOMP_API DecompError decomp_load_binary(
    DecompContext* ctx,
    const uint8_t* data,
    size_t len,
    uint64_t base_addr,
    int is_64bit
) {
    return load_binary(ctx, data, len, base_addr, is_64bit != 0);
}

// ============================================================================
// Symbol Management
// ============================================================================

extern "C" DECOMP_API void decomp_add_symbol(
    DecompContext* ctx,
    uint64_t addr,
    const char* name
) {
    add_symbol(ctx, addr, name);
}

extern "C" DECOMP_API void decomp_clear_symbols(DecompContext* ctx) {
    clear_symbols(ctx);
}

extern "C" DECOMP_API void decomp_add_global_symbol(
    DecompContext* ctx,
    uint64_t addr,
    const char* name
) {
    add_global_symbol(ctx, addr, name);
}

extern "C" DECOMP_API void decomp_clear_global_symbols(DecompContext* ctx) {
    clear_global_symbols(ctx);
}

// Batch symbol registration for reduced FFI overhead
extern "C" DECOMP_API void decomp_add_symbols_batch(
    DecompContext* ctx,
    const uint64_t* addrs,
    const char* const* names,
    size_t count
) {
    if (!ctx || !addrs || !names) return;
    for (size_t i = 0; i < count; ++i) {
        if (names[i]) {
            add_symbol(ctx, addrs[i], names[i]);
        }
    }
}

extern "C" DECOMP_API void decomp_add_global_symbols_batch(
    DecompContext* ctx,
    const uint64_t* addrs,
    const char* const* names,
    size_t count
) {
    if (!ctx || !addrs || !names) return;
    for (size_t i = 0; i < count; ++i) {
        if (names[i]) {
            add_global_symbol(ctx, addrs[i], names[i]);
        }
    }
}

extern "C" DECOMP_API void decomp_set_symbol_provider(
    DecompContext* ctx,
    const DecompSymbolProvider* provider
) {
    set_symbol_provider(ctx, provider);
}

extern "C" DECOMP_API void decomp_reset_symbol_provider(DecompContext* ctx) {
    reset_symbol_provider(ctx);
}

extern "C" DECOMP_API DecompError decomp_add_function(
    DecompContext* ctx,
    uint64_t addr,
    const char* name
) {
    return add_function(ctx, addr, name);
}

// ============================================================================
// Memory Block Management
// ============================================================================

extern "C" DECOMP_API DecompError decomp_add_memory_block(
    DecompContext* ctx,
    const char* name,
    uint64_t va_addr,
    uint64_t va_size,
    uint64_t file_offset,
    uint64_t file_size,
    int is_executable,
    int is_writable
) {
    return add_memory_block(
        ctx, name, va_addr, va_size, 
        file_offset, file_size,
        is_executable != 0, is_writable != 0
    );
}

// ============================================================================
// Decompilation
// ============================================================================

extern "C" DECOMP_API char* decomp_function(DecompContext* ctx, uint64_t addr) {
    if (!ctx) return nullptr;
    
    std::lock_guard<std::mutex> lock(ctx->mutex);
    
    try {
        std::string result = run_decompilation(ctx, addr);
        
        // Allocate and copy result (caller must free)
        char* output = static_cast<char*>(malloc(result.size() + 1));
        if (output) {
            std::memcpy(output, result.c_str(), result.size() + 1);
        }
        return output;
    } catch (const std::exception& e) {
        ctx->last_error = std::string("Error: ") + e.what();
        return nullptr;
    } catch (...) {
        ctx->last_error = "Unknown decompilation error";
        return nullptr;
    }
}

extern "C" DECOMP_API char* decomp_function_pcode(DecompContext* ctx, uint64_t addr) {
    if (!ctx) return nullptr;
    
    std::lock_guard<std::mutex> lock(ctx->mutex);
    
    try {
        std::string result = run_decompilation_pcode(ctx, addr);
        
        char* output = static_cast<char*>(malloc(result.size() + 1));
        if (output) {
            std::memcpy(output, result.c_str(), result.size() + 1);
        }
        return output;
    } catch (const std::exception& e) {
        ctx->last_error = std::string("Error: ") + e.what();
        return nullptr;
    } catch (...) {
        ctx->last_error = "Unknown decompilation error";
        return nullptr;
    }
}

extern "C" DECOMP_API const char* decomp_get_last_error(DecompContext* ctx) {
    if (!ctx) return "Invalid context";
    return ctx->last_error.c_str();
}

extern "C" DECOMP_API void decomp_free_string(char* str) {
    if (str) {
        free(str);
    }
}

// ============================================================================
// Configuration
// ============================================================================

extern "C" DECOMP_API DecompError decomp_set_gdt(DecompContext* ctx, const char* gdt_path) {
    if (!ctx) return DECOMP_ERR_INVALID_CONTEXT;
    
    set_gdt_path(ctx, gdt_path);
    return DECOMP_OK;
}

extern "C" DECOMP_API void decomp_set_feature(
    DecompContext* ctx,
    const char* feature,
    int enabled
) {
    set_feature(ctx, feature, enabled != 0);
}

// ============================================================================
// FID Support
// ============================================================================

extern "C" DECOMP_API DecompError decomp_load_fid_db(DecompContext* ctx, const char* db_path) {
    return load_fid_database(ctx, db_path);
}

extern "C" DECOMP_API char* decomp_get_fid_match(DecompContext* ctx, uint64_t addr, size_t len) {
    return get_fid_match(ctx, addr, len);
}
