#include "fission/loader/BinaryDetector.h"

#include <cstring>
#include <iostream>
#include "fission/utils/logger.h"

namespace fission {
namespace loader {

// ============================================================================
// A-4: ELF compiler detection helper
//
// Most Clang-produced ELFs contain "clang version" in the .comment section.
// We parse ELF section headers to locate .comment, then scan its content.
// Falls back to "gcc" if the section is absent or unrecognised.
// ============================================================================
static std::string elf_detect_compiler(const uint8_t* data, size_t size) {
    if (size < 64) return "gcc";

    bool is64 = (data[4] == 2);  // ELFCLASS64
    bool isle = (data[5] == 1);  // ELFDATA2LSB (little-endian)

    auto rd16 = [&](size_t off) -> uint16_t {
        if (off + 2 > size) return 0;
        return isle ? (uint16_t)(data[off] | (data[off+1] << 8))
                    : (uint16_t)((data[off] << 8) | data[off+1]);
    };
    auto rd32 = [&](size_t off) -> uint32_t {
        if (off + 4 > size) return 0;
        if (isle) return (uint32_t)(data[off] | (data[off+1]<<8) | (data[off+2]<<16) | (data[off+3]<<24));
        return (uint32_t)((data[off]<<24)|(data[off+1]<<16)|(data[off+2]<<8)|data[off+3]);
    };
    auto rd64 = [&](size_t off) -> uint64_t {
        if (off + 8 > size) return 0;
        uint64_t lo = rd32(off), hi = rd32(off + 4);
        return isle ? lo | (hi << 32) : (lo << 32) | hi;
    };

    // ELF section header table info
    uint64_t shoff    = is64 ? rd64(40)  : rd32(32);
    uint16_t shentsize= is64 ? rd16(58)  : rd16(46);
    uint16_t shnum    = is64 ? rd16(60)  : rd16(48);
    uint16_t shstrndx = is64 ? rd16(62)  : rd16(50);

    if (shoff == 0 || shentsize == 0 || shnum == 0 ||
        shstrndx >= shnum || shoff + (uint64_t)shnum * shentsize > size)
        return "gcc";

    // Locate section name string table
    size_t strtab_shdr = (size_t)(shoff + (uint64_t)shstrndx * shentsize);
    uint64_t strtab_off  = is64 ? rd64(strtab_shdr + 24) : rd32(strtab_shdr + 16);
    uint64_t strtab_size = is64 ? rd64(strtab_shdr + 32) : rd32(strtab_shdr + 20);
    if (strtab_off + strtab_size > size) return "gcc";

    // Scan section headers for .comment
    for (uint16_t i = 0; i < shnum; ++i) {
        size_t shdr = (size_t)(shoff + (uint64_t)i * shentsize);
        uint32_t sh_name      = rd32(shdr);
        uint64_t sh_data_off  = is64 ? rd64(shdr + 24) : rd32(shdr + 16);
        uint64_t sh_data_size = is64 ? rd64(shdr + 32) : rd32(shdr + 20);

        uint64_t name_off = strtab_off + sh_name;
        if (name_off >= size) continue;
        if (std::strncmp((const char*)(data + name_off), ".comment", 8) != 0) continue;

        // Found .comment — scan for "clang version" marker
        if (sh_data_off + sh_data_size > size) break;
        const char* comment = (const char*)(data + sh_data_off);
        const char* CLANG_MARKER = "clang version ";
        for (size_t j = 0; j + 14 <= (size_t)sh_data_size; ++j) {
            if (std::memcmp(comment + j, CLANG_MARKER, 14) == 0) return "clang";
        }
        break;
    }
    return "gcc";
}

// Magic bytes
static const uint8_t PE_MAGIC[] = { 0x4D, 0x5A };  // "MZ"
static const uint8_t ELF_MAGIC[] = { 0x7F, 0x45, 0x4C, 0x46 };  // "\x7FELF"
static const uint32_t MACHO_MAGIC_32 = 0xFEEDFACE;
static const uint32_t MACHO_MAGIC_64 = 0xFEEDFACF;
static const uint32_t MACHO_CIGAM_32 = 0xCEFAEDFE;  // Byte-swapped
static const uint32_t MACHO_CIGAM_64 = 0xCFFAEDFE;
// D-1: Universal / Fat Binary (fields are big-endian in file)
static const uint32_t FAT_CIGAM = 0xBEBAFECA;  // 0xCAFEBABE read on LE system

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
    // D-1: Also recognise Universal (Fat) binaries
    return magic == MACHO_MAGIC_32 || magic == MACHO_MAGIC_64 ||
           magic == MACHO_CIGAM_32 || magic == MACHO_CIGAM_64 ||
           magic == FAT_CIGAM;
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
            info.sleigh_id = get_sleigh_id(info.format, info.arch);
            break;
        case 0x8664:  // IMAGE_FILE_MACHINE_AMD64
            info.arch = ArchType::X86_64;
            info.is_64bit = true;
            info.sleigh_id = get_sleigh_id(info.format, info.arch);
            break;
        case 0xAA64:  // IMAGE_FILE_MACHINE_ARM64
            info.arch = ArchType::ARM64;
            info.is_64bit = true;
            info.sleigh_id = get_sleigh_id(info.format, info.arch);
            break;
        case 0x01C0:  // IMAGE_FILE_MACHINE_ARM
        case 0x01C4:  // IMAGE_FILE_MACHINE_ARMNT
            info.arch = ArchType::ARM;
            info.is_64bit = false;
            info.sleigh_id = get_sleigh_id(info.format, info.arch);
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
    
