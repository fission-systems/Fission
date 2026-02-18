#ifndef FISSION_CORE_DATA_SYMBOL_REGISTRY_H
#define FISSION_CORE_DATA_SYMBOL_REGISTRY_H

#include "fission/loaders/DataSectionScanner.h"

#include <cstdint>
#include <functional>
#include <string>
#include <vector>

namespace ghidra {
class Architecture;
}

namespace fission {
namespace ffi {
struct DecompContext;
}

namespace core {

// ---------------------------------------------------------------------------
// Lightweight PE data-section descriptor (no file I/O, derived from raw bytes)
// ---------------------------------------------------------------------------
struct PeDataSection {
    std::string  name;
    uint64_t     va_addr    = 0;
    uint32_t     file_offset = 0;
    uint32_t     file_size   = 0;
};

/// Parse PE header and return .rdata/.data section descriptors.
/// Returns empty vector if the binary is not a PE or is malformed.
std::vector<PeDataSection> extract_pe_data_sections(
    const uint8_t* data,
    size_t         size,
    uint64_t       image_base
);

/// Register scanned data symbols into the architecture global scope.
/// Optionally invokes `on_scanned_symbol` for each scanned symbol.
int registerDataSymbolsInGlobalScope(
    ghidra::Architecture* arch,
    const std::vector<fission::loaders::DataSymbol>& symbols,
    const std::function<void(const fission::loaders::DataSymbol&)>& on_scanned_symbol = {}
);

/// Scan PE data sections from raw binary bytes and register discovered symbols.
/// Uses extract_pe_data_sections + DataSectionScanner — the single PE parsing path.
int scanAndRegisterDataSymbols(
    ghidra::Architecture*  arch,
    const uint8_t*         data,
    size_t                 size,
    uint64_t               image_base,
    const std::function<void(const fission::loaders::DataSymbol&)>& on_scanned_symbol = {}
);

/// Scan data sections from FFI context and register discovered symbols.
void registerDataSectionSymbols(fission::ffi::DecompContext* ctx);

} // namespace core
} // namespace fission

#endif // FISSION_CORE_DATA_SYMBOL_REGISTRY_H
