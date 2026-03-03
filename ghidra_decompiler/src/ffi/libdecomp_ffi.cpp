/**
 * Fission Decompiler FFI Implementation
 * 
 * C++ implementation of the FFI interface defined in libdecomp_ffi.h.
 * Refactored to use unified DecompilerFFI component.
 */

#include "fission/ffi/libdecomp_ffi.h"
#include "fission/ffi/DecompContext.h"
#include "fission/ffi/DecompilerFFI.h"
#include "fission/ffi/SymbolProviderManager.h"
#include "fission/ffi/DecompilerCore.h"
#include "fission/decompiler/PcodeOptimizationBridge.h"

// Ghidra types for type registration
#include "type.hh"
#include "address.hh"
#include "funcdata.hh"
#include "fspec.hh"
#include "architecture.hh"

#include <cstring>
#include <iostream>
#include <sstream>

using namespace fission::ffi;

static ghidra::Funcdata* get_or_create_function_for_option(DecompContext* ctx, uint64_t addr) {
    if (ctx == nullptr) return nullptr;

    ensure_architecture(ctx);
    if (!ctx->arch || !ctx->arch->symboltab) {
        return nullptr;
    }

    ghidra::Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
    ghidra::AddrSpace* code_space = ctx->arch->getDefaultCodeSpace();
    if (global_scope == nullptr || code_space == nullptr) {
        return nullptr;
    }

    ghidra::Address start_addr(code_space, addr);
    ghidra::Funcdata* fd = global_scope->findFunction(start_addr);
    if (fd != nullptr) {
        return fd;
    }

    std::ostringstream name_ss;
    name_ss << "sub_" << std::hex << addr;
    ghidra::FunctionSymbol* sym = global_scope->addFunction(start_addr, name_ss.str());
    return (sym != nullptr) ? sym->getFunction() : nullptr;
}

