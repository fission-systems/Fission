#include "fission/core/ContextServices.h"

#include "fission/loader/SectionAwareLoadImage.h"
#include "fission/core/SymbolProvider.h"
#include "fission/analysis/VTableAnalyzer.h"
#include "fission/types/RttiAnalyzer.h"

#include "libdecomp.hh"

#include <iostream>
#include "fission/utils/logger.h"
#include <sstream>
#include <set>

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
    if (ctx->memory_image) {
        ctx->memory_image->addLoaderSymbol(addr, name);
    }
}

void clear_symbols(DecompContext* ctx) {
    if (!ctx) {
        return;
    }

    std::lock_guard<std::mutex> lock(ctx->mutex);
    ctx->symbols.clear();
    if (ctx->memory_image) {
        ctx->memory_image->clearLoaderSymbols();
    }
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
        if (ctx->memory_image) {
            ctx->memory_image->addLoaderSymbol(addr, func_name);
        }

        if (ctx->arch && ctx->memory_image) {
            ghidra::Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
            if (global_scope) {
                ghidra::Address func_addr(ctx->arch->getDefaultCodeSpace(), addr);
                const ghidra::Funcdata* existing = global_scope->findFunction(func_addr);
                if (!existing) {
                    global_scope->addFunction(func_addr, func_name);
                    fission::utils::log_stream() << "[SymbolManager] Declared function at 0x" << std::hex << addr
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
    bool is_64bit,
    const char* sleigh_id,
    const char* compiler_id
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
        if (sleigh_id) ctx->sleigh_id = sleigh_id;
        if (compiler_id) ctx->compiler_id = compiler_id;

        // Re-seed loader symbol records from currently known function symbols.
        for (const auto& kv : ctx->symbols) {
            ctx->memory_image->addLoaderSymbol(kv.first, kv.second);
        }

        // Build reusable virtual-call display maps for post-processing.
        ctx->vtable_virtual_names.clear();
        ctx->vcall_slot_name_hints.clear();
        ctx->vcall_slot_target_hints.clear();

        fission::analysis::VTableAnalyzer vtable_analyzer;
        vtable_analyzer.scan_vtables(ctx->binary_data.data(), ctx->binary_data.size(), base_addr, is_64bit);

        std::map<uint64_t, std::string> recovered_classes =
            fission::types::RttiAnalyzer::recover_class_names(ctx->binary_data, base_addr, is_64bit);
        if (!recovered_classes.empty()) {
            vtable_analyzer.link_with_rtti(recovered_classes);
        }

        const int ptr_size = is_64bit ? 8 : 4;
        std::set<int> ambiguous_slot_targets;
        const auto is_unresolved_vname = [](const std::string& name) {
            return name.empty() ||
                   name.find("::vfunc_") != std::string::npos ||
                   name.rfind("sub_", 0) == 0;
        };
        for (const auto& vt : vtable_analyzer.get_vtables()) {
            for (size_t i = 0; i < vt.entries.size(); ++i) {
                int slot_offset = static_cast<int>(i) * ptr_size;
                std::string display_name =
                    vtable_analyzer.get_virtual_call_name(vt.address, slot_offset, ptr_size);
                uint64_t resolved_target =
                    vtable_analyzer.resolve_virtual_call(vt.address, slot_offset, ptr_size);

                if (resolved_target != 0) {
                    if (!ambiguous_slot_targets.count(slot_offset)) {
                        auto it_slot_target = ctx->vcall_slot_target_hints.find(slot_offset);
                        if (it_slot_target == ctx->vcall_slot_target_hints.end()) {
                            ctx->vcall_slot_target_hints[slot_offset] = resolved_target;
                        } else if (it_slot_target->second != resolved_target) {
                            ctx->vcall_slot_target_hints.erase(it_slot_target);
                            ambiguous_slot_targets.insert(slot_offset);
                        }
                    }
                }

                // Fallback to resolved call target if class/slot name is generic.
                if (is_unresolved_vname(display_name)) {
                    if (resolved_target != 0) {
                        auto it_sym = ctx->symbols.find(resolved_target);
                        if (it_sym != ctx->symbols.end()) {
                            display_name = it_sym->second;
                        } else {
                            auto it_g = ctx->global_symbols.find(resolved_target);
                            if (it_g != ctx->global_symbols.end()) {
                                display_name = it_g->second;
                            } else {
                                std::ostringstream ss;
                                ss << "sub_" << std::hex << resolved_target;
                                display_name = ss.str();
                            }
                        }
                    }
                }

                if (display_name.empty()) {
                    continue;
                }

                ctx->vtable_virtual_names[vt.address][slot_offset] = display_name;

                // Prefer RTTI-linked class names over generic ::vfunc_N placeholders.
                if (!is_unresolved_vname(display_name)) {
                    if (!ctx->vcall_slot_name_hints.count(slot_offset)) {
                        ctx->vcall_slot_name_hints[slot_offset] = display_name;
                    }
                }
            }
        }

        fission::utils::log_stream() << "[ContextServices] Virtual-call naming map: "
                  << ctx->vtable_virtual_names.size() << " vtables, "
                  << ctx->vcall_slot_name_hints.size() << " slot hints, "
                  << ctx->vcall_slot_target_hints.size() << " slot target hints" << std::endl;

        // The old Architecture (if any) is now stale — it references the
        // previous memory image.  Attempt a proper destruction first; only
        // fall back to a controlled leak if the destructor throws (a known
        // issue in Ghidra's SleighArchitecture teardown path).
        if (ctx->arch) {
            try {
                ctx->arch.reset();
            } catch (...) {
                fission::utils::log_stream()
                    << "[ContextServices] WARNING: Architecture destructor threw — "
                       "leaking pointer to avoid crash" << std::endl;
                ctx->arch.release(); // controlled leak
            }
        }
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

        fission::utils::log_stream() << "[MemoryManager] Registered memory block: " << name
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
