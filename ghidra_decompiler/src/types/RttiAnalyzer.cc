#include "fission/types/RttiAnalyzer.h"
#include <cstring>
#include <iostream>
#include <cstdlib>
#include <cctype>
#include "fission/utils/logger.h"  // E-2: use log_stream() instead of std::cerr

// Itanium ABI demangling (available on GCC/Clang)
#if defined(__GNUC__) || defined(__clang__)
#include <cxxabi.h>
#endif

namespace fission {
namespace types {

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

static inline uint32_t read_u32(const uint8_t* data, size_t off) {
    uint32_t v = 0;
    std::memcpy(&v, data + off, 4);
    return v;
}

static inline uint64_t read_u64(const uint8_t* data, size_t off) {
    uint64_t v = 0;
    std::memcpy(&v, data + off, 8);
    return v;
}

static inline uint64_t read_ptr(const uint8_t* data, size_t off, bool is_64bit) {
    return is_64bit ? read_u64(data, off) : (uint64_t)read_u32(data, off);
}

// Best-effort Itanium demangling; returns mangled name on failure.
static std::string itanium_demangle(const std::string& mangled) {
#if defined(__GNUC__) || defined(__clang__)
    int status = 0;
    char* d = abi::__cxa_demangle(mangled.c_str(), nullptr, nullptr, &status);
    if (d && status == 0) {
        std::string result(d);
        std::free(d);
        // Strip "typeinfo for " prefix added by demangler
        const std::string prefix = "typeinfo for ";
        if (result.rfind(prefix, 0) == 0)
            result = result.substr(prefix.size());
        return result;
    }
    if (d) std::free(d);
#endif
    // Fallback: strip leading length-encoded number (Itanium simple names)
    std::string s = mangled;
    size_t p = 0;
    while (p < s.size() && std::isdigit((unsigned char)s[p])) ++p;
    if (p > 0 && p < s.size()) return s.substr(p);
    return s;
}

// ---------------------------------------------------------------------------
// B-1: MSVC CompleteObjectLocator-based vtable discovery
// ---------------------------------------------------------------------------
// MSVC RTTI layout for x64 PE:
//   TypeDescriptor:  { void* pVFTable[8], void* spare[8], char name[] }
//   CompleteObjectLocator (COL): {
//       uint32 signature   (1 = x64, 0 = x86)
//       uint32 offset      (vtable offset in object)
//       uint32 cdOffset
//       uint32 pRtti0      (image-base-relative offset to TypeDescriptor)  x64
//              uint32 pRtti0 (absolute VA to TypeDescriptor)               x86
//       uint32 pRtti3      (image-base-relative offset to ClassHierarchy)
//       [x64] uint32 objectBaseOffset
//   }
//   vtable[-ptr_size] = address of COL  -> vtable starts right after that slot
//
// Algorithm (from Ghidra Rtti4Model.java):
//   1. Scan binary for ".?AV" to locate TypeDescriptors -> build td_offset_map
//   2. Scan for COL structures whose pRtti0 references a known TypeDescriptor
//   3. For each COL found, scan binary for a pointer-sized slot containing
//      col_va -> that slot is vtable[-1] -> vtable = slot_va + ptr_size
// ---------------------------------------------------------------------------
static void scan_msvc_rtti(
    const std::vector<uint8_t>& bytes,
    uint64_t image_base,
    bool is_64bit,
    std::map<uint64_t, std::string>& result)
{
    const size_t sz = bytes.size();
    const uint8_t* data = bytes.data();
    const int ptr_size = is_64bit ? 8 : 4;
    // TypeDescriptor name field starts after pVFTable + spare
    const size_t td_name_offset = (size_t)(ptr_size * 2);

    // Step 1: build TypeDescriptor file-offset -> class_name map
    // key = file offset of TypeDescriptor start (i.e. name_pos - td_name_offset)
    std::map<size_t, std::string> td_map;
    const char* msvc_sig = ".?AV";

    for (size_t i = td_name_offset; i + 4 < sz; ++i) {
        if (std::memcmp(data + i, msvc_sig, 4) != 0) continue;

        // TypeDescriptor begins at i - td_name_offset
        size_t td_start = i - td_name_offset;

        // Extract raw name starting at i
        std::string raw;
        for (size_t p = i; p < sz && data[p] != 0; ++p)
            raw += (char)data[p];

        // ".?AVFoo@@" or ".?AVFoo@Bar@@"  -> "Foo" or "Foo::Bar" (simplified)
        if (raw.size() <= 4) continue;
        std::string name = raw.substr(4); // drop ".?AV"
        // Remove trailing "@@"
        if (name.size() > 2 && name.substr(name.size() - 2) == "@@")
            name = name.substr(0, name.size() - 2);
        // Replace remaining '@' separators with "::" (namespace)
        for (char& c : name)
            if (c == '@') c = ':';
        // Deduplicate "::" from consecutive '@'
        std::string clean;
        clean.reserve(name.size());
        bool last_colon = false;
        for (size_t k = 0; k < name.size(); ++k) {
            if (name[k] == ':' && k + 1 < name.size() && name[k+1] == ':') {
                if (!last_colon) { clean += "::"; last_colon = true; }
                ++k;
            } else {
                clean += name[k];
                last_colon = false;
            }
        }

        td_map[td_start] = clean.empty() ? name : clean;
    }

    if (td_map.empty()) return;

    // Step 2: scan for CompleteObjectLocator entries
    // COL size: x64 = 24 bytes, x86 = 20 bytes
    struct ColHit { uint64_t col_va; std::string class_name; };
    std::vector<ColHit> col_hits;

    for (size_t i = 0; i + 24 <= sz; i += 4) {
        uint32_t sig_field = read_u32(data, i);
        if (is_64bit  && sig_field != 1) continue;
        if (!is_64bit && sig_field != 0) continue;

        uint32_t rtti0_field = read_u32(data, i + 12);
        if (rtti0_field == 0) continue;

        size_t td_file_off;
        if (is_64bit) {
            // pRtti0 is image-base-relative
            if ((uint64_t)rtti0_field >= sz) continue;
            td_file_off = (size_t)rtti0_field;
        } else {
            // pRtti0 is absolute VA
            if ((uint64_t)rtti0_field < image_base) continue;
            td_file_off = (size_t)((uint64_t)rtti0_field - image_base);
        }
        if (td_file_off >= sz) continue;

        auto it = td_map.find(td_file_off);
        if (it == td_map.end()) continue;

        col_hits.push_back({ image_base + i, it->second });
    }

    // Step 3: for each COL, find vtable by scanning for a pointer to col_va
    for (const auto& hit : col_hits) {
        uint64_t col_va = hit.col_va;
        for (size_t j = 0; j + ptr_size <= sz; j += (size_t)ptr_size) {
            uint64_t stored = read_ptr(data, j, is_64bit);
            if (stored != col_va) continue;
            // vtable starts at j + ptr_size
            uint64_t vtable_va = image_base + j + (size_t)ptr_size;
            if (result.find(vtable_va) == result.end())
                result[vtable_va] = hit.class_name;
        }
    }
}

// ---------------------------------------------------------------------------
// B-4: Itanium ABI RTTI (_ZTI* / __ZTI*) vtable discovery
// ---------------------------------------------------------------------------
// Itanium vtable layout:
//   vtable[-2*ptr_size] = offset-to-top   (usually 0 for primary)
//   vtable[-1*ptr_size] = pointer to type_info object
//   vtable[0 .. N]      = virtual function pointers
//
// type_info object = { void* vtable_for_type_info; const char* name; ... }
//   where name points to a _ZTS<mangled> ("typeinfo name for <class>") string.
//
// Algorithm:
//   1. Scan binary data for "_ZTS" / "__ZTS" null-terminated strings
//   2. For each string, scan for pointers to it -> candidate type_info.name slot
//   3. type_info object VA = pointer_location - ptr_size
//   4. Scan for pointers to type_info VA -> vtable[-1] slot
//   5. Verify vtable[-2] == 0 (offset-to-top), then vtable_va = slot_va + ptr_size
// ---------------------------------------------------------------------------
static void scan_itanium_rtti(
    const std::vector<uint8_t>& bytes,
    uint64_t image_base,
    bool is_64bit,
    std::map<uint64_t, std::string>& result)
{
    const size_t sz = bytes.size();
    const uint8_t* data = bytes.data();
    const int ptr_size = is_64bit ? 8 : 4;

    // Step 1+2: find _ZTS/_ZTS name strings and candidate type_info VAs
    struct TICandidate { uint64_t ti_va; std::string class_name; };
    std::vector<TICandidate> ti_list;

    // Both Linux (_ZTS) and macOS (__ZTS) prefixes
    const char* zts_prefixes[] = { "_ZTS", "__ZTS", nullptr };
    for (int pi = 0; zts_prefixes[pi]; ++pi) {
        const char* prefix = zts_prefixes[pi];
        size_t prefix_len = std::strlen(prefix);

        for (size_t i = 0; i + prefix_len + 1 < sz; ++i) {
            if (std::memcmp(data + i, prefix, prefix_len) != 0) continue;

            // Build _ZTI mangled form for demangling
            std::string suffix((const char*)(data + i + prefix_len));
            if (suffix.empty()) continue;

            // "_ZTI" + suffix for demangling
            std::string zti_mangled = std::string("_ZTI") + suffix;
            std::string class_name  = itanium_demangle(zti_mangled);

            uint64_t zts_va = image_base + i;

            // Scan for pointers-to-zts_va (this is type_info.name slot)
            for (size_t j = ptr_size; j + ptr_size <= sz; j += (size_t)ptr_size) {
                uint64_t stored = read_ptr(data, j, is_64bit);
                if (stored != zts_va) continue;
                // type_info object VA = j - ptr_size  (vtable-for-type_info pointer)
                uint64_t ti_va = image_base + j - (size_t)ptr_size;
                ti_list.push_back({ ti_va, class_name });
            }
        }
    }

    // Step 3+4: for each type_info, find vtables that reference it
    for (const auto& ti : ti_list) {
        uint64_t ti_va = ti.ti_va;
        for (size_t j = ptr_size; j + ptr_size <= sz; j += (size_t)ptr_size) {
            uint64_t stored = read_ptr(data, j, is_64bit);
            if (stored != ti_va) continue;

            // Verify vtable[-2] == 0 (offset-to-top for primary vtable)
            if (j >= (size_t)(ptr_size * 2)) {
                uint64_t ott = read_ptr(data, j - (size_t)ptr_size, is_64bit);
                if (ott != 0) continue; // not a primary vtable slot
            }

            uint64_t vtable_va = image_base + j + (size_t)ptr_size;
            if (result.find(vtable_va) == result.end())
                result[vtable_va] = ti.class_name;
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

std::map<uint64_t, std::string> RttiAnalyzer::recover_class_names(
    const std::vector<uint8_t>& bytes,
    uint64_t image_base,
    bool is_64bit)
{
    std::map<uint64_t, std::string> result;

    // B-1: MSVC CompleteObjectLocator chain (PE x86/x64)
    scan_msvc_rtti(bytes, image_base, is_64bit, result);

    // B-4: Itanium ABI type_info chain (ELF / Mach-O)
    scan_itanium_rtti(bytes, image_base, is_64bit, result);

    // E-2: Use log_stream() consistent with the rest of the codebase.
    fission::utils::log_stream() << "[RttiAnalyzer] Recovered " << result.size()
              << " vtable->class_name mappings" << std::endl;
    return result;
}

} // namespace types
} // namespace fission
