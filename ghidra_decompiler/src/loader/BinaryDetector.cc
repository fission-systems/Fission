#include "fission/loader/BinaryDetector.h"

#include <cstring>
#include <iostream>
#include "fission/utils/logger.h"

namespace fission {
namespace loader {

// Magic bytes
static const uint8_t PE_MAGIC[] = { 0x4D, 0x5A };  // "MZ"
static const uint8_t ELF_MAGIC[] = { 0x7F, 0x45, 0x4C, 0x46 };  // "\x7FELF"
static const uint32_t MACHO_MAGIC_32 = 0xFEEDFACE;
static const uint32_t MACHO_MAGIC_64 = 0xFEEDFACF;
static const uint32_t MACHO_CIGAM_32 = 0xCEFAEDFE;  // Byte-swapped
static const uint32_t MACHO_CIGAM_64 = 0xCFFAEDFE;

BinaryInfo BinaryDetector::detect(const uint8_t* data, size_t size) {
    BinaryInfo info;
    
    if (!data || size < 64) {
        return info;
    }
    
    // Check PE first (most common for our use case)
    if (is_pe(data, size)) {
        return parse_pe(data, size);
    }
    
    // Check ELF
    if (is_elf(data, size)) {
        return parse_elf(data, size);
    }
    
    // Check Mach-O
    if (is_macho(data, size)) {
        return parse_macho(data, size);
    }
    
    return info;
}

bool BinaryDetector::is_pe(const uint8_t* data, size_t size) {
    if (size < 2) return false;
    return data[0] == PE_MAGIC[0] && data[1] == PE_MAGIC[1];
}

bool BinaryDetector::is_elf(const uint8_t* data, size_t size) {
    if (size < 4) return false;
    return memcmp(data, ELF_MAGIC, 4) == 0;
}

bool BinaryDetector::is_macho(const uint8_t* data, size_t size) {
    if (size < 4) return false;
    uint32_t magic = *(const uint32_t*)data;
    return magic == MACHO_MAGIC_32 || magic == MACHO_MAGIC_64 ||
           magic == MACHO_CIGAM_32 || magic == MACHO_CIGAM_64;
}

bool BinaryDetector::is_valid_executable(const uint8_t* data, size_t size) {
    return is_pe(data, size) || is_elf(data, size) || is_macho(data, size);
}

BinaryInfo BinaryDetector::parse_pe(const uint8_t* data, size_t size) {
    BinaryInfo info;
    info.format = BinaryFormat::PE;
    info.compiler_id = "windows";
    
    // Get PE header offset from DOS header
    if (size < 64) return info;
    uint32_t pe_offset = *(const uint32_t*)(data + 0x3C);
    
    if (pe_offset + 24 > size) return info;
    
    // Check PE signature
    if (memcmp(data + pe_offset, "PE\0\0", 4) != 0) return info;
    
    // Machine type at PE+4
    uint16_t machine = *(const uint16_t*)(data + pe_offset + 4);
    
    switch (machine) {
        case 0x014C:  // IMAGE_FILE_MACHINE_I386
            info.arch = ArchType::X86;
            info.is_64bit = false;
            info.sleigh_id = "x86:LE:32:default";
            break;
        case 0x8664:  // IMAGE_FILE_MACHINE_AMD64
            info.arch = ArchType::X86_64;
            info.is_64bit = true;
            info.sleigh_id = "x86:LE:64:default";
            break;
        case 0xAA64:  // IMAGE_FILE_MACHINE_ARM64
            info.arch = ArchType::ARM64;
            info.is_64bit = true;
            info.sleigh_id = "AARCH64:LE:64:v8A";
            break;
        case 0x01C0:  // IMAGE_FILE_MACHINE_ARM
        case 0x01C4:  // IMAGE_FILE_MACHINE_ARMNT
            info.arch = ArchType::ARM;
            info.is_64bit = false;
            info.sleigh_id = "ARM:LE:32:v7";
            break;
        default:
            info.arch = ArchType::UNKNOWN;
            break;
    }
    
    // Get image base from Optional Header
    uint16_t optional_header_magic = *(const uint16_t*)(data + pe_offset + 24);
    if (optional_header_magic == 0x20B) {  // PE32+
        info.image_base = *(const uint64_t*)(data + pe_offset + 24 + 24);
        info.entry_point = info.image_base + *(const uint32_t*)(data + pe_offset + 24 + 16);
    } else if (optional_header_magic == 0x10B) {  // PE32
        info.image_base = *(const uint32_t*)(data + pe_offset + 24 + 28);
        info.entry_point = info.image_base + *(const uint32_t*)(data + pe_offset + 24 + 16);
    }
    
    fission::utils::log_stream() << "[BinaryDetector] PE: " << (info.is_64bit ? "64-bit" : "32-bit")
              << " Arch=" << info.sleigh_id << std::endl;
    
    return info;
}

BinaryInfo BinaryDetector::parse_elf(const uint8_t* data, size_t size) {
    BinaryInfo info;
    info.format = BinaryFormat::ELF;
    info.compiler_id = "gcc";  // Assume GCC for Linux
    
    if (size < 64) return info;
    
    // ELF class (32/64 bit) at offset 4
    uint8_t elf_class = data[4];
    info.is_64bit = (elf_class == 2);  // ELFCLASS64
    
    // Machine type at offset 18 (for both 32 and 64)
    uint16_t machine = *(const uint16_t*)(data + 18);
    
    switch (machine) {
        case 0x03:  // EM_386
            info.arch = ArchType::X86;
            info.sleigh_id = "x86:LE:32:default";
            break;
        case 0x3E:  // EM_X86_64
            info.arch = ArchType::X86_64;
            info.sleigh_id = "x86:LE:64:default";
            break;
        case 0x28:  // EM_ARM
            info.arch = ArchType::ARM;
            info.sleigh_id = "ARM:LE:32:v7";
            break;
        case 0xB7:  // EM_AARCH64
            info.arch = ArchType::ARM64;
            info.sleigh_id = "AARCH64:LE:64:v8A";
            break;
        default:
            info.arch = ArchType::UNKNOWN;
            info.sleigh_id = "x86:LE:64:default";  // fallback
            break;
    }
    
    // Get entry point
    if (info.is_64bit) {
        info.entry_point = *(const uint64_t*)(data + 24);
    } else {
        info.entry_point = *(const uint32_t*)(data + 24);
    }
    
    fission::utils::log_stream() << "[BinaryDetector] ELF: " << (info.is_64bit ? "64-bit" : "32-bit")
              << " Arch=" << info.sleigh_id << std::endl;
    
    return info;
}

BinaryInfo BinaryDetector::parse_macho(const uint8_t* data, size_t size) {
    BinaryInfo info;
    info.format = BinaryFormat::MACHO;
    info.compiler_id = "clang";  // Assume Clang for macOS
    
    if (size < 32) return info;
    
    uint32_t magic = *(const uint32_t*)data;
    bool is_big_endian = (magic == MACHO_CIGAM_32 || magic == MACHO_CIGAM_64);
    info.is_64bit = (magic == MACHO_MAGIC_64 || magic == MACHO_CIGAM_64);
    
    // CPU type at offset 4
    uint32_t cputype = *(const uint32_t*)(data + 4);
    if (is_big_endian) {
#ifdef _MSC_VER
        cputype = _byteswap_ulong(cputype);
#else
        cputype = __builtin_bswap32(cputype);
#endif
    }
    
    // CPU_TYPE constants
    const uint32_t CPU_TYPE_X86 = 0x7;
    const uint32_t CPU_TYPE_X86_64 = 0x01000007;
    const uint32_t CPU_TYPE_ARM = 0xC;
    const uint32_t CPU_TYPE_ARM64 = 0x0100000C;
    
    switch (cputype) {
        case CPU_TYPE_X86:
            info.arch = ArchType::X86;
            info.sleigh_id = "x86:LE:32:default";
            break;
        case CPU_TYPE_X86_64:
            info.arch = ArchType::X86_64;
            info.sleigh_id = "x86:LE:64:default";
            break;
        case CPU_TYPE_ARM:
            info.arch = ArchType::ARM;
            info.sleigh_id = "ARM:LE:32:v7";
            break;
        case CPU_TYPE_ARM64:
            info.arch = ArchType::ARM64;
            info.sleigh_id = "AARCH64:LE:64:v8A";
            break;
        default:
            info.arch = ArchType::UNKNOWN;
            info.sleigh_id = "AARCH64:LE:64:v8A";  // fallback for modern Macs
            break;
    }
    
    fission::utils::log_stream() << "[BinaryDetector] Mach-O: " << (info.is_64bit ? "64-bit" : "32-bit")
              << " Arch=" << info.sleigh_id << std::endl;
    
    return info;
}

std::string BinaryDetector::get_sleigh_id(BinaryFormat format, ArchType arch) {
    switch (arch) {
        case ArchType::X86:
            return "x86:LE:32:default";
        case ArchType::X86_64:
            return "x86:LE:64:default";
        case ArchType::ARM:
            return "ARM:LE:32:v7";
        case ArchType::ARM64:
            return "AARCH64:LE:64:v8A";
        default:
            return "x86:LE:64:default";
    }
}

std::string BinaryDetector::get_compiler_id(BinaryFormat format) {
    switch (format) {
        case BinaryFormat::PE:
            return "windows";
        case BinaryFormat::ELF:
            return "gcc";
        case BinaryFormat::MACHO:
            return "clang";
        default:
            return "default";
    }
}

} // namespace loader
} // namespace fission
