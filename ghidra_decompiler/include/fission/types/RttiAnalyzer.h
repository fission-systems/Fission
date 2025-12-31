#ifndef FISSION_TYPES_RTTI_ANALYZER_H
#define FISSION_TYPES_RTTI_ANALYZER_H

#include <vector>
#include <string>
#include <map>
#include <cstdint>

namespace fission {
namespace types {

// Minimal RTTI structures for MSVC
struct RTTICompleteObjectLocator {
    uint32_t signature;
    uint32_t offset;
    uint32_t cdOffset;
    uint32_t typeDescriptorOffset; // ImageBase relative
    uint32_t classDescriptorOffset; // ImageBase relative
    uint32_t objectBaseOffset; // ImageBase relative (x64 only)
};

class RttiAnalyzer {
public:
    // Scan binary for RTTI structures and recover class names
    // Returns map of vftable address -> class name
    static std::map<uint64_t, std::string> recover_class_names(
        const std::vector<uint8_t>& bytes, 
        uint64_t image_base,
        bool is_64bit
    );
};

} // namespace types
} // namespace fission

#endif // FISSION_TYPES_RTTI_ANALYZER_H
