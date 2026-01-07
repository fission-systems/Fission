#ifndef FISSION_FFI_SYMBOL_PROVIDER_MANAGER_H
#define FISSION_FFI_SYMBOL_PROVIDER_MANAGER_H

#include "fission/ffi/DecompContext.h"
#include "fission/ffi/SymbolProviderFfi.h"

namespace fission {
namespace ffi {

void set_symbol_provider(DecompContext* ctx, const DecompSymbolProvider* provider);
void reset_symbol_provider(DecompContext* ctx);

} // namespace ffi
} // namespace fission

#endif // FISSION_FFI_SYMBOL_PROVIDER_MANAGER_H
