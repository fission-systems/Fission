/**
 * Fission Path Configuration Implementation
 */

#include "fission/config/PathConfig.h"
#include "fission/utils/file_utils.h"

namespace fission {
namespace config {

using fission::utils::file_exists;

// ============================================================================
// Path Constants
// ============================================================================

static const std::vector<std::string> FID_SEARCH_DIRS = {
    "./utils/signatures/fid/",
    "../utils/signatures/fid/",
    "../../utils/signatures/fid/"
};

// Must match TypePropagator logic
// MSVC FID database filenames by version (highest priority)
static const std::vector<std::string> MSVC_FID_FILES_X64 = {
    "vs2019_x64.fidbf", "vs2017_x64.fidbf", "vs2015_x64.fidbf", 
    "vs2012_x64.fidbf", "vsOlder_x64.fidbf"
};
static const std::vector<std::string> MSVC_FID_FILES_X86 = {
    "vs2019_x86.fidbf", "vs2017_x86.fidbf", "vs2015_x86.fidbf", 
    "vs2012_x86.fidbf", "vsOlder_x86.fidbf"
};

// GCC/MinGW FID database patterns
static const std::vector<std::string> GCC_FID_FILES_X64 = {
    "gcc-x86.LE.64.default.fidbf", "gcc-AARCH64.LE.64.v8A.fidbf"
};
static const std::vector<std::string> GCC_FID_FILES_X86 = {
    "gcc-x86.LE.32.default.fidbf", "gcc-ARM.LE.32.v8.fidbf"
};

// libc FID database patterns
static const std::vector<std::string> LIBC_FID_FILES_X64 = {
    "libc-x86.LE.64.default.fidbf", "libc-AARCH64.LE.64.v8A.fidbf"
};
static const std::vector<std::string> LIBC_FID_FILES_X86 = {
    "libc-x86.LE.32.default.fidbf", "libc-ARM.LE.32.v8.fidbf"
};

// Crypto library FID databases (OpenSSL, libsodium)
static const std::vector<std::string> CRYPTO_FID_FILES_X64 = {
    "sigmoid-openssl-1.1.0f-x86.LE.64.default.fidbf",
    "sigmoid-openssl-1.0.2l-x86.LE.64.default.fidbf",
    "sigmoid-openssl-1.0.1u-x86.LE.64.default.fidbf",
    "libsodium-x86.LE.64.default.fidbf"
};
static const std::vector<std::string> CRYPTO_FID_FILES_X86 = {
    "sigmoid-openssl-1.1.0f-x86.LE.32.default.fidbf",
    "sigmoid-openssl-1.0.2l-x86.LE.32.default.fidbf",
    "sigmoid-openssl-1.0.1u-x86.LE.32.default.fidbf",
    "libsodium-x86.LE.32.default.fidbf"
};

// Enterprise Linux FID databases
static const std::vector<std::string> EL_FID_FILES_X64 = {
    "el7.x86_64.fidbf", "el6.x86_64.fidbf"
};
static const std::vector<std::string> EL_FID_FILES_X86 = {
    "el7.i686.fidbf", "el6.i686.fidbf"
};


// GDT Search Paths
static const std::vector<std::string> GDT_SEARCH_PREFIXES = {
    "../../utils/signatures/typeinfo/win32/",
    "../utils/signatures/typeinfo/win32/",
    "./utils/signatures/typeinfo/win32/",
    "utils/signatures/typeinfo/win32/"
};

// ============================================================================
// Implementation
// ============================================================================

std::string get_fid_filename(bool is_64bit, const std::string& compiler_id) {
    std::string suffix = is_64bit ? "_x64.fidbf" : "_x86.fidbf";
    std::string fid_filename = "vs2019" + suffix; // Default

    // Logic extracted from TypePropagator.cc
    if (compiler_id.find("vs2017") != std::string::npos) 
        fid_filename = "vs2017" + suffix;
    else if (compiler_id.find("vs2015") != std::string::npos) 
        fid_filename = "vs2015" + suffix;
    else if (compiler_id.find("vs2012") != std::string::npos) 
        fid_filename = "vs2012" + suffix;
    else if (compiler_id.find("gcc") != std::string::npos || compiler_id.find("clang") != std::string::npos) {
        // D-3: distinguish x86_64 vs AARCH64 based on compiler_id
        // BinaryDetector sets compiler_id = "clang-aarch64" for ARM64 Mach-O.
        bool is_aarch64 = (compiler_id.find("aarch64") != std::string::npos ||
                           compiler_id.find("arm64")   != std::string::npos);
        if (is_aarch64) {
            return "gcc-AARCH64.LE.64.v8A.fidbf"; // AARCH64 FID (shared for gcc/clang)
        }
        if (is_64bit && !GCC_FID_FILES_X64.empty()) return GCC_FID_FILES_X64[0];
        if (!is_64bit && !GCC_FID_FILES_X86.empty()) return GCC_FID_FILES_X86[0];
    }
    
    return fid_filename;
}

std::string find_fid_file(const std::string& filename) {
    if (filename.empty()) return "";
    
    for (const auto& dir : FID_SEARCH_DIRS) {
        std::string path = dir + filename;
        if (file_exists(path)) {
            return path;
        }
    }
    return "";
}

std::vector<std::string> get_all_fid_paths(bool is_64bit) {
    std::vector<std::string> result;
    
    // Collect all file lists for this architecture
    std::vector<const std::vector<std::string>*> all_lists;
    if (is_64bit) {
        all_lists = {&MSVC_FID_FILES_X64, &GCC_FID_FILES_X64, &LIBC_FID_FILES_X64, 
                     &CRYPTO_FID_FILES_X64, &EL_FID_FILES_X64};
    } else {
        all_lists = {&MSVC_FID_FILES_X86, &GCC_FID_FILES_X86, &LIBC_FID_FILES_X86,
                     &CRYPTO_FID_FILES_X86, &EL_FID_FILES_X86};
    }

    // Try to find each file in search directories
    for (const auto* list : all_lists) {
        for (const auto& filename : *list) {
            std::string path = find_fid_file(filename);
            if (!path.empty()) {
                result.push_back(path);
            }
        }
    }
    
    return result;
}

std::vector<std::string> get_gdt_candidates(bool is_64bit) {
    std::string filename = is_64bit ? "windows_vs12_64.gdt" : "windows_vs12_32.gdt";
    std::vector<std::string> candidates;
    
    for (const auto& prefix : GDT_SEARCH_PREFIXES) {
        candidates.push_back(prefix + filename);
    }
    return candidates;
}

std::string find_gdt_file(const std::string& filename) {
     for (const auto& prefix : GDT_SEARCH_PREFIXES) {
        std::string path = prefix + filename;
        if (file_exists(path)) {
            return path;
        }
    }
    return "";
}

std::vector<std::string> get_common_symbol_files() {
    return {
        "./utils/signatures/fid/common_symbols_win32.txt",
        "./utils/signatures/fid/common_symbols_win64.txt",
        "../utils/signatures/fid/common_symbols_win32.txt",
        "../utils/signatures/fid/common_symbols_win64.txt",
        "../../utils/signatures/fid/common_symbols_win32.txt",
        "../../utils/signatures/fid/common_symbols_win64.txt"
    };
}

std::vector<std::string> get_guid_files() {
    return {
        "../../utils/signatures/typeinfo/win32/msvcrt/guids.txt",
        "../utils/signatures/typeinfo/win32/msvcrt/guids.txt",
        "./utils/signatures/typeinfo/win32/msvcrt/guids.txt",
        "utils/signatures/typeinfo/win32/msvcrt/guids.txt",
        "../../utils/signatures/typeinfo/win32/msvcrt/iids.txt",
        "../utils/signatures/typeinfo/win32/msvcrt/iids.txt",
        "./utils/signatures/typeinfo/win32/msvcrt/iids.txt",
        "utils/signatures/typeinfo/win32/msvcrt/iids.txt"
    };
}

} // namespace config
} // namespace fission
