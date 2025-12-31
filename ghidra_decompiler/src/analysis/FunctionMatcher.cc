#include "fission/analysis/FunctionMatcher.h"
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

    std::cerr << "[FunctionMatcher] Loaded " << signatures.size() 
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
        std::cerr << "[FunctionMatcher] Failed to open: " << json_path << std::endl;
        return false;
    }
    
    // Simple JSON parsing (for demonstration)
    // In production, use a proper JSON library
    std::string content((std::istreambuf_iterator<char>(file)),
                         std::istreambuf_iterator<char>());
    
    // TODO: Parse JSON format like:
    // [{"name": "malloc", "pattern": "48 83 EC", "mask": "FF FF FF"}, ...]
    
    std::cerr << "[FunctionMatcher] JSON loading not yet implemented" << std::endl;
    return false;
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
            std::cerr << "[FunctionMatcher] Matched " << sig.name 
                      << " at 0x" << std::hex << address << std::dec << std::endl;
            return sig.name;
        }
    }
    
    return "";  // No match
}

} // namespace analysis
} // namespace fission
