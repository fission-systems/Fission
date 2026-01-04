#include "fission/decompiler/PostProcessor.h"
#include <vector>
#include <cctype>

namespace fission {
namespace decompiler {

std::string PostProcessor::convert_integer_constants(std::string c_code) {
    // Manual scan for hex patterns: 0x[0-9a-fA-F]+
    size_t pos = 0;
    while ((pos = c_code.find("0x", pos)) != std::string::npos) {
        size_t start = pos;
        size_t end = start + 2;
        while (end < c_code.length() && isxdigit(c_code[end])) {
            end++;
        }
        
        size_t len = end - start;
        if (len > 4) { // Only check if longer than single byte (0xXX)
            std::string hex_str = c_code.substr(start, len);
            try {
                unsigned long long val = std::stoull(hex_str, nullptr, 16);
                
                // Extract bytes (Little Endian for x86)
                std::string decoded;
                bool is_ascii = true;
                unsigned long long temp = val;
                
                std::vector<char> bytes;
                while (temp > 0) {
                    char c = (char)(temp & 0xFF);
                    bytes.push_back(c);
                    temp >>= 8;
                }
                
                // If empty, it was 0x0 - not a string
                if (bytes.empty()) is_ascii = false;
                
                int valid_chars = 0;
                for (char c : bytes) {
                    if (c == 0) continue; // Allow null terminators
                    if (isalnum(c) || ispunct(c) || c == ' ') {
                        valid_chars++;
                        decoded += c;
                    } else {
                        is_ascii = false;
                        break;
                    }
                }
                
                if (is_ascii && valid_chars >= 3) {
                    // Found a string! Format: (QWORD)"string" or (DWORD)"string"
                    std::string replacement = "\"" + decoded + "\"";
                    if (len > 10) { // > 4 bytes -> QWORD
                        replacement = "(QWORD)" + replacement;
                    } else {
                        replacement = "(DWORD)" + replacement;
                    }
                    
                    c_code.replace(start, len, replacement);
                    pos = start + replacement.length();
                    continue;
                }
            } catch (...) {
                // Ignore conversion errors
            }
        }
        pos = end;
    }
    return c_code;
}

std::string PostProcessor::process(const std::string& c_code) {
    std::string result = c_code;
    result = convert_integer_constants(result);
    return result;
}

} // namespace decompiler
} // namespace fission
