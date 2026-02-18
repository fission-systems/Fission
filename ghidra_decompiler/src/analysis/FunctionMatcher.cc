#include "fission/analysis/FunctionMatcher.h"
#include "fission/utils/logger.h"
#include "fission/utils/json_utils.h"
#include <iostream>
#include <fstream>
#include <sstream>
#include <cstring>

namespace fission {
namespace analysis {

FunctionMatcher::FunctionMatcher() {
}

FunctionMatcher::~FunctionMatcher() {
}

void FunctionMatcher::load_builtin_msvc_x64() {
    // Common MSVC x64 CRT function prologues
    // These are simplified patterns for demonstration
    
    // malloc - typical pattern
    {
        FunctionSignature sig;
        sig.name = "malloc";
        sig.library = "ucrtbase";
        // sub rsp, XX; mov r8d, [rsp+XX] or similar
        sig.pattern = {0x48, 0x83, 0xEC};  // sub rsp, imm8
        sig.mask = {0xFF, 0xFF, 0xFF};
        sig.pattern_length = 3;
        // Too generic, skip for now
    }
    
    // memcpy - MSVC x64
    {
        FunctionSignature sig;
        sig.name = "memcpy";
        sig.library = "ucrtbase";
        // mov r11, rsp; sub rsp, XX; push rbx
        sig.pattern = {0x4C, 0x8B, 0xDC, 0x48, 0x83, 0xEC};
        sig.mask = {0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF};
        sig.pattern_length = 6;
        signatures.push_back(sig);
    }
    
    // strlen - MSVC x64
    {
        FunctionSignature sig;
        sig.name = "strlen";
        sig.library = "ucrtbase";
        // mov rax, rcx; (48 8B C1)
        sig.pattern = {0x48, 0x8B, 0xC1};
        sig.mask = {0xFF, 0xFF, 0xFF};
        sig.pattern_length = 3;
        // Too generic, skip
    }

    // fopen / fopen_s patterns
    {
        FunctionSignature sig;
        sig.name = "_fopen_s";
        sig.library = "ucrtbase";
        // Typical: mov [rsp+XX], rbx; push rdi
        sig.pattern = {0x48, 0x89, 0x5C, 0x24};
        sig.mask = {0xFF, 0xFF, 0xFF, 0xFF};
        sig.pattern_length = 4;
        // Still generic, but useful
        signatures.push_back(sig);
    }
    
    // printf
    {
        FunctionSignature sig;
        sig.name = "printf";
        sig.library = "ucrtbase";
        // push rbp; mov rbp, rsp; sub rsp, XX
        sig.pattern = {0x48, 0x89, 0x4C, 0x24, 0x08};  // mov [rsp+8], rcx
        sig.mask = {0xFF, 0xFF, 0xFF, 0xFF, 0xFF};
        sig.pattern_length = 5;
        signatures.push_back(sig);
    }
    
    // HeapAlloc wrapper
    {
        FunctionSignature sig;
        sig.name = "__acrt_heap_alloc";
        sig.library = "ucrtbase";
        // mov rax, qword ptr [rip+XX]
        sig.pattern = {0x48, 0x8B, 0x05};
        sig.mask = {0xFF, 0xFF, 0xFF};
        sig.pattern_length = 3;
        // Very common, skip
    }

    fission::utils::log_stream() << "[FunctionMatcher] Loaded " << signatures.size() 
              << " built-in MSVC x64 signatures" << std::endl;
}

void FunctionMatcher::load_builtin_signatures(const std::string& platform) {
    signatures.clear();
    matched_funcs.clear();
    
    if (platform == "msvc_x64" || platform == "windows_x64") {
        load_builtin_msvc_x64();
    }
    // Add more platforms as needed
}

bool FunctionMatcher::load_signatures(const std::string& json_path) {
    std::ifstream file(json_path);
    if (!file.is_open()) {
        fission::utils::log_stream() << "[FunctionMatcher] Failed to open: " << json_path << std::endl;
        return false;
    }
    
    std::string content((std::istreambuf_iterator<char>(file)),
                         std::istreambuf_iterator<char>());
    
    // Parse JSON array of signature objects
    // Format: [{"name": "malloc", "pattern": "48 83 EC", "mask": "FF FF FF", "library": "ucrtbase"}, ...]
    
    auto parse_hex_bytes = [](const std::string& hex_str) -> std::vector<uint8_t> {
        std::vector<uint8_t> bytes;
        std::istringstream iss(hex_str);
        std::string token;
        while (iss >> token) {
            // Handle wildcards (e.g., "??" or "XX")
            if (token == "??" || token == "XX" || token == "xx") {
                bytes.push_back(0x00);
            } else {
                try {
                    bytes.push_back(static_cast<uint8_t>(std::stoul(token, nullptr, 16)));
                } catch (...) {
                    bytes.push_back(0x00);
                }
            }
        }
        return bytes;
    };
    
    auto objects = fission::utils::extract_json_array(content);
    
    int loaded_count = 0;
    for (const auto& obj : objects) {
        std::string name = fission::utils::extract_json_string(obj, "name");
        std::string pattern_str = fission::utils::extract_json_string(obj, "pattern");
        std::string mask_str = fission::utils::extract_json_string(obj, "mask");
        std::string library = fission::utils::extract_json_string(obj, "library");
        
        if (name.empty() || pattern_str.empty()) {
            continue;
        }
        
        FunctionSignature sig;
        sig.name = name;
        sig.library = library;
        sig.pattern = parse_hex_bytes(pattern_str);
        
        if (!mask_str.empty()) {
            sig.mask = parse_hex_bytes(mask_str);
        } else {
            // Default mask: all FF (exact match)
            sig.mask = std::vector<uint8_t>(sig.pattern.size(), 0xFF);
        }
        
        // Ensure mask and pattern have same length
        while (sig.mask.size() < sig.pattern.size()) {
            sig.mask.push_back(0xFF);
        }
        
        sig.pattern_length = static_cast<int>(sig.pattern.size());
        signatures.push_back(sig);
        loaded_count++;
    }
    
    fission::utils::log_stream() << "[FunctionMatcher] Loaded " << loaded_count 
              << " signatures from " << json_path << std::endl;
    return loaded_count > 0;
}

bool FunctionMatcher::match_pattern(const uint8_t* bytes, int size, 
                                    const FunctionSignature& sig) const {
    if (size < sig.pattern_length) return false;
    
    for (int i = 0; i < sig.pattern_length; ++i) {
        if (sig.mask[i] == 0x00) continue;  // Wildcard
        if ((bytes[i] & sig.mask[i]) != (sig.pattern[i] & sig.mask[i])) {
            return false;
        }
    }
    return true;
}

std::string FunctionMatcher::match(uint64_t address, const uint8_t* bytes, int size) {
    // Check cache first
    auto it = matched_funcs.find(address);
    if (it != matched_funcs.end()) {
        return it->second;
    }
    
    // Try each signature
    for (const auto& sig : signatures) {
        if (match_pattern(bytes, size, sig)) {
            matched_funcs[address] = sig.name;
            fission::utils::log_stream() << "[FunctionMatcher] Matched " << sig.name 
                      << " at 0x" << std::hex << address << std::dec << std::endl;
            return sig.name;
        }
    }
    
    return "";  // No match
}

std::string FunctionMatcher::match_by_fid(uint64_t address, const uint8_t* bytes, size_t size, bool is_x86) {
    static size_t debug_hash_count = 0;
    
    // Check cache first
    auto it = matched_funcs.find(address);
    if (it != matched_funcs.end()) {
        return it->second;
    }
    
    // Check if any FID databases are available
    if (fid_dbs_.empty() && (!fid_db || !fid_db->is_loaded())) {
        return "";
    }
    
    // Minimum function size for reliable matching
    if (size < 8) {
        return "";
    }
    
    // Calculate FID hash
    uint64_t hash = FidHasher::calculate_full_hash(bytes, std::min(size, (size_t)64));
    
    // Debug: Print first 3 computed hashes
    if (debug_hash_count < 3) {
        fission::utils::log_stream() << "[FunctionMatcher] Computed hash at 0x" << std::hex << address 
                  << ": 0x" << hash << std::dec;
        // Show first few bytes
        fission::utils::log_stream() << " bytes=[";
        for (size_t i = 0; i < std::min(size, (size_t)8); ++i) {
            fission::utils::log_stream() << std::hex << (int)bytes[i] << " ";
        }
        fission::utils::log_stream() << "]" << std::dec << std::endl;
        debug_hash_count++;
    }
    
    // Search all loaded FID databases (multi-DB exhaustive lookup)
    const auto& dbs_to_search = fid_dbs_.empty()
        ? std::vector<const FidDatabase*>{fid_db}
        : fid_dbs_;

    for (const auto* db : dbs_to_search) {
        if (!db || !db->is_loaded()) continue;
        std::vector<std::string> matches = db->lookup_by_hash(hash);
        if (!matches.empty()) {
            std::string name = matches[0];
            matched_funcs[address] = name;
            fission::utils::log_stream() << "[FunctionMatcher] FID MATCH! 0x" << std::hex << address
                      << " -> " << name << " (hash=0x" << hash << ")" << std::dec << std::endl;
            return name;
        }
    }

    return "";  // No match across all DBs
}

} // namespace analysis
} // namespace fission
