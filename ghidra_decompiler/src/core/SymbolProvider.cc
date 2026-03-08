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

// -----------------------------------------------------------------------------
// CachedCallbackSymbolProvider
// -----------------------------------------------------------------------------

CachedCallbackSymbolProvider::CachedCallbackSymbolProvider(const DecompSymbolProvider* provider)
    : inner_(provider) {}

bool CachedCallbackSymbolProvider::find_symbol(
    uint64_t address,
    uint32_t size,
    bool require_start,
    SymbolInfo& out
) const {
    SymbolCacheKey key{address, size, static_cast<uint8_t>(require_start ? 1u : 0u)};

    auto it = symbol_map_.find(key);
    if (it != symbol_map_.end()) {
        out = it->second->second;
        symbol_lru_.splice(symbol_lru_.begin(), symbol_lru_, it->second);
        return true;
    }

    if (!inner_.find_symbol(address, size, require_start, out)) {
        return false;
    }

    if (symbol_map_.size() >= kSymbolCacheSize) {
        auto& oldest = symbol_lru_.back();
        symbol_map_.erase(oldest.first);
        symbol_lru_.pop_back();
    }
    symbol_lru_.emplace_front(key, out);
    symbol_map_[key] = symbol_lru_.begin();
    return true;
}

bool CachedCallbackSymbolProvider::find_function(uint64_t address, SymbolInfo& out) const {
    auto it = function_map_.find(address);
    if (it != function_map_.end()) {
        out = it->second->second;
        function_lru_.splice(function_lru_.begin(), function_lru_, it->second);
        return true;
    }

    if (!inner_.find_function(address, out)) {
        return false;
    }

    if (function_map_.size() >= kFunctionCacheSize) {
        auto& oldest = function_lru_.back();
        function_map_.erase(oldest.first);
        function_lru_.pop_back();
    }
    function_lru_.emplace_front(address, out);
    function_map_[address] = function_lru_.begin();
    return true;
}

} // namespace core
} // namespace fission
