#include "fission/core/ArchInit.h"

#include "fission/ffi/DecompContext.h"
#include "fission/core/ArchPolicy.h"
#include "fission/core/DataSymbolRegistry.h"
#include "fission/core/SymbolProvider.h"
#include "fission/types/TypeManager.h"
#include "fission/types/GdtBinaryParser.h"
#include "fission/utils/file_utils.h"

#include "libdecomp.hh"
#include "address.hh"
#include "funcdata.hh"
#include "flow.hh"
#include "varnode.hh"
#include "architecture.hh"
#include "options.hh"

#include <algorithm>
#include <iostream>
#include "fission/utils/logger.h"
#include "fission/config/PathConfig.h"

using namespace fission::config;

namespace fission {
namespace core {

using fission::ffi::DecompContext;
using fission::types::GdtBinaryParser;
using fission::types::TypeManager;
using fission::utils::file_exists;

static std::string select_sleigh_id(const DecompContext* ctx) {
    if (!ctx->sleigh_id.empty()) {
        if (!ctx->compiler_id.empty() && ctx->sleigh_id.find(':') != std::string::npos) {
            // Check if it already has 4 segments (e.g. x86:LE:64:default)
            // Ghidra expects 5 segments for full spec: x86:LE:64:default:windows
            size_t colon_count = 0;
            for (char c : ctx->sleigh_id) if (c == ':') colon_count++;
            if (colon_count == 3) {
                return ctx->sleigh_id + ":" + ctx->compiler_id;
            }
        }
        return ctx->sleigh_id;
    }
    return ctx->is_64bit ? "x86:LE:64:default" : "x86:LE:32:default";
}

static bool try_load_gdt(ghidra::Architecture* arch, const std::string& path) {
    if (path.empty() || !file_exists(path)) {
        return false;
    }

    fission::utils::log_stream() << "[DecompilerCore] Loading GDT from: " << path << std::endl;
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

    std::vector<std::string> candidates = fission::config::get_gdt_candidates(is_64bit);

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

    if (ctx->record_jumploads) {
        ctx->arch->flowoptions |= ghidra::FlowInfo::record_jumploads;
    } else {
        ctx->arch->flowoptions &= ~ghidra::FlowInfo::record_jumploads;
    }

    if (ctx->disable_toomanyinstructions_error) {
        ctx->arch->flowoptions &= ~ghidra::FlowInfo::error_toomanyinstructions;
    } else {
        ctx->arch->flowoptions |= ghidra::FlowInfo::error_toomanyinstructions;
    }

    // Keep OptionDatabase in sync with flags (mirrors original options.cc toggles)
    if (ctx->arch->options != nullptr) {
        try {
            ctx->arch->options->set(ghidra::ELEM_INFERCONSTPTR.getId(), ctx->infer_pointers ? "on" : "off", "", "");
            ctx->arch->options->set(ghidra::ELEM_ANALYZEFORLOOPS.getId(), ctx->analyze_loops ? "on" : "off", "", "");
            ctx->arch->options->set(ghidra::ELEM_READONLY.getId(), ctx->readonly_propagate ? "on" : "off", "", "");
            ctx->arch->options->set(ghidra::ELEM_JUMPLOAD.getId(), ctx->record_jumploads ? "on" : "off", "", "");
            ctx->arch->options->set(ghidra::ELEM_ERRORTOOMANYINSTRUCTIONS.getId(), ctx->disable_toomanyinstructions_error ? "off" : "on", "", "");
            ctx->arch->options->set(ghidra::ELEM_INLINE.getId(), ctx->allow_inline ? "on" : "off", "", "");
            // Phase 1: output quality options
            ctx->arch->options->set(ghidra::ELEM_NULLPRINTING.getId(),       ctx->null_printing       ? "on" : "off", "", "");
            ctx->arch->options->set(ghidra::ELEM_INPLACEOPS.getId(),         ctx->inplace_ops         ? "on" : "off", "", "");
            ctx->arch->options->set(ghidra::ELEM_NOCASTPRINTING.getId(),     ctx->no_cast_printing    ? "on" : "off", "", "");
            ctx->arch->options->set(ghidra::ELEM_CONVENTIONPRINTING.getId(), ctx->convention_printing ? "on" : "off", "", "");
        } catch (const std::exception& e) {
            fission::utils::log_stream() << "[DecompilerCore] apply_feature_flags: option sync failed: "
                      << e.what() << std::endl;
        } catch (...) {
            fission::utils::log_stream() << "[DecompilerCore] apply_feature_flags: option sync failed (unknown)" << std::endl;
        }
    }
}

static void register_functions_from_symbols(DecompContext* ctx) {
    if (ctx->symbols.empty()) {
        return;
    }

    fission::utils::log_stream() << "[DecompilerCore] Injecting " << ctx->symbols.size() << " symbols" << std::endl;
    ctx->arch->injectIatSymbols(ctx->symbols);

    ghidra::Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
    if (!global_scope) {
        return;
    }

    fission::utils::log_stream() << "[DecompilerCore] Using code space for registration: "
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
                    fission::utils::log_stream() << "[DecompilerCore] Failed to add function at 0x" << std::hex << addr << std::dec
                              << ": " << name << std::endl;
                }
            } else {
                existing_count++;
            }
        } catch (const std::exception& e) {
            failed_count++;
            fission::utils::log_stream() << "[DecompilerCore] Exception adding function at 0x" << std::hex << addr << std::dec
                      << ": " << e.what() << std::endl;
        } catch (...) {
            failed_count++;
        }
    }

    fission::utils::log_stream() << "[DecompilerCore] Function registration: " << func_count << " added, "
              << existing_count << " already exist, " << failed_count << " failed" << std::endl;
    fission::utils::log_stream() << "[DecompilerCore] Global scope: "
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

    fission::utils::log_stream() << "[DecompilerCore] Registering " << ctx->memory_blocks.size() << " memory blocks" << std::endl;
    for (const auto& block : ctx->memory_blocks) {
        fission::utils::log_stream() << "  - " << block.name
                  << ": VA 0x" << std::hex << block.va_addr << "-0x" << (block.va_addr + block.va_size)
                  << std::dec << " (vsize: " << block.va_size << " bytes, "
                  << "file_off: 0x" << std::hex << block.file_offset << std::dec << ", "
                  << (block.is_executable ? "CODE" : "DATA") << ")" << std::endl;
    }
}