    // C-1: Parse PE section table (IMAGE_SECTION_HEADER array).
    // NumberOfSections at PE+6; SizeOfOptionalHeader at PE+20.
    // Section table begins immediately after the Optional Header.
    {
        uint16_t num_sections   = *(const uint16_t*)(data + pe_offset + 6);
        uint16_t opt_hdr_size   = *(const uint16_t*)(data + pe_offset + 20);
        size_t   sec_table_base = pe_offset + 24 + (size_t)opt_hdr_size;

        for (uint16_t i = 0; i < num_sections; ++i) {
            size_t sec_off = sec_table_base + (size_t)i * 40;
            if (sec_off + 40 > size) break;

            SectionInfo sec;
            char raw_name[9] = {};
            memcpy(raw_name, data + sec_off, 8);
            sec.name      = raw_name;                                     // e.g. ".text\0\0\0"
            sec.virtual_size   = *(const uint32_t*)(data + sec_off + 8);      // VirtualSize
            uint32_t rva  = *(const uint32_t*)(data + sec_off + 12);     // VirtualAddress (RVA)
            sec.virtual_address   = info.image_base + rva;
            uint32_t raw_data_size   = *(const uint32_t*)(data + sec_off + 16); // SizeOfRawData
            uint32_t raw_data_offset = *(const uint32_t*)(data + sec_off + 20); // PointerToRawData
            sec.file_offset = raw_data_offset;
            sec.file_size   = raw_data_size;
            if (sec.virtual_size == 0)
                sec.virtual_size = raw_data_size;                              // SizeOfRawData fallback
            uint32_t ch   = *(const uint32_t*)(data + sec_off + 36);     // Characteristics
            sec.is_executable = ((ch & 0x20000000u) != 0)                // IMAGE_SCN_MEM_EXECUTE
                             || ((ch & 0x00000020u) != 0);               // IMAGE_SCN_CNT_CODE
            if (!sec.name.empty() && sec.virtual_size > 0)
                info.sections.push_back(std::move(sec));
        }
    }

    fission::utils::log_stream() << "[BinaryDetector] PE: " << (info.is_64bit ? "64-bit" : "32-bit")
              << " Arch=" << info.sleigh_id << std::endl;
    
