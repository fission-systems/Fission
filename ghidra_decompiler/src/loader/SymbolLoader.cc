#include "fission/loader/SymbolLoader.h"
#include "fission/utils/json_utils.h"
#include "fission/utils/file_utils.h"
#include <iostream>

namespace fission {
namespace loader {

std::map<uint64_t, std::string> SymbolLoader::load_symbols_json(const std::string& path) {
    std::map<uint64_t, std::string> symbols;
    
    std::string content = fission::utils::read_file_content(path);
    if (content.empty()) return symbols;
    
    // Very simple JSON parse for flat k:v object (requires robust parser ideally)
    // For now, let's assume a simple line-based format or basic regex-like scanning
    // as we don't have a full JSON DOM library in utils yet (only extract helpers).
    
    // Fallback: expect "address": "name" format
    // "0x401000": "main",
    
    size_t pos = 0;
    while (pos < content.length()) {
        size_t quote1 = content.find('"', pos);
        if (quote1 == std::string::npos) break;
        
        size_t quote2 = content.find('"', quote1 + 1);
        if (quote2 == std::string::npos) break;
        
        std::string key = content.substr(quote1 + 1, quote2 - quote1 - 1);
        
        size_t colon = content.find(':', quote2);
        if (colon == std::string::npos) break;
        
        size_t quote3 = content.find('"', colon);
        if (quote3 == std::string::npos) break;
        
        size_t quote4 = content.find('"', quote3 + 1);
        if (quote4 == std::string::npos) break;
        
        std::string val = content.substr(quote3 + 1, quote4 - quote3 - 1);
        
        // Parse key as address
        try {
            uint64_t addr = std::stoull(key, nullptr, 0); // Handles 0x prefix
            symbols[addr] = val;
        } catch (...) {}
        
        pos = quote4 + 1;
    }
    
    return symbols;
}

} // namespace loader
} // namespace fission
