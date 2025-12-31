#ifndef FISSION_LOADER_SYMBOL_LOADER_H
#define FISSION_LOADER_SYMBOL_LOADER_H

#include <string>
#include <map>
#include <cstdint>

namespace fission {
namespace loader {

class SymbolLoader {
public:
    // Load simple JSON symbol map: { "address": "name", ... }
    static std::map<uint64_t, std::string> load_symbols_json(const std::string& path);
};

} // namespace loader
} // namespace fission

#endif // FISSION_LOADER_SYMBOL_LOADER_H
