/**
 * Fission Symbol Provider Manager
 */

#include "fission/ffi/SymbolProviderManager.h"
#include "fission/core/ContextServices.h"

using namespace fission::ffi;

void fission::ffi::set_symbol_provider(DecompContext* ctx, const DecompSymbolProvider* provider) {
    fission::core::set_symbol_provider(ctx, provider);
}

void fission::ffi::reset_symbol_provider(DecompContext* ctx) {
    fission::core::reset_symbol_provider(ctx);
}
