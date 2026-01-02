#ifndef __BINARY_DETECTOR_H__
#define __BINARY_DETECTOR_H__

#include <cstdint>
#include <string>
#include <vector>

namespace fission {
namespace loader {

/**
 * Binary format types supported by Fission
 */
enum class BinaryFormat {
    PE,         // Windows Portable Executable
    ELF,        // Linux/Unix ELF
    MACHO,      // macOS/iOS Mach-O
    UNKNOWN
};

/**
 * Architecture types
 */
enum class ArchType {
    X86,        // 32-bit x86
    X86_64,     // 64-bit x86
    ARM,        // 32-bit ARM
    ARM64,      // 64-bit ARM (AArch64)
    UNKNOWN
};

/**
 * Binary detection result
 */
struct BinaryInfo {
    BinaryFormat format = BinaryFormat::UNKNOWN;
    ArchType arch = ArchType::UNKNOWN;
    bool is_64bit = false;
    uint64_t image_base = 0;
    uint64_t entry_point = 0;
    std::string sleigh_id;      // e.g., "x86:LE:64:default"
    std::string compiler_id;    // e.g., "windows", "gcc", "clang"
};

/**
 * BinaryDetector - Unified binary format detection
 * 
 * Detects PE, ELF, and Mach-O binaries and determines
 * the appropriate Sleigh specification and compiler ID.
 */
class BinaryDetector {
public:
    /**
     * Detect binary format from raw bytes
     * @param data Pointer to binary data
     * @param size Size of binary data
     * @return BinaryInfo with detected attributes
     */
    static BinaryInfo detect(const uint8_t* data, size_t size);
    
    /**
     * Get appropriate Sleigh ID for the detected binary
     */
    static std::string get_sleigh_id(BinaryFormat format, ArchType arch);
    
    /**
     * Get compiler/OS ID for GDT selection
     */
    static std::string get_compiler_id(BinaryFormat format);
    
    /**
     * Check if format is valid executable
     */
    static bool is_valid_executable(const uint8_t* data, size_t size);

private:
    // PE detection helpers
    static bool is_pe(const uint8_t* data, size_t size);
    static BinaryInfo parse_pe(const uint8_t* data, size_t size);
    
    // ELF detection helpers
    static bool is_elf(const uint8_t* data, size_t size);
    static BinaryInfo parse_elf(const uint8_t* data, size_t size);
    
    // Mach-O detection helpers
    static bool is_macho(const uint8_t* data, size_t size);
    static BinaryInfo parse_macho(const uint8_t* data, size_t size);
};

} // namespace loader
} // namespace fission

#endif
