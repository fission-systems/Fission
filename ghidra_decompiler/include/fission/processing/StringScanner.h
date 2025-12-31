#ifndef FISSION_PROCESSING_STRING_SCANNER_H
#define FISSION_PROCESSING_STRING_SCANNER_H

#include <vector>
#include <string>
#include <map>
#include <cstdint>

namespace fission {
namespace processing {

// Scan memory for potential strings and return a map of Address -> String
class StringScanner {
public:
    // Scan for null-terminated ASCII strings (min length > 3)
    // Only considers printable characters.
    static std::map<uint64_t, std::string> scan_ascii_strings(
        const std::vector<uint8_t>& memory, 
        uint64_t base_address
    );

    // Scan for null-terminated UTF-16LE strings (min length > 3)
    // Returns them as UTF-8 encoded strings for display.
    static std::map<uint64_t, std::string> scan_unicode_strings(
        const std::vector<uint8_t>& memory, 
        uint64_t base_address
    );
};

} // namespace processing
} // namespace fission

#endif // FISSION_PROCESSING_STRING_SCANNER_H
