/**
 * Fission Symbol Manager Implementation
 */

#include "fission/ffi/SymbolManager.h"
#include "libdecomp.hh"

#include <iostream>
#include <sstream>

using namespace fission::ffi;

void fission::ffi::add_symbol(DecompContext* ctx, uint64_t addr, const char* name) {
    if (!ctx || !name) return;
    
    std::lock_guard<std::mutex> lock(ctx->mutex);
    ctx->symbols[addr] = name;
}

void fission::ffi::clear_symbols(DecompContext* ctx) {
    if (!ctx) return;
    
    std::lock_guard<std::mutex> lock(ctx->mutex);
    ctx->symbols.clear();
}

DecompError fission::ffi::add_function(DecompContext* ctx, uint64_t addr, const char* name) {
    if (!ctx) return DECOMP_ERR_INVALID_CONTEXT;
    
    std::lock_guard<std::mutex> lock(ctx->mutex);
    
    try {
        // Store the function declaration (will be applied when arch is created)
        std::string func_name = name ? name : ("FUN_" + std::to_string(addr));
        ctx->symbols[addr] = func_name;
        
        // If architecture already exists, add function immediately
        if (ctx->arch && ctx->memory_image) {
            ghidra::Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
            if (global_scope) {
                ghidra::Address func_addr(ctx->arch->getDefaultCodeSpace(), addr);
                
                // Check if function already exists
                ghidra::Funcdata* existing = global_scope->findFunction(func_addr);
                if (!existing) {
                    // Create new function
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
