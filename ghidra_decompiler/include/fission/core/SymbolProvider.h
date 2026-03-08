#ifndef FISSION_CORE_SYMBOL_PROVIDER_H
#define FISSION_CORE_SYMBOL_PROVIDER_H

#include <cstdint>
#include <list>
#include <map>
#include <memory>
#include <string>
#include <unordered_map>

struct DecompSymbolProvider;

namespace fission {
namespace core {

struct SymbolInfo {
    uint64_t address = 0;
    uint32_t size = 0;
    uint32_t flags = 0;
    std::string name;
};

enum SymbolFlags : uint32_t {
    SymbolFlagFunction = 1u << 0,
    SymbolFlagData = 1u << 1,
    SymbolFlagExternal = 1u << 2,
    SymbolFlagReadOnly = 1u << 3,
    SymbolFlagVolatile = 1u << 4,
};

class SymbolProvider {
public:
    virtual ~SymbolProvider() = default;

    virtual bool find_symbol(
        uint64_t address,
        uint32_t size,
        bool require_start,
        SymbolInfo& out
    ) const = 0;

    virtual bool find_function(uint64_t address, SymbolInfo& out) const = 0;
};

class MapSymbolProvider final : public SymbolProvider {
public:
    MapSymbolProvider(
        const std::map<uint64_t, std::string>* function_symbols,
        const std::map<uint64_t, std::string>* data_symbols
    );

    bool find_symbol(
        uint64_t address,
        uint32_t size,
        bool require_start,
        SymbolInfo& out
    ) const override;

    bool find_function(uint64_t address, SymbolInfo& out) const override;

private:
    const std::map<uint64_t, std::string>* function_symbols_;
    const std::map<uint64_t, std::string>* data_symbols_;
};

class CallbackSymbolProvider final : public SymbolProvider {
public:
    explicit CallbackSymbolProvider(const DecompSymbolProvider* provider);

    void set_provider(const DecompSymbolProvider* provider);

    bool find_symbol(
        uint64_t address,
        uint32_t size,
        bool require_start,
        SymbolInfo& out
    ) const override;

    bool find_function(uint64_t address, SymbolInfo& out) const override;

private:
    const DecompSymbolProvider* provider_;
};

/** Cache key for find_symbol: (address, size, require_start) */
struct SymbolCacheKey {
    uint64_t addr = 0;
    uint32_t size = 0;
    uint8_t require_start = 0;

    bool operator==(const SymbolCacheKey& other) const {
        return addr == other.addr && size == other.size && require_start == other.require_start;
    }
};

} // namespace core
} // namespace fission

namespace std {
template<>
struct hash<fission::core::SymbolCacheKey> {
    size_t operator()(const fission::core::SymbolCacheKey& k) const {
        return hash<uint64_t>()(k.addr) ^ (hash<uint32_t>()(k.size) << 1) ^
               hash<uint8_t>()(k.require_start);
    }
};
} // namespace std

namespace fission {
namespace core {

/**
 * Wraps CallbackSymbolProvider with an LRU cache (4096 symbols + 4096 functions)
 * to avoid FFI callbacks on repeated lookups of the same (addr, size, require_start).
 */
class CachedCallbackSymbolProvider final : public SymbolProvider {
public:
    explicit CachedCallbackSymbolProvider(const DecompSymbolProvider* provider);

    bool find_symbol(
        uint64_t address,
        uint32_t size,
        bool require_start,
        SymbolInfo& out
    ) const override;

    bool find_function(uint64_t address, SymbolInfo& out) const override;

private:
    CallbackSymbolProvider inner_;
    static constexpr size_t kSymbolCacheSize = 4096;
    static constexpr size_t kFunctionCacheSize = 4096;

    using SymbolCacheList = std::list<std::pair<SymbolCacheKey, SymbolInfo>>;
    using FunctionCacheList = std::list<std::pair<uint64_t, SymbolInfo>>;

    mutable SymbolCacheList symbol_lru_;
    mutable std::unordered_map<SymbolCacheKey, SymbolCacheList::iterator> symbol_map_;
    mutable FunctionCacheList function_lru_;
    mutable std::unordered_map<uint64_t, FunctionCacheList::iterator> function_map_;
};

} // namespace core
} // namespace fission

#endif // FISSION_CORE_SYMBOL_PROVIDER_H
