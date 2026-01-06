#include "fission/core/SymbolProvider.h"
#include "fission/ffi/SymbolProviderFfi.h"

#include <cstring>

namespace fission {
namespace core {

MapSymbolProvider::MapSymbolProvider(
    const std::map<uint64_t, std::string>* function_symbols,
    const std::map<uint64_t, std::string>* data_symbols
)
    : function_symbols_(function_symbols), data_symbols_(data_symbols) {}

bool MapSymbolProvider::find_symbol(
    uint64_t address,
    uint32_t size,
    bool require_start,
    SymbolInfo& out
) const {
    (void)size;
    (void)require_start;

    if (!data_symbols_) {
        return false;
    }

    auto it = data_symbols_->find(address);
    if (it == data_symbols_->end()) {
        return false;
    }

    out.address = address;
    out.size = 1;
    out.flags = SymbolFlagData;
    out.name = it->second;
    return true;
}

bool MapSymbolProvider::find_function(uint64_t address, SymbolInfo& out) const {
    if (!function_symbols_) {
        return false;
    }

    auto it = function_symbols_->find(address);
    if (it == function_symbols_->end()) {
        return false;
    }

    out.address = address;
    out.size = 0;
    out.flags = SymbolFlagFunction;
    out.name = it->second;
    return true;
}

CallbackSymbolProvider::CallbackSymbolProvider(const DecompSymbolProvider* provider)
    : provider_(provider) {}

void CallbackSymbolProvider::set_provider(const DecompSymbolProvider* provider) {
    provider_ = provider;
}

bool CallbackSymbolProvider::find_symbol(
    uint64_t address,
    uint32_t size,
    bool require_start,
    SymbolInfo& out
) const {
    if (!provider_ || !provider_->find_symbol) {
        return false;
    }

    DecompSymbolInfo info{};
    int ok = provider_->find_symbol(
        provider_->userdata,
        address,
        size,
        require_start ? 1 : 0,
        &info
    );
    if (ok == 0 || info.name == nullptr) {
        return false;
    }

    out.address = info.address;
    out.size = info.size;
    out.flags = info.flags;
    if (info.name_len > 0) {
        out.name.assign(info.name, info.name + info.name_len);
    } else {
        out.name = info.name;
    }

    return true;
}

bool CallbackSymbolProvider::find_function(uint64_t address, SymbolInfo& out) const {
    if (!provider_ || !provider_->find_function) {
        return false;
    }

    DecompSymbolInfo info{};
    int ok = provider_->find_function(provider_->userdata, address, &info);
    if (ok == 0 || info.name == nullptr) {
        return false;
    }

    out.address = info.address;
    out.size = info.size;
    out.flags = info.flags;
    if (info.name_len > 0) {
        out.name.assign(info.name, info.name + info.name_len);
    } else {
        out.name = info.name;
    }

    return true;
}

} // namespace core
} // namespace fission