static std::mutex arch_init_mutex;

void initialize_architecture(DecompContext* ctx) {
    ArchInitOptions options;
    initialize_architecture(ctx, options);
}

void initialize_architecture(DecompContext* ctx, const ArchInitOptions& options) {
    if (!ctx || ctx->arch) {
        return;
    }

    std::lock_guard<std::mutex> lock(arch_init_mutex);
    
    std::string sleigh_id = select_sleigh_id(ctx);

    ctx->arch = std::make_unique<fission::core::CliArchitecture>(
        sleigh_id,
        ctx->memory_image.get(),
        &ctx->err_stream
    );

    try {
        ensure_symbol_provider(ctx);
        ctx->arch->setSymbolProvider(ctx->symbol_provider.get());

        ghidra::DocumentStorage store;
        ctx->arch->init(store);

        bool readonly_props_set = apply_default_space(ctx);

        configure_arch(ctx->arch.get());

        if (options.read_loader_symbols) {
            try {
                ctx->arch->readLoaderSymbols("::");
            } catch (const std::exception& e) {
                fission::utils::log_stream() << "[DecompilerCore] WARNING: readLoaderSymbols failed: "
                          << e.what() << std::endl;
            } catch (...) {
                fission::utils::log_stream() << "[DecompilerCore] WARNING: readLoaderSymbols failed (unknown)"
                          << std::endl;
            }
        }

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
            fission::utils::log_stream() << "[DecompilerCore] Injecting " << ctx->symbols.size() << " symbols" << std::endl;
            ctx->arch->injectIatSymbols(ctx->symbols);
        }

        if (options.apply_memory_blocks) {
            if (!readonly_props_set) {
                apply_memory_block_readonly(ctx);
            }
            log_memory_blocks(ctx);
        }

        // FISSION IMPROVEMENT: Register data section symbols
        if (options.register_data_symbols && !ctx->binary_data.empty()) {
            registerDataSectionSymbols(ctx);
        }

        fission::utils::log_stream() << "[DecompilerCore] Architecture initialized: " << sleigh_id << std::endl;

    } catch (const ghidra::LowlevelError& e) {
        fission::utils::log_stream() << "[DecompilerCore] ERROR: Ghidra LowlevelError during architecture initialization: " << e.explain << std::endl;
        ctx->arch.release(); // WORKAROUND: Leak instead of crash on failed init
        throw;
    } catch (const std::exception& e) {
        fission::utils::log_stream() << "[DecompilerCore] ERROR: Architecture initialization failed: " << e.what() << std::endl;
        ctx->arch.release(); // WORKAROUND: Leak instead of crash on failed init
        throw;
    } catch (...) {
        fission::utils::log_stream() << "[DecompilerCore] ERROR: Unknown error during architecture initialization" << std::endl;
        ctx->arch.release(); // WORKAROUND: Leak instead of crash on failed init
        throw;
    }
}

} // namespace core
} // namespace fission
