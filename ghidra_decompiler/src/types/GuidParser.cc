#include "fission/types/GuidParser.h"
#include <sstream>
#include <algorithm>

namespace fission {
namespace types {

std::map<std::string, std::string> load_guids_to_map(const std::string& content) {
    std::map<std::string, std::string> guid_map;
    std::stringstream ss(content);
    std::string line;
    
    while (std::getline(ss, line)) {
        if (line.empty()) continue;
        
        std::stringstream ls(line);
        std::string uuid, name;
        
        // Expect format: UUID Name
        // We read UUID first, then the rest is the name (or just the second token)
        if (ls >> uuid >> name) {
            // Basic validation: UUID should be reasonably long
            if (uuid.length() >= 36) {
                // Ensure UUID is formatted with braces for consistency in replacement code later
                // The input files are raw: 00000000-0000-0000-C000-000000000046
                // Ghidra decompiler output might format them differently, but usually as raw hex bytes or struct init.
                // However, for string replacement, we'll store the raw string for flexible matching
                // or normalized to UPPERCASE.
                
                // Let's store normalized UPPERCASE uuid
                std::transform(uuid.begin(), uuid.end(), uuid.begin(), ::toupper);
                
                if (!name.empty()) {
                    guid_map[uuid] = name;
                }
            }
        }
    }
    return guid_map;
}

} // namespace types
} // namespace fission
