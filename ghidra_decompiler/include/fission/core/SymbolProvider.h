#ifndef FISSION_CORE_SYMBOL_PROVIDER_H
#define FISSION_CORE_SYMBOL_PROVIDER_H

#include <cstdint>
#include <map>
#include <memory>
#include <string>

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

} // namespace core
} // namespace fission

#endif // FISSION_CORE_SYMBOL_PROVIDER_H
