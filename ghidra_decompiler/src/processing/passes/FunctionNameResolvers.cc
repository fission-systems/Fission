#include "fission/processing/PostProcessors.h"

#include <string>
#include <map>
#include <cctype>
#include <cstdio>

namespace fission {
namespace processing {

std::string improve_internal_function_names(const std::string& code) {
    std::string result = code;
    
    // Replace func_0x pattern with sub_ (standard disassembler convention)
    size_t pos = 0;
    while ((pos = result.find("func_0x", pos)) != std::string::npos) {
        // Extract the address
        size_t addr_start = pos + 7; // after "func_0x"
        size_t addr_end = addr_start;
        while (addr_end < result.size() && isxdigit(result[addr_end])) {
            addr_end++;
        }
        
        if (addr_end > addr_start) {
            std::string addr = result.substr(addr_start, addr_end - addr_start);
            // Use sub_XXXX format (shorter, more readable)
            std::string replacement = "sub_" + addr;
            result.replace(pos, addr_end - pos, replacement);
            pos += replacement.length();
        } else {
            pos++;
        }
    }
    
    return result;
}

std::string apply_fid_names(const std::string& code, const std::map<uint64_t, std::string>& fid_names) {
    if (fid_names.empty()) return code;
    
    std::string result = code;
    
    // Replace sub_XXXXXXXX with FID-resolved names
    for (const auto& [addr, name] : fid_names) {
        // Format address as 8-character hex (no leading 0x for sub_)
        char addr_str[16];
        snprintf(addr_str, sizeof(addr_str), "%08llx", (unsigned long long)addr);
        
        std::string pattern = "sub_" + std::string(addr_str);
        
        size_t pos = 0;
        while ((pos = result.find(pattern, pos)) != std::string::npos) {
            result.replace(pos, pattern.length(), name);
            pos += name.length();
        }
    }
    
    return result;
}

} // namespace processing
} // namespace fission