static ghidra::ProtoModel* resolve_proto_model(DecompContext* ctx, const char* model_name) {
    if (ctx == nullptr || model_name == nullptr || model_name[0] == '\0') {
        return nullptr;
    }
    ensure_architecture(ctx);
    if (!ctx->arch) {
        return nullptr;
    }

    std::string name(model_name);
    if (name == "default") {
        return ctx->arch->defaultfp;
    }
    return ctx->arch->getModel(name);
}

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
    int is_64bit,
    const char* sleigh_id,
    const char* compiler_id
) {
    return load_binary(ctx, data, len, base_addr, is_64bit != 0, sleigh_id, compiler_id);
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

extern "C" DECOMP_API DecompError decomp_set_function_inline(
    DecompContext* ctx,
    uint64_t addr,
    int enabled
) {
    if (ctx == nullptr) return DECOMP_ERR_INVALID_CONTEXT;

    std::lock_guard<std::mutex> lock(ctx->mutex);
    try {
        ghidra::Funcdata* fd = get_or_create_function_for_option(ctx, addr);
        if (fd == nullptr) {
            ctx->last_error = "Failed to resolve function for inline option";
            return DECOMP_ERR_INIT;
        }

        fd->getFuncProto().setInline(enabled != 0);
        if (ctx->arch) {
            ctx->arch->clearAnalysis(fd);
        }
        return DECOMP_OK;
    } catch (const std::exception& e) {
        ctx->last_error = std::string("Error setting inline option: ") + e.what();
        return DECOMP_ERR_INIT;
    }
}

extern "C" DECOMP_API DecompError decomp_set_function_noreturn(
    DecompContext* ctx,
    uint64_t addr,
    int enabled
) {
    if (ctx == nullptr) return DECOMP_ERR_INVALID_CONTEXT;

    std::lock_guard<std::mutex> lock(ctx->mutex);
    try {
        ghidra::Funcdata* fd = get_or_create_function_for_option(ctx, addr);
        if (fd == nullptr) {
            ctx->last_error = "Failed to resolve function for noreturn option";
            return DECOMP_ERR_INIT;
        }

        fd->getFuncProto().setNoReturn(enabled != 0);
        if (ctx->arch) {
            ctx->arch->clearAnalysis(fd);
        }
        return DECOMP_OK;
    } catch (const std::exception& e) {
        ctx->last_error = std::string("Error setting noreturn option: ") + e.what();
        return DECOMP_ERR_INIT;
    }
}

extern "C" DECOMP_API DecompError decomp_set_function_extrapop(
    DecompContext* ctx,
    uint64_t addr,
    int32_t extrapop
) {
    if (ctx == nullptr) return DECOMP_ERR_INVALID_CONTEXT;

    std::lock_guard<std::mutex> lock(ctx->mutex);
    try {
        ghidra::Funcdata* fd = get_or_create_function_for_option(ctx, addr);
        if (fd == nullptr) {
            ctx->last_error = "Failed to resolve function for extrapop option";
            return DECOMP_ERR_INIT;
        }

        fd->getFuncProto().setExtraPop(static_cast<int>(extrapop));
        if (ctx->arch) {
            ctx->arch->clearAnalysis(fd);
        }
        return DECOMP_OK;
    } catch (const std::exception& e) {
        ctx->last_error = std::string("Error setting extrapop option: ") + e.what();
        return DECOMP_ERR_INIT;
    }
}

extern "C" DECOMP_API DecompError decomp_set_default_prototype(
    DecompContext* ctx,
    const char* model_name
) {
    if (ctx == nullptr) return DECOMP_ERR_INVALID_CONTEXT;

    std::lock_guard<std::mutex> lock(ctx->mutex);
    try {
        ghidra::ProtoModel* model = resolve_proto_model(ctx, model_name);
        if (model == nullptr) {
            ctx->last_error = std::string("Unknown prototype model: ") + (model_name ? model_name : "(null)");
            return DECOMP_ERR_INIT;
        }
        ctx->arch->setDefaultModel(model);
        return DECOMP_OK;
    } catch (const std::exception& e) {
        ctx->last_error = std::string("Error setting default prototype: ") + e.what();
        return DECOMP_ERR_INIT;
    }
}

extern "C" DECOMP_API DecompError decomp_set_protoeval_current(
    DecompContext* ctx,
    const char* model_name
) {
    if (ctx == nullptr) return DECOMP_ERR_INVALID_CONTEXT;

    std::lock_guard<std::mutex> lock(ctx->mutex);
    try {
        ghidra::ProtoModel* model = resolve_proto_model(ctx, model_name);
        if (model == nullptr) {
            ctx->last_error = std::string("Unknown prototype model: ") + (model_name ? model_name : "(null)");
            return DECOMP_ERR_INIT;
        }
        ctx->arch->evalfp_current = model;
        return DECOMP_OK;
    } catch (const std::exception& e) {
        ctx->last_error = std::string("Error setting protoeval current: ") + e.what();
        return DECOMP_ERR_INIT;
    }
}

extern "C" DECOMP_API DecompError decomp_set_protoeval_called(
    DecompContext* ctx,
    const char* model_name
) {
    if (ctx == nullptr) return DECOMP_ERR_INVALID_CONTEXT;

    std::lock_guard<std::mutex> lock(ctx->mutex);
    try {
        ghidra::ProtoModel* model = resolve_proto_model(ctx, model_name);
        if (model == nullptr) {
            ctx->last_error = std::string("Unknown prototype model: ") + (model_name ? model_name : "(null)");
            return DECOMP_ERR_INIT;
        }
        ctx->arch->evalfp_called = model;
        return DECOMP_OK;
    } catch (const std::exception& e) {
        ctx->last_error = std::string("Error setting protoeval called: ") + e.what();
        return DECOMP_ERR_INIT;
    }
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

// ============================================================================
// Type Registration
// ============================================================================

extern "C" DECOMP_API DecompError decomp_register_struct_type(
    DecompContext* ctx,
    const char* name,
    uint32_t size,
    const DecompFieldInfo* fields,
    size_t field_count
) {
    if (!ctx || !name) return DECOMP_ERR_INVALID_CONTEXT;
    if (!ctx->arch) return DECOMP_ERR_INIT;
    
    try {
        ghidra::TypeFactory* factory = ctx->arch->types;
        if (!factory) return DECOMP_ERR_INIT;
        
        // Check if type already exists
        ghidra::Datatype* existing = factory->findByName(name);
        if (existing != nullptr) {
            // Type already registered
            return DECOMP_OK;
        }
        
        // Create new struct type
        ghidra::TypeStruct* new_struct = factory->getTypeStruct(name);
        
        // Build field list
        std::vector<ghidra::TypeField> field_list;
        int ptr_size = factory->getSizeOfPointer();
        
        for (size_t i = 0; i < field_count; ++i) {
            const DecompFieldInfo& f = fields[i];
            if (!f.name) continue;
            
            // Determine field type based on size and type_name hint
            ghidra::Datatype* field_type = nullptr;
            int field_size = (f.size > 0) ? f.size : ptr_size;
            
            // Try to match common type names
            if (f.type_name) {
                std::string tn(f.type_name);
                if (tn == "int" || tn == "Si" || tn == "Int") {
                    field_type = factory->getBase(field_size, ghidra::TYPE_INT);
                } else if (tn == "uint" || tn == "Su" || tn == "UInt") {
                    field_type = factory->getBase(field_size, ghidra::TYPE_UINT);
                } else if (tn == "float" || tn == "Sf") {
                    field_type = factory->getBase(4, ghidra::TYPE_FLOAT);
                } else if (tn == "double" || tn == "Sd") {
                    field_type = factory->getBase(8, ghidra::TYPE_FLOAT);
                } else if (tn == "char*" || tn == "SS" || tn == "String") {
                    // String type - pointer to char
                    ghidra::Datatype* char_type = factory->getBase(1, ghidra::TYPE_INT);
                    field_type = factory->getTypePointer(ptr_size, char_type, 1);
                } else {
                    // Default to appropriate sized type
                    field_type = factory->getBase(field_size, ghidra::TYPE_UNKNOWN);
                }
            } else {
                field_type = factory->getBase(field_size, ghidra::TYPE_UNKNOWN);
            }
            
            field_list.push_back(ghidra::TypeField(i, f.offset, f.name, field_type));
        }
        
        // Apply fields to struct
        if (!field_list.empty()) {
            // Calculate appropriate size if not provided
            int struct_size = (size > 0) ? size : 0;
            if (struct_size == 0 && !field_list.empty()) {
                const auto& last = field_list.back();
                struct_size = last.offset + last.type->getSize();
                // Align to pointer size
                if (struct_size % ptr_size != 0) {
                    struct_size += (ptr_size - (struct_size % ptr_size));
                }
            }
            
            factory->setFields(field_list, new_struct, struct_size, ptr_size, 0);
        }
        
        // Store in context for later lookup
        ctx->registered_types[name] = new_struct;
        
        std::cout << "[TypeRegistry] Registered struct '" << name 
                  << "' with " << field_count << " fields, size=" << size << std::endl;
        
        return DECOMP_OK;
    } catch (const std::exception& e) {
        ctx->last_error = std::string("Type registration error: ") + e.what();
        return DECOMP_ERR_INIT;
    }
}

extern "C" DECOMP_API DecompError decomp_apply_struct_to_param(
    DecompContext* ctx,
    uint64_t func_addr,
    int param_index,
    const char* struct_name
) {
    if (!ctx || !struct_name) return DECOMP_ERR_INVALID_CONTEXT;
    if (!ctx->arch) return DECOMP_ERR_INIT;
    
    try {
        // Find the registered struct
        auto it = ctx->registered_types.find(struct_name);
        if (it == ctx->registered_types.end()) {
            // Try to find in TypeFactory
            ghidra::Datatype* dt = ctx->arch->types->findByName(struct_name);
            if (!dt || dt->getMetatype() != ghidra::TYPE_STRUCT) {
                ctx->last_error = "Struct type not found: " + std::string(struct_name);
                return DECOMP_ERR_INIT;
            }
            // Store for future use
            ctx->registered_types[struct_name] = static_cast<ghidra::TypeStruct*>(dt);
        }
        
        // Store the param -> type mapping for use during decompilation
        ctx->param_type_hints[func_addr][param_index] = struct_name;
        
        std::cout << "[TypeRegistry] Applied '" << struct_name 
                  << "' to param " << param_index 
                  << " of function @0x" << std::hex << func_addr << std::dec << std::endl;
        
        return DECOMP_OK;
    } catch (const std::exception& e) {
        ctx->last_error = std::string("Apply struct error: ") + e.what();
        return DECOMP_ERR_INIT;
    }
}

// ---------------------------------------------------------------------------
// Pcode bridge initialisation — called from Rust at process startup.
// Registers function pointers directly, bypassing dlsym on macOS/Linux.
// ---------------------------------------------------------------------------
void decomp_init_pcode_bridge(
    char* (*optimize_fn)(const char*, size_t),
    void  (*free_fn)(char*)
) {
    fission::decompiler::PcodeOptimizationBridge::register_rust_fn_ptrs(optimize_fn, free_fn);
}
