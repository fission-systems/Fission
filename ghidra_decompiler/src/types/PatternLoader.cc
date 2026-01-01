#include "fission/types/PatternLoader.h"
#include <iostream>

namespace fission {
namespace types {

std::vector<BytePattern> PatternLoader::load_standard_patterns() {
    std::vector<BytePattern> patterns;
    
    // Hardcoded common patterns for x64 Windows as an example
    // In a full implementation, we would parse the XML.
    
    // __security_check_cookie (common prologue)
    // 48 89 5c 24 08          mov    QWORD PTR [rsp+0x8],rbx
    // 57                      push   rdi
    // 48 83 ec 20             sub    rsp,0x20
    // 48 8b f9                mov    rdi,rcx
    
    // Just a placeholder example of how this would work
    BytePattern chkstk;
    chkstk.name = "__chkstk";
    // 48 83 ec 10 4c 89 14 24
    chkstk.bytes = {0x48, 0x83, 0xec, 0x10, 0x4c, 0x89, 0x14, 0x24}; 
    chkstk.mask =  {true, true, true, true, true, true, true, true};
    patterns.push_back(chkstk);

    return patterns;
}

std::map<uint64_t, std::string> PatternLoader::match_functions(
    const std::vector<uint8_t>& memory, 
    uint64_t base_address,
    const std::vector<BytePattern>& patterns
) {
    std::map<uint64_t, std::string> matches;
    
    // Naive scan (slow for large binaries, okay for small/proof of concept)
    // O(M * N * P) where M = mem size, N = patterns, P = pattern len
    
    for (size_t i = 0; i < memory.size(); ++i) {
        for (const auto& pat : patterns) {
            if (i + pat.bytes.size() > memory.size()) continue;
            
            bool match = true;
            for (size_t j = 0; j < pat.bytes.size(); ++j) {
                if (pat.mask[j] && memory[i+j] != pat.bytes[j]) {
                    match = false;
                    break;
                }
            }
            
            if (match) {
                matches[base_address + i] = pat.name;
                // Optimization: Maybe skip ahead?
                // i += pat.bytes.size() - 1; // Careful with overlapping patterns
                // break; // If we assume one name per address
            }
        }
    }
    
    return matches;
}

std::vector<uint64_t> PatternLoader::scan_function_prologues(
    const std::vector<uint8_t>& memory,
    uint64_t base_address
) {
    std::vector<uint64_t> prologues;
    const size_t SCAN_INTERVAL = 16;  // Every 16 bytes
    const size_t MAX_SCAN = std::min(memory.size(), (size_t)(10 * 1024 * 1024));  // Max 10MB

    for (size_t offset = 0; offset + 4 < MAX_SCAN; offset += SCAN_INTERVAL) {
        uint8_t b0 = memory[offset];
        uint8_t b1 = memory[offset + 1];
        uint8_t b2 = memory[offset + 2];
        uint8_t b3 = memory[offset + 3];
        
        bool is_prologue = false;
        
        // x86 Standard prologues
        // push ebp; mov ebp, esp (55 8B EC)
        if (b0 == 0x55 && b1 == 0x8B && b2 == 0xEC) is_prologue = true;
        // push ebp; mov ebp, esp (55 89 E5)
        else if (b0 == 0x55 && b1 == 0x89 && b2 == 0xE5) is_prologue = true;
        // mov edi, edi; push ebp; mov ebp, esp (8B FF 55 8B)
        else if (b0 == 0x8B && b1 == 0xFF && b2 == 0x55 && b3 == 0x8B) is_prologue = true;
        // sub esp, XX (83 EC XX) - common for leaf functions
        else if (b0 == 0x83 && b1 == 0xEC) is_prologue = true;
        // push ebx (53)
        else if (b0 == 0x53 && (b1 == 0x8B || b1 == 0x56 || b1 == 0x57)) is_prologue = true;
        // push esi (56)
        else if (b0 == 0x56 && (b1 == 0x8B || b1 == 0x57 || b1 == 0x53)) is_prologue = true;
        // push edi (57)
        else if (b0 == 0x57 && (b1 == 0x8B || b1 == 0x56 || b1 == 0x53)) is_prologue = true;
        // push -1 for SEH (6A FF)
        else if (b0 == 0x6A && b1 == 0xFF) is_prologue = true;
        // mov eax, fs:[0] for SEH (64 A1 00 00 00 00)
        else if (b0 == 0x64 && b1 == 0xA1) is_prologue = true;
        // int 3 padding followed by prologue
        else if (b0 == 0xCC && b1 == 0x55 && b2 == 0x8B) {
            // Adjust offset to point to actual prologue
            prologues.push_back(base_address + offset + 1);
            continue;
        }
        
        if (is_prologue) {
            prologues.push_back(base_address + offset);
        }
    }
    return prologues;
}

} // namespace types
} // namespace fission
