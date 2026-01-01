#ifndef FISSION_TYPES_PATTERN_LOADER_H
#define FISSION_TYPES_PATTERN_LOADER_H

#include <vector>
#include <string>
#include <map>
#include <cstdint>

namespace fission {
namespace types {

// Simplified pattern matcher for function identification.
// In reality, full FID involves hashing.
// Here we support loading generic byte signatures for function starts or specific library functions.

struct BytePattern {
    std::string name;       // "Function Start" or "memcpy", etc.
    std::vector<uint8_t> bytes;
    std::vector<bool> mask; // true = exact match, false = wildcard
};

class PatternLoader {
public:
    // Load patterns from XML (simplified) or hardcoded standard patterns
    static std::vector<BytePattern> load_standard_patterns();
    
    // Scan memory for patterns and return map of Address -> Name
    static std::map<uint64_t, std::string> match_functions(
        const std::vector<uint8_t>& memory, 
        uint64_t base_address,
        const std::vector<BytePattern>& patterns
    );

    /**
     * Scan for function prologues in the binary
     * @param memory Binary bytes
     * @param base_address Image base address
     * @return Vector of potential function entry points (addresses)
     */
    static std::vector<uint64_t> scan_function_prologues(
        const std::vector<uint8_t>& memory,
        uint64_t base_address
    );
};

} // namespace types
} // namespace fission

#endif // FISSION_TYPES_PATTERN_LOADER_H
