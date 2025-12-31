#ifndef FISSION_LOADER_PE_HEADER_H
#define FISSION_LOADER_PE_HEADER_H

#include <vector>
#include <cstdint>
#include <string>

namespace fission {
namespace loader {

// Simplified PE structures for detection
struct PeDetectionResult {
    bool is_pe;
    bool is_64bit;
    std::string compiler_id; // "windows", "gcc", etc.
    std::string pdb_path;    // New field for PDB path
    std::string pdb_guid;    // New field for PDB GUID (string representation)
    uint64_t image_base;
};

// Detect architecture and compiler from raw bytes
PeDetectionResult detect_pe_arch(const std::vector<uint8_t>& bytes);

} // namespace loader
} // namespace fission

#endif // FISSION_LOADER_PE_HEADER_H
