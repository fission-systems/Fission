/**
 * Fission Symbol Provider Manager
 */

#include "fission/ffi/SymbolProviderManager.h"
#include "fission/core/SymbolProvider.h"

using namespace fission::ffi;

void fission::ffi::set_symbol_provider(DecompContext* ctx, const DecompSymbolProvider* provider) {
    if (!ctx) return;

    std::lock_guard<std::mutex> lock(ctx->mutex);

    if (ctx->symbol_provider_enabled && ctx->symbol_provider_callbacks.drop) {
        ctx->symbol_provider_callbacks.drop(ctx->symbol_provider_callbacks.userdata);
    }

    if (!provider) {
        ctx->symbol_provider_callbacks = DecompSymbolProvider{};
        ctx->symbol_provider_enabled = false;
        ctx->symbol_provider.reset();

        if (ctx->arch) {
            ctx->symbol_provider = std::make_unique<fission::core::MapSymbolProvider>(
                &ctx->symbols,
                &ctx->global_symbols
            );
            ctx->arch->setSymbolProvider(ctx->symbol_provider.get());
        }
        return;
    }

    ctx->symbol_provider_callbacks = *provider;
    ctx->symbol_provider_enabled = true;
    ctx->symbol_provider = std::make_unique<fission::core::CallbackSymbolProvider>(
        &ctx->symbol_provider_callbacks
    );

    if (ctx->arch) {
        ctx->arch->setSymbolProvider(ctx->symbol_provider.get());
    }
}
