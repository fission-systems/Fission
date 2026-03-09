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
#include <mutex>
#include <unordered_map>
#include <memory>
#include "fission/utils/logger.h"
#include "fission/config/PathConfig.h"
#include "fission/core/CliArchitecture.h"

using namespace fission::config;

namespace fission {
namespace core {

using fission::ffi::DecompContext;
using fission::types::GdtBinaryParser;
using fission::types::TypeManager;
using fission::utils::file_exists;

// Global GDT cache to avoid redundant parsing across workers.
static std::mutex g_gdt_cache_mutex;
static std::unordered_map<std::string, std::shared_ptr<GdtBinaryParser>> g_gdt_cache;

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

    std::shared_ptr<GdtBinaryParser> gdt_ptr;
    {
        std::lock_guard<std::mutex> lock(g_gdt_cache_mutex);
        auto it = g_gdt_cache.find(path);
        if (it != g_gdt_cache.end()) {
            gdt_ptr = it->second;
        } else {
            fission::utils::log_stream() << "[DecompilerCore] First-time loading GDT: " << path << std::endl;
            gdt_ptr = std::make_shared<GdtBinaryParser>();
            if (gdt_ptr->load(path)) {
                g_gdt_cache[path] = gdt_ptr;
            } else {
                return false;
            }
        }
    }

