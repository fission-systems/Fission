#include "fission/loader/SymbolLoader.h"
#include "fission/utils/json_utils.h"
#include "fission/utils/file_utils.h"
#include <iostream>
#include <fstream>

namespace fission {
namespace loader {

std::map<uint64_t, std::string> SymbolLoader::load_symbols_json(const std::string& path) {
    std::map<uint64_t, std::string> symbols;
    
    std::string content = fission::utils::read_file_content(path);
    if (content.empty()) return symbols;
    
    // Use the robust parser from utils
    return fission::utils::parse_json_string_map(content);
}

std::map<uint64_t, std::string> SymbolLoader::load_symbols_text(const std::string& path) {
    std::map<uint64_t, std::string> symbols;
    std::ifstream ifs(path);
    if (!ifs.is_open()) return symbols;

    std::string line;
    while (std::getline(ifs, line)) {
        if (!line.empty() && line[0] != '#') {
            size_t space = line.find(' ');
            if (space != std::string::npos && line.substr(0, 2) == "0x") {
                try {
                    uint64_t addr = std::stoull(line.substr(2, space - 2), nullptr, 16);
                    std::string name = line.substr(space + 1);
                    // Trim newline if present
                    if (!name.empty() && name.back() == '\r') name.pop_back();
                    symbols[addr] = name;
                } catch (...) {
                    // Ignore malformed lines
                }
            }
        }
    }
    return symbols;
}

} // namespace loader
} // namespace fission
