#include "fission/core/ContextServices.h"

#include "fission/loader/SectionAwareLoadImage.h"
#include "fission/core/SymbolProvider.h"

#include "libdecomp.hh"

#include <iostream>
#include <sstream>

namespace fission {
namespace core {

using fission::ffi::DecompContext;
using fission::loader::SectionAwareLoadImage;
using fission::core::CallbackSymbolProvider;
using fission::core::MapSymbolProvider;

void add_symbol(DecompContext* ctx, uint64_t addr, const char* name) {
    if (!ctx || !name) {
        return;
    }

    std::lock_guard<std::mutex> lock(ctx->mutex);
    ctx->symbols[addr] = name;
}

void clear_symbols(DecompContext* ctx) {
    if (!ctx) {
        return;
    }

    std::lock_guard<std::mutex> lock(ctx->mutex);
    ctx->symbols.clear();
}

void add_global_symbol(DecompContext* ctx, uint64_t addr, const char* name) {
    if (!ctx || !name) {
        return;
    }

    std::lock_guard<std::mutex> lock(ctx->mutex);
    ctx->global_symbols[addr] = name;
}

void clear_global_symbols(DecompContext* ctx) {
    if (!ctx) {
        return;
    }

    std::lock_guard<std::mutex> lock(ctx->mutex);
    ctx->global_symbols.clear();
}

DecompError add_function(DecompContext* ctx, uint64_t addr, const char* name) {
    if (!ctx) {
        return DECOMP_ERR_INVALID_CONTEXT;
    }

    std::lock_guard<std::mutex> lock(ctx->mutex);

    try {
        std::string func_name = name ? name : ("FUN_" + std::to_string(addr));
        ctx->symbols[addr] = func_name;

        if (ctx->arch && ctx->memory_image) {
            ghidra::Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
            if (global_scope) {
                ghidra::Address func_addr(ctx->arch->getDefaultCodeSpace(), addr);
                const ghidra::Funcdata* existing = global_scope->findFunction(func_addr);
                if (!existing) {
                    global_scope->addFunction(func_addr, func_name);
                    std::cerr << "[SymbolManager] Declared function at 0x" << std::hex << addr
                              << std::dec << ": " << func_name << std::endl;
                }
            }
        }

        return DECOMP_OK;
    } catch (const std::exception& e) {
        ctx->last_error = std::string("Failed to add function: ") + e.what();
        return DECOMP_ERR_DECOMPILE;
    } catch (...) {
        ctx->last_error = "Unknown error in add_function";
        return DECOMP_ERR_DECOMPILE;
    }
}

void set_symbol_provider(DecompContext* ctx, const DecompSymbolProvider* provider) {
    if (!ctx) {
        return;
    }

    std::lock_guard<std::mutex> lock(ctx->mutex);

    if (ctx->symbol_provider_enabled && ctx->symbol_provider_callbacks.drop) {
        ctx->symbol_provider_callbacks.drop(ctx->symbol_provider_callbacks.userdata);
    }

    if (!provider) {
        ctx->symbol_provider_callbacks = DecompSymbolProvider{};
        ctx->symbol_provider_enabled = false;
        ctx->symbol_provider.reset();

        if (ctx->arch) {
            ctx->symbol_provider = std::make_unique<MapSymbolProvider>(
                &ctx->symbols,
                &ctx->global_symbols
            );
            ctx->arch->setSymbolProvider(ctx->symbol_provider.get());
        }
        return;
    }

    ctx->symbol_provider_callbacks = *provider;
    ctx->symbol_provider_enabled = true;
    ctx->symbol_provider = std::make_unique<CallbackSymbolProvider>(
        &ctx->symbol_provider_callbacks
    );

    if (ctx->arch) {
        ctx->arch->setSymbolProvider(ctx->symbol_provider.get());
    }
}

void reset_symbol_provider(DecompContext* ctx) {
    set_symbol_provider(ctx, nullptr);
}

DecompError load_binary(
    DecompContext* ctx,
    const uint8_t* data,
    size_t len,
    uint64_t base_addr,
    bool is_64bit
) {
    if (!ctx) {
        return DECOMP_ERR_INVALID_CONTEXT;
    }

    std::lock_guard<std::mutex> lock(ctx->mutex);

    try {
        ctx->binary_data.assign(data, data + len);
        ctx->memory_image = std::make_unique<SectionAwareLoadImage>(ctx->binary_data);
        ctx->base_addr = base_addr;
        ctx->is_64bit = is_64bit;
        ctx->arch.reset();
        return DECOMP_OK;
    } catch (const std::exception& e) {
        ctx->last_error = e.what();
        return DECOMP_ERR_LOAD;
    } catch (...) {
        ctx->last_error = "Unknown error during binary load";
        return DECOMP_ERR_LOAD;
    }
}

DecompError add_memory_block(
    DecompContext* ctx,
    const char* name,
    uint64_t va_addr,
    uint64_t va_size,
    uint64_t file_offset,
    uint64_t file_size,
    bool is_executable,
    bool is_writable
) {
    if (!ctx || !name) {
        return DECOMP_ERR_INVALID_CONTEXT;
    }

    std::lock_guard<std::mutex> lock(ctx->mutex);

    try {
        fission::ffi::MemoryBlockInfo block;
        block.name = name;
        block.va_addr = va_addr;
        block.va_size = va_size;
        block.file_offset = file_offset;
        block.file_size = file_size;
        block.is_executable = is_executable;
        block.is_writable = is_writable;

        ctx->memory_blocks.push_back(block);

        if (ctx->memory_image) {
            ctx->memory_image->addSection(
                va_addr,
                va_size,
                file_offset,
                file_size,
                is_executable,
                is_writable,
                block.name
            );
        }

        std::cerr << "[MemoryManager] Registered memory block: " << name
                  << " at VA 0x" << std::hex << va_addr << std::dec
                  << " (vsize: " << va_size << ", file_off: 0x" << std::hex << file_offset
                  << std::dec << ", fsize: " << file_size << ", "
                  << (block.is_executable ? "executable" : "data")
                  << (block.is_writable ? ", writable" : ", readonly")
                  << ")" << std::endl;

        return DECOMP_OK;
    } catch (const std::exception& e) {
        ctx->last_error = std::string("Failed to add memory block: ") + e.what();
        return DECOMP_ERR_LOAD;
    } catch (...) {
        ctx->last_error = "Unknown error in add_memory_block";
        return DECOMP_ERR_LOAD;
    }
}

} // namespace core
} // namespace fission