    return info;
}

BinaryInfo BinaryDetector::parse_elf(const uint8_t* data, size_t size) {
    BinaryInfo info;
    info.format = BinaryFormat::ELF;
    // A-4: Detect GCC vs Clang by scanning the .comment ELF section.
    // "clang version X.Y.Z" appears there for Clang-compiled binaries.
    info.compiler_id = elf_detect_compiler(data, size);

    if (size < 64) return info;
    
    // ELF class (32/64 bit) at offset 4
    uint8_t elf_class = data[4];
    info.is_64bit = (elf_class == 2);  // ELFCLASS64
    
    // Machine type at offset 18 (for both 32 and 64)
    uint16_t machine = *(const uint16_t*)(data + 18);
    
    switch (machine) {
        case 0x03:  // EM_386
            info.arch = ArchType::X86;
            info.sleigh_id = get_sleigh_id(info.format, info.arch);
            break;
        case 0x3E:  // EM_X86_64
            info.arch = ArchType::X86_64;
            info.sleigh_id = get_sleigh_id(info.format, info.arch);
            break;
        case 0x28:  // EM_ARM
            info.arch = ArchType::ARM;
            info.sleigh_id = get_sleigh_id(info.format, info.arch);
            break;
        case 0xB7:  // EM_AARCH64
            info.arch = ArchType::ARM64;
            info.sleigh_id = get_sleigh_id(info.format, info.arch);
            break;
        default:
            info.arch = ArchType::UNKNOWN;
            info.sleigh_id = get_sleigh_id(info.format, info.arch);  // fallback
            break;
    }
    
    // Get entry point
    if (info.is_64bit) {
        info.entry_point = *(const uint64_t*)(data + 24);
    } else {
        info.entry_point = *(const uint32_t*)(data + 24);
    }
    
    // C-1: Parse ELF section headers.
    // We replicate the read helpers here so this function remains self-contained.
    {
        if (size >= 64) {
            bool isle = (data[5] == 1); // ELFDATA2LSB
            auto rd16e = [&](size_t off) -> uint16_t {
                if (off + 2 > size) return 0;
                return isle ? (uint16_t)(data[off] | (data[off+1] << 8))
                            : (uint16_t)((data[off] << 8) | data[off+1]);
            };
            auto rd32e = [&](size_t off) -> uint32_t {
                if (off + 4 > size) return 0;
                if (isle) return (uint32_t)(data[off]|(data[off+1]<<8)|(data[off+2]<<16)|(data[off+3]<<24));
                return (uint32_t)((data[off]<<24)|(data[off+1]<<16)|(data[off+2]<<8)|data[off+3]);
            };
            auto rd64e = [&](size_t off) -> uint64_t {
                if (off + 8 > size) return 0;
                uint64_t lo = rd32e(off), hi = rd32e(off + 4);
                return isle ? lo | (hi << 32) : (lo << 32) | hi;
            };
            auto rdptr = [&](size_t off) -> uint64_t {
                return info.is_64bit ? rd64e(off) : (uint64_t)rd32e(off);
            };

            uint64_t shoff     = info.is_64bit ? rd64e(40)  : (uint64_t)rd32e(32);
            uint16_t shentsize = info.is_64bit ? rd16e(58)  : rd16e(46);
            uint16_t shnum     = info.is_64bit ? rd16e(60)  : rd16e(48);
            uint16_t shstrndx  = info.is_64bit ? rd16e(62)  : rd16e(50);

            if (shoff > 0 && shentsize > 0 && shnum > 0 && shstrndx < shnum &&
                shoff + (uint64_t)shnum * shentsize <= size) {

                size_t   strtab_shdr  = (size_t)(shoff + (uint64_t)shstrndx * shentsize);
                uint64_t sh_name_off  = info.is_64bit ? rd64e(strtab_shdr + 24) : (uint64_t)rd32e(strtab_shdr + 16);
                uint64_t sh_name_size = info.is_64bit ? rd64e(strtab_shdr + 32) : (uint64_t)rd32e(strtab_shdr + 20);

                for (uint16_t i = 0; i < shnum; ++i) {
                    size_t   shdr       = (size_t)(shoff + (uint64_t)i * shentsize);
                    uint32_t sh_name    = rd32e(shdr);
                    // ELF64: sh_flags@8(8), sh_addr@16(8), sh_size@32(8)
                    // ELF32: sh_flags@8(4), sh_addr@12(4), sh_size@20(4)
                    uint64_t sh_flags   = rdptr(shdr + 8);
                    uint64_t sh_addr    = rdptr(shdr + (info.is_64bit ? 16 : 12));
                    // ELF64: sh_offset@24(8), sh_size@32(8)
                    // ELF32: sh_offset@16(4), sh_size@20(4)
                    uint64_t sh_offset  = rdptr(shdr + (info.is_64bit ? 24 : 16));
                    uint64_t sh_size    = rdptr(shdr + (info.is_64bit ? 32 : 20));

                    uint64_t name_pos = sh_name_off + sh_name;
                    if (name_pos >= size || sh_size == 0) continue;

                    // Bound the name string
                    size_t name_max = (size_t)std::min((uint64_t)(size - name_pos), (uint64_t)64);
                    const char* raw = (const char*)(data + name_pos);
                    SectionInfo sec;
                    sec.name          = std::string(raw, strnlen(raw, name_max));
                    sec.virtual_address       = sh_addr;
                    sec.virtual_size       = sh_size;
                    sec.file_offset   = sh_offset;
                    sec.file_size     = sh_size;
                    sec.is_executable = (sh_flags & 0x4u) != 0; // SHF_EXECINSTR
                    if (!sec.name.empty())
                        info.sections.push_back(std::move(sec));
                }
            }
        }
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

    // -----------------------------------------------------------------------
    // D-1: Handle Universal (Fat) Binary
    // fat_header fields are big-endian regardless of host architecture.
    // On LE systems, 0xCAFEBABE is read as 0xBEBAFECA (FAT_CIGAM).
    // -----------------------------------------------------------------------
    if (magic == FAT_CIGAM) {
        // fat_header: { uint32 magic; uint32 nfat_arch; }
        // fat_arch:   { int32 cputype; int32 cpusubtype; uint32 offset; uint32 size; uint32 align; }
        // All fields are big-endian in file.
        if (size < 8) return info;

#ifdef _MSC_VER
#define BSWAP32(x) _byteswap_ulong(x)
#else
#define BSWAP32(x) __builtin_bswap32(x)
#endif
        uint32_t nfat_arch = BSWAP32(*(const uint32_t*)(data + 4));

        const uint32_t CPU_TYPE_ARM64_BE = BSWAP32(0x0100000C); // as stored in file
        const uint32_t CPU_TYPE_X86_64_BE = BSWAP32(0x01000007);

        // Prefer ARM64 slice; fall back to x86_64
        uint32_t chosen_offset = 0, chosen_size = 0;
        uint32_t x86_64_offset = 0, x86_64_size = 0;

        for (uint32_t i = 0; i < nfat_arch && i < 16; ++i) {
            size_t arch_off = 8 + (size_t)i * 20;
            if (arch_off + 20 > size) break;

            uint32_t cputype_raw = *(const uint32_t*)(data + arch_off);
            uint32_t slice_offset = BSWAP32(*(const uint32_t*)(data + arch_off + 8));
            uint32_t slice_size   = BSWAP32(*(const uint32_t*)(data + arch_off + 12));

            if (cputype_raw == CPU_TYPE_ARM64_BE) {
                chosen_offset = slice_offset;
                chosen_size   = slice_size;
                break; // ARM64 preferred
            } else if (cputype_raw == CPU_TYPE_X86_64_BE) {
                x86_64_offset = slice_offset;
                x86_64_size   = slice_size;
            }
        }

        if (chosen_offset == 0) {
            // No ARM64; try x86_64
            chosen_offset = x86_64_offset;
            chosen_size   = x86_64_size;
        }
#undef BSWAP32

        if (chosen_offset != 0 && chosen_offset + chosen_size <= size) {
            // Recurse on the selected slice
            return parse_macho(data + chosen_offset, chosen_size);
        }
        // Fallback: parse the file header as-is (shouldn't happen)
        return info;
    }

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
    const uint32_t CPU_TYPE_X86   = 0x7;
    const uint32_t CPU_TYPE_X86_64 = 0x01000007;
    const uint32_t CPU_TYPE_ARM    = 0xC;
    const uint32_t CPU_TYPE_ARM64  = 0x0100000C;

    switch (cputype) {
        case CPU_TYPE_X86:
            info.arch = ArchType::X86;
            info.sleigh_id = get_sleigh_id(info.format, info.arch);
            break;
        case CPU_TYPE_X86_64:
            info.arch = ArchType::X86_64;
            info.sleigh_id = get_sleigh_id(info.format, info.arch);
            break;
        case CPU_TYPE_ARM:
            info.arch = ArchType::ARM;
            info.sleigh_id = get_sleigh_id(info.format, info.arch);
            break;
        case CPU_TYPE_ARM64:
            info.arch = ArchType::ARM64;
            info.sleigh_id = get_sleigh_id(info.format, info.arch);
            info.is_64bit = true;
            // D-3: encode arch in compiler_id so PathConfig can select correct FID
            info.compiler_id = "clang-aarch64";
            break;
        default:
            info.arch = ArchType::UNKNOWN;
            // Mach-O unknown arch: modern Macs are predominantly ARM64
            info.sleigh_id = get_sleigh_id(info.format, ArchType::ARM64);  // fallback for modern Macs
            break;
    }

    // -----------------------------------------------------------------------
    // D-2: Parse load commands to find __TEXT vmaddr (= image_base)
    // Mach-O 64-bit header size = 32 bytes; 32-bit = 28 bytes
    // LC_SEGMENT_64 (cmd=0x19) / LC_SEGMENT (cmd=0x1) contains vmaddr
    // -----------------------------------------------------------------------
    const uint32_t LC_SEGMENT    = 0x1;
    const uint32_t LC_SEGMENT_64 = 0x19;

    size_t hdr_size = info.is_64bit ? 32 : 28;
    if (size <= hdr_size + 8) {
        fission::utils::log_stream() << "[BinaryDetector] Mach-O: " << (info.is_64bit ? "64-bit" : "32-bit")
                  << " Arch=" << info.sleigh_id << std::endl;
        return info;
    }

    uint32_t ncmds      = *(const uint32_t*)(data + 16);
    uint32_t sizeofcmds = *(const uint32_t*)(data + 20);
    if (is_big_endian) {
#ifdef _MSC_VER
        ncmds      = _byteswap_ulong(ncmds);
        sizeofcmds = _byteswap_ulong(sizeofcmds);
#else
        ncmds      = __builtin_bswap32(ncmds);
        sizeofcmds = __builtin_bswap32(sizeofcmds);
#endif
    }

    size_t lc_start = hdr_size;
    size_t lc_end   = lc_start + sizeofcmds;
    if (lc_end > size) lc_end = size;

    size_t lc_off = lc_start;
    for (uint32_t ci = 0; ci < ncmds && lc_off + 8 <= lc_end; ++ci) {
        uint32_t cmd     = *(const uint32_t*)(data + lc_off);
        uint32_t cmdsize = *(const uint32_t*)(data + lc_off + 4);
        if (is_big_endian) {
#ifdef _MSC_VER
            cmd     = _byteswap_ulong(cmd);
            cmdsize = _byteswap_ulong(cmdsize);
#else
            cmd     = __builtin_bswap32(cmd);
            cmdsize = __builtin_bswap32(cmdsize);
#endif
        }
        if (cmdsize < 8 || lc_off + cmdsize > lc_end) break;

        if (cmd == LC_SEGMENT_64 && lc_off + 64 <= lc_end) {
            // segment_command_64: cmd(4) cmdsize(4) segname[16] vmaddr(8) vmsize(8) fileoff(8) filesize(8) ...
            const char* segname = (const char*)(data + lc_off + 8);
            uint64_t vmaddr = *(const uint64_t*)(data + lc_off + 24);
            uint64_t vmsize = *(const uint64_t*)(data + lc_off + 32);
            uint64_t fileoff  = *(const uint64_t*)(data + lc_off + 40);
            uint64_t filesize = *(const uint64_t*)(data + lc_off + 48);
            if (is_big_endian) {
#ifdef _MSC_VER
                vmaddr = _byteswap_uint64(vmaddr);
                vmsize = _byteswap_uint64(vmsize);
                fileoff  = _byteswap_uint64(fileoff);
                filesize = _byteswap_uint64(filesize);
#else
                vmaddr = __builtin_bswap64(vmaddr);
                vmsize = __builtin_bswap64(vmsize);
                fileoff  = __builtin_bswap64(fileoff);
                filesize = __builtin_bswap64(filesize);
#endif
            }
            // Record image_base from __TEXT
            if (std::strncmp(segname, "__TEXT", 6) == 0)
                info.image_base = vmaddr;
            // C-1: Record section info for every segment
            if (vmsize > 0) {
                SectionInfo sec;
                char raw[17] = {};
                memcpy(raw, segname, 16);
                sec.name          = raw;
                sec.virtual_address       = vmaddr;
                sec.virtual_size       = vmsize;
                sec.file_offset   = fileoff;
                sec.file_size     = filesize;
                sec.is_executable = (std::strncmp(segname, "__TEXT", 6) == 0);
                info.sections.push_back(std::move(sec));
            }
        } else if (cmd == LC_SEGMENT && lc_off + 56 <= lc_end) {
            // segment_command (32-bit): cmd(4) cmdsize(4) segname[16] vmaddr(4) vmsize(4) fileoff(4) filesize(4) ...
            const char* segname = (const char*)(data + lc_off + 8);
            uint32_t vmaddr32  = *(const uint32_t*)(data + lc_off + 24);
            uint32_t vmsize32  = *(const uint32_t*)(data + lc_off + 28);
            uint32_t fileoff32  = *(const uint32_t*)(data + lc_off + 32);
            uint32_t filesize32 = *(const uint32_t*)(data + lc_off + 36);
            if (is_big_endian) {
#ifdef _MSC_VER
                vmaddr32  = _byteswap_ulong(vmaddr32);
                vmsize32  = _byteswap_ulong(vmsize32);
                fileoff32  = _byteswap_ulong(fileoff32);
                filesize32 = _byteswap_ulong(filesize32);
#else
                vmaddr32  = __builtin_bswap32(vmaddr32);
                vmsize32  = __builtin_bswap32(vmsize32);
                fileoff32  = __builtin_bswap32(fileoff32);
                filesize32 = __builtin_bswap32(filesize32);
#endif
            }
            if (std::strncmp(segname, "__TEXT", 6) == 0)
                info.image_base = (uint64_t)vmaddr32;
            // C-1: Record section info for every segment
            if (vmsize32 > 0) {
                SectionInfo sec;
                char raw[17] = {};
                memcpy(raw, segname, 16);
                sec.name          = raw;
                sec.virtual_address       = (uint64_t)vmaddr32;
                sec.virtual_size       = (uint64_t)vmsize32;
                sec.file_offset   = (uint64_t)fileoff32;
                sec.file_size     = (uint64_t)filesize32;
                sec.is_executable = (std::strncmp(segname, "__TEXT", 6) == 0);
                info.sections.push_back(std::move(sec));
            }
        }

        lc_off += cmdsize;
    }

    fission::utils::log_stream() << "[BinaryDetector] Mach-O: " << (info.is_64bit ? "64-bit" : "32-bit")
              << " Arch=" << info.sleigh_id
              << " ImageBase=0x" << std::hex << info.image_base << std::dec << std::endl;
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
            // Mach-O ARM64 uses AppleSilicon variant (matches AARCH64.opinion)
            // ELF/PE use generic v8A
            if (format == BinaryFormat::MACHO) {
                return "AARCH64:LE:64:AppleSilicon";
            }
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
