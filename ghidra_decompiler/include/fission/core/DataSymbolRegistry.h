#ifndef FISSION_CORE_DATA_SYMBOL_REGISTRY_H
#define FISSION_CORE_DATA_SYMBOL_REGISTRY_H

#include "fission/loaders/DataSectionScanner.h"

#include <functional>
#include <vector>

namespace ghidra {
class Architecture;
}

namespace fission {
namespace ffi {
struct DecompContext;
}

namespace core {

/// Register scanned data symbols into the architecture global scope.
/// Optionally invokes `on_scanned_symbol` for each scanned symbol.
int registerDataSymbolsInGlobalScope(
    ghidra::Architecture* arch,
    const std::vector<fission::loaders::DataSymbol>& symbols,
    const std::function<void(const fission::loaders::DataSymbol&)>& on_scanned_symbol = {}
);

/// Scan data sections from context and register discovered symbols.
void registerDataSectionSymbols(fission::ffi::DecompContext* ctx);

} // namespace core
} // namespace fission

#endif // FISSION_CORE_DATA_SYMBOL_REGISTRY_H
