/**
 * Fission Symbol Manager Implementation
 */

#include "fission/ffi/SymbolManager.h"
#include "fission/core/ContextServices.h"

using namespace fission::ffi;

void fission::ffi::add_symbol(DecompContext* ctx, uint64_t addr, const char* name) {
    fission::core::add_symbol(ctx, addr, name);
}

void fission::ffi::clear_symbols(DecompContext* ctx) {
    fission::core::clear_symbols(ctx);
}

void fission::ffi::add_global_symbol(DecompContext* ctx, uint64_t addr, const char* name) {
    fission::core::add_global_symbol(ctx, addr, name);
}

void fission::ffi::clear_global_symbols(DecompContext* ctx) {
    fission::core::clear_global_symbols(ctx);
}

DecompError fission::ffi::add_function(DecompContext* ctx, uint64_t addr, const char* name) {
    return fission::core::add_function(ctx, addr, name);
}
