#include "fission/processing/StringScanner.h"
#include <cctype>

namespace fission {
namespace processing {

std::map<uint64_t, std::string> StringScanner::scan_ascii_strings(
    const std::vector<uint8_t>& memory, 
    uint64_t base_address
) {
    std::map<uint64_t, std::string> strings;
    size_t len = memory.size();
    if (len < 4) return strings;

    size_t i = 0;
    while (i < len) {
        // Find start of printable sequence
        if (std::isprint(memory[i]) && memory[i] != 0) {
            size_t start = i;
            size_t current = i;
            
            // Collect string, allowing printable chars and common escape sequences
            while (current < len && memory[current] != 0) {
                uint8_t c = memory[current];
                // Allow printable chars and common whitespace/format chars
                if (std::isprint(c) || c == '\n' || c == '\r' || c == '\t') {
                    current++;
                } else {
                    break;  // Hit non-string character
                }
            }
            
            // Check terminator
            if (current < len && memory[current] == 0 && current > start) {
                // Must be at least 4 chars to be interesting
                if (current - start >= 4) {
                    std::string s(memory.begin() + start, memory.begin() + current);
                    // Add quotes for C-style literal
                    strings[base_address + start] = "\"" + s + "\"";
                }
            }
            // Skip past this block
            i = current + 1;
        } else {
            i++;
        }
    }
    return strings;
}

std::map<uint64_t, std::string> StringScanner::scan_unicode_strings(
    const std::vector<uint8_t>& memory, 
    uint64_t base_address
) {
    std::map<uint64_t, std::string> strings;
    size_t len = memory.size();
    if (len < 8) return strings; // Wchar is 2 bytes, min 4 chars = 8 bytes

    size_t i = 0;
    while (i < len - 1) {
        // Check for printable ASCII in UTF-16LE (char, 0x00)
        // Basic heuristic: simple alphanumeric latin range
        // byte[i] is char, byte[i+1] is 0x00
        if (std::isprint(memory[i]) && memory[i+1] == 0) {
            size_t start = i;
            size_t current = i; // byte index
            
            std::string extracted;
            
            while (current < len - 1 && std::isprint(memory[current]) && memory[current+1] == 0) {
                extracted += (char)memory[current];
                current += 2;
            }
            
            // Check terminator (00 00)
            if (current < len - 1 && memory[current] == 0 && memory[current+1] == 0) {
                if (extracted.length() >= 4) {
                    // Store formatted as Wide String Literal for display
                    // e.g. L"Hello"
                    strings[base_address + start] = "L\"" + extracted + "\"";
                }
            }
            
            i = current + 2; 
        } else {
            i += 1; // Unaligned check allowed? usually aligned to 2 bytes but let's be loose
        }
    }
    return strings;
}

} // namespace processing
} // namespace fission