    if (gdt_ptr && gdt_ptr->is_loaded()) {
        fission::utils::log_stream() << "[DecompilerCore] Applying cached GDT: " << path << std::endl;
        TypeManager::load_types_from_gdt(arch->types, gdt_ptr.get(), ArchPolicy::getPointerSize(arch));
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
        ctx->symbol_provider = std::make_unique<fission::core::CachedCallbackSymbolProvider>(
            &ctx->symbol_provider_callbacks
        );
    } else {
        ctx->symbol_provider = std::make_unique<fission::core::MapSymbolProvider>(
            &ctx->symbols,
            &ctx->global_symbols
        );
    }
}

static bool apply_default_space(DecompContext* ctx, ghidra::Architecture* arch) {
    if (!ctx->memory_image) {
        return false;
    }

    ghidra::AddrSpace* data_space = arch->getDefaultDataSpace();
    if (!data_space) {
        return false;
    }

    ctx->memory_image->setDefaultSpace(data_space);
    if (auto* cli_arch = dynamic_cast<CliArchitecture*>(arch)) {
        cli_arch->refreshReadOnly();
    }
    return true;
}

static void apply_feature_flags(DecompContext* ctx, ghidra::Architecture* arch) {
    arch->infer_pointers = ctx->infer_pointers;
    arch->analyze_for_loops = ctx->analyze_loops;
    arch->readonlypropagate = ctx->readonly_propagate;
    arch->analysis_timeout_sec = static_cast<double>(ctx->timeout_ms) / 1000.0;

    if (ctx->record_jumploads) {
        arch->flowoptions |= ghidra::FlowInfo::record_jumploads;
    } else {
        arch->flowoptions &= ~ghidra::FlowInfo::record_jumploads;
    }

    if (ctx->disable_toomanyinstructions_error) {
        arch->flowoptions &= ~ghidra::FlowInfo::error_toomanyinstructions;
    } else {
        arch->flowoptions |= ghidra::FlowInfo::error_toomanyinstructions;
    }

    if (arch->options != nullptr) {
        try {
            arch->options->set(ghidra::ELEM_INFERCONSTPTR.getId(), ctx->infer_pointers ? "on" : "off", "", "");
            arch->options->set(ghidra::ELEM_ANALYZEFORLOOPS.getId(), ctx->analyze_loops ? "on" : "off", "", "");
            arch->options->set(ghidra::ELEM_READONLY.getId(), ctx->readonly_propagate ? "on" : "off", "", "");
            arch->options->set(ghidra::ELEM_JUMPLOAD.getId(), ctx->record_jumploads ? "on" : "off", "", "");
            arch->options->set(ghidra::ELEM_ERRORTOOMANYINSTRUCTIONS.getId(), ctx->disable_toomanyinstructions_error ? "off" : "on", "", "");
            arch->options->set(ghidra::ELEM_INLINE.getId(), ctx->allow_inline ? "on" : "off", "", "");
            arch->options->set(ghidra::ELEM_NULLPRINTING.getId(),       ctx->null_printing       ? "on" : "off", "", "");
            arch->options->set(ghidra::ELEM_INPLACEOPS.getId(),         ctx->inplace_ops         ? "on" : "off", "", "");
            arch->options->set(ghidra::ELEM_NOCASTPRINTING.getId(),     ctx->no_cast_printing    ? "on" : "off", "", "");
            arch->options->set(ghidra::ELEM_CONVENTIONPRINTING.getId(), ctx->convention_printing ? "on" : "off", "", "");
        } catch (...) {}
    }
}

static void register_functions_from_symbols(DecompContext* ctx, ghidra::Architecture* arch) {
    if (ctx->symbols.empty() || !arch) return;

    if (auto* cli_arch = dynamic_cast<CliArchitecture*>(arch)) {
        cli_arch->injectIatSymbols(ctx->symbols);
    }

    ghidra::Scope* global_scope = arch->symboltab->getGlobalScope();
    if (!global_scope) return;

    for (const auto& [addr, name] : ctx->symbols) {
        try {
            ghidra::Address func_addr(arch->getDefaultCodeSpace(), addr);
            if (!global_scope->findFunction(func_addr)) {
                global_scope->addFunction(func_addr, name);
            }
        } catch (...) {}
    }
}

static void apply_memory_block_readonly(DecompContext* ctx, ghidra::Architecture* arch) {
    if (ctx->memory_blocks.empty() || !arch || !arch->symboltab) return;

    ghidra::AddrSpace* data_space = arch->getDefaultDataSpace();
    if (!data_space) return;

    for (const auto& block : ctx->memory_blocks) {
        uint64_t size = block.va_size > 0 ? block.va_size : block.file_size;
        if (size == 0) continue;

        ghidra::uintb start = block.va_addr;
        ghidra::uintb last = start + static_cast<ghidra::uintb>(size - 1);
        if (last < start) last = start;

        if (!block.is_writable) {
            arch->symboltab->setPropertyRange(ghidra::Varnode::readonly, ghidra::Range(data_space, start, last));
        }
    }
}

static void log_memory_blocks(const DecompContext* ctx) {
    fission::utils::log_stream() << "[DecompilerCore] Registering " << ctx->memory_blocks.size() << " memory blocks" << std::endl;
}

static std::mutex arch_init_mutex;

void initialize_architecture(DecompContext* ctx) {
    ArchInitOptions options;
    initialize_architecture(ctx, options);
}

void initialize_architecture(DecompContext* ctx, const ArchInitOptions& options) {
    if (!ctx || ctx->arch) return;

    std::string sleigh_id = select_sleigh_id(ctx);
    std::unique_ptr<CliArchitecture> new_arch;

    {
        std::lock_guard<std::mutex> lock(arch_init_mutex);
        new_arch = std::make_unique<CliArchitecture>(sleigh_id, ctx->memory_image.get(), &ctx->err_stream);
        ensure_symbol_provider(ctx);
        new_arch->setSymbolProvider(ctx->symbol_provider.get());
        ghidra::DocumentStorage store;
        new_arch->init(store);
    }

    try {
        bool readonly_props_set = apply_default_space(ctx, new_arch.get());
        configure_arch(new_arch.get());

        if (options.read_loader_symbols) {
            try { new_arch->readLoaderSymbols("::"); } catch (...) {}
        }

        if (options.apply_feature_flags) {
            apply_feature_flags(ctx, new_arch.get());
        }

        if (options.register_windows_types) {
            TypeManager::register_windows_types(new_arch->types, ArchPolicy::getPointerSize(new_arch.get()));
        }

        if (options.load_gdt) {
            load_gdt_for_arch(new_arch.get(), ctx->is_64bit, ctx->gdt_path);
        }

        if (options.inject_symbols) {
            if (options.register_functions) register_functions_from_symbols(ctx, new_arch.get());
            else new_arch->injectIatSymbols(ctx->symbols);
        }

        if (options.apply_memory_blocks) {
            if (!readonly_props_set) apply_memory_block_readonly(ctx, new_arch.get());
            log_memory_blocks(ctx);
        }

        if (options.register_data_symbols && !ctx->binary_data.empty()) {
            registerDataSectionSymbols(ctx, new_arch.get());
        }

        ctx->arch = std::move(new_arch);
        fission::utils::log_stream() << "[DecompilerCore] Architecture initialized: " << sleigh_id << std::endl;

    } catch (...) {
        fission::utils::log_stream() << "[DecompilerCore] ERROR: Architecture initialization failed" << std::endl;
        throw;
    }
}

} // namespace core
} // namespace fission
