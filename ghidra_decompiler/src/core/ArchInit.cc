#include "fission/core/ArchInit.h"

#include "fission/ffi/DecompContext.h"
#include "fission/core/ArchPolicy.h"
#include "fission/core/SymbolProvider.h"
#include "fission/types/TypeManager.h"
#include "fission/types/GdtBinaryParser.h"
#include "fission/utils/file_utils.h"

#include "libdecomp.hh"
#include "address.hh"
#include "funcdata.hh"
#include "varnode.hh"

#include <algorithm>
#include <iostream>

// Forward declaration for data symbol registration
namespace fission {
namespace core {
    void registerDataSectionSymbols(fission::ffi::DecompContext* ctx);
}
}

namespace fission {
namespace core {

using fission::ffi::DecompContext;
using fission::types::GdtBinaryParser;
using fission::types::TypeManager;
using fission::utils::file_exists;

static std::string select_sleigh_id(const DecompContext* ctx) {
    return ctx->is_64bit ? "x86:LE:64:default" : "x86:LE:32:default";
}

static bool try_load_gdt(ghidra::Architecture* arch, const std::string& path) {
    if (path.empty() || !file_exists(path)) {
        return false;
    }

    std::cerr << "[DecompilerCore] Loading GDT from: " << path << std::endl;
    GdtBinaryParser gdt;
    if (gdt.load(path)) {
        TypeManager::load_types_from_gdt(arch->types, &gdt, ArchPolicy::getPointerSize(arch));
        return true;
    }

    return false;
}

static void load_gdt_for_arch(ghidra::Architecture* arch, bool is_64bit, const std::string& override_path) {
    if (try_load_gdt(arch, override_path)) {
        return;
    }

    std::string suffix = is_64bit ? "_64.gdt" : "_32.gdt";
    std::vector<std::string> candidates = {
        "../../utils/ghidra/typeinfo/win32/windows_vs12" + suffix,
        "../utils/ghidra/typeinfo/win32/windows_vs12" + suffix,
        "./utils/ghidra/typeinfo/win32/windows_vs12" + suffix,
        "utils/ghidra/typeinfo/win32/windows_vs12" + suffix
    };

    const bool gdt_loaded = std::any_of(candidates.begin(), candidates.end(), [&](const auto& path) {
        return try_load_gdt(arch, path);
    });
    (void)gdt_loaded;
}

static void ensure_symbol_provider(DecompContext* ctx) {
    if (ctx->symbol_provider) {
        return;
    }

    if (ctx->symbol_provider_enabled) {
        ctx->symbol_provider = std::make_unique<fission::core::CallbackSymbolProvider>(
            &ctx->symbol_provider_callbacks
        );
    } else {
        ctx->symbol_provider = std::make_unique<fission::core::MapSymbolProvider>(
            &ctx->symbols,
            &ctx->global_symbols
        );
    }
}

static bool apply_default_space(DecompContext* ctx) {
    if (!ctx->memory_image) {
        return false;
    }

    ghidra::AddrSpace* data_space = ctx->arch->getDefaultDataSpace();
    if (!data_space) {
        return false;
    }

    ctx->memory_image->setDefaultSpace(data_space);
    ctx->arch->refreshReadOnly();
    return true;
}

static void apply_feature_flags(DecompContext* ctx) {
    if (!ctx->arch) {
        return;
    }

    ctx->arch->infer_pointers = ctx->infer_pointers;
    ctx->arch->analyze_for_loops = ctx->analyze_loops;
    ctx->arch->readonlypropagate = ctx->readonly_propagate;
}

static void register_functions_from_symbols(DecompContext* ctx) {
    if (ctx->symbols.empty()) {
        return;
    }

    std::cerr << "[DecompilerCore] Injecting " << ctx->symbols.size() << " symbols" << std::endl;
    ctx->arch->injectIatSymbols(ctx->symbols);

    ghidra::Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
    if (!global_scope) {
        return;
    }

    std::cerr << "[DecompilerCore] Using code space for registration: "
              << ctx->arch->getDefaultCodeSpace()->getName() << std::endl;

    int func_count = 0;
    int existing_count = 0;
    int failed_count = 0;
    for (const auto& [addr, name] : ctx->symbols) {
        try {
            ghidra::Address func_addr(ctx->arch->getDefaultCodeSpace(), addr);
            const ghidra::Funcdata* existing = global_scope->findFunction(func_addr);
            if (!existing) {
                const ghidra::FunctionSymbol* sym = global_scope->addFunction(func_addr, name);
                if (sym) {
                    func_count++;
                } else {
                    failed_count++;
                    std::cerr << "[DecompilerCore] Failed to add function at 0x" << std::hex << addr << std::dec
                              << ": " << name << std::endl;
                }
            } else {
                existing_count++;
            }
        } catch (const std::exception& e) {
            failed_count++;
            std::cerr << "[DecompilerCore] Exception adding function at 0x" << std::hex << addr << std::dec
                      << ": " << e.what() << std::endl;
        } catch (...) {
            failed_count++;
        }
    }

    std::cerr << "[DecompilerCore] Function registration: " << func_count << " added, "
              << existing_count << " already exist, " << failed_count << " failed" << std::endl;
    std::cerr << "[DecompilerCore] Global scope: "
              << static_cast<const void*>(global_scope) << std::endl;
}

static void apply_memory_block_readonly(DecompContext* ctx) {
    if (ctx->memory_blocks.empty() || !ctx->arch->symboltab) {
        return;
    }

    ghidra::AddrSpace* data_space = ctx->arch->getDefaultDataSpace();
    if (!data_space) {
        return;
    }

    for (const auto& block : ctx->memory_blocks) {
        uint64_t size = block.va_size > 0 ? block.va_size : block.file_size;
        if (size == 0) {
            continue;
        }

        ghidra::uintb start = block.va_addr;
        ghidra::uintb last = start + static_cast<ghidra::uintb>(size - 1);
        if (last < start) {
            last = start;
        }

        ghidra::uint4 flags = 0;
        if (!block.is_writable) {
            flags |= ghidra::Varnode::readonly;
        }

        if (flags != 0) {
            ctx->arch->symboltab->setPropertyRange(
                flags,
                ghidra::Range(data_space, start, last)
            );
        }
    }
}

static void log_memory_blocks(const DecompContext* ctx) {
    if (ctx->memory_blocks.empty()) {
        return;
    }

    std::cerr << "[DecompilerCore] Registering " << ctx->memory_blocks.size() << " memory blocks" << std::endl;
    for (const auto& block : ctx->memory_blocks) {
        std::cerr << "  - " << block.name
                  << ": VA 0x" << std::hex << block.va_addr << "-0x" << (block.va_addr + block.va_size)
                  << std::dec << " (vsize: " << block.va_size << " bytes, "
                  << "file_off: 0x" << std::hex << block.file_offset << std::dec << ", "
                  << (block.is_executable ? "CODE" : "DATA") << ")" << std::endl;
    }
}

void initialize_architecture(DecompContext* ctx) {
    ArchInitOptions options;
    initialize_architecture(ctx, options);
}

void initialize_architecture(DecompContext* ctx, const ArchInitOptions& options) {
    if (!ctx || ctx->arch) {
        return;
    }

    std::string sleigh_id = select_sleigh_id(ctx);

    ctx->arch = std::make_unique<fission::core::CliArchitecture>(
        sleigh_id,
        ctx->memory_image.get(),
        &ctx->err_stream
    );

    ensure_symbol_provider(ctx);
    ctx->arch->setSymbolProvider(ctx->symbol_provider.get());

    ghidra::DocumentStorage store;
    ctx->arch->init(store);

    bool readonly_props_set = apply_default_space(ctx);

    configure_arch(ctx->arch.get());

    if (options.apply_feature_flags) {
        apply_feature_flags(ctx);
    }

    if (options.register_windows_types) {
        TypeManager::register_windows_types(ctx->arch->types, ArchPolicy::getPointerSize(ctx->arch.get()));
    }

    if (options.load_gdt) {
        load_gdt_for_arch(ctx->arch.get(), ctx->is_64bit, ctx->gdt_path);
    }

    if (options.inject_symbols && options.register_functions) {
        register_functions_from_symbols(ctx);
    } else if (options.inject_symbols) {
        std::cerr << "[DecompilerCore] Injecting " << ctx->symbols.size() << " symbols" << std::endl;
        ctx->arch->injectIatSymbols(ctx->symbols);
    }

    if (options.apply_memory_blocks) {
        if (!readonly_props_set) {
            apply_memory_block_readonly(ctx);
        }
        log_memory_blocks(ctx);
    }

    // FISSION IMPROVEMENT: Register data section symbols (floating-point constants, etc.)
    // This enables proper type propagation through memory loads
    if (options.register_data_symbols && !ctx->binary_data.empty()) {
        registerDataSectionSymbols(ctx);
    }

    std::cerr << "[DecompilerCore] Architecture initialized: " << sleigh_id << std::endl;
}

} // namespace core
} // namespace fission
