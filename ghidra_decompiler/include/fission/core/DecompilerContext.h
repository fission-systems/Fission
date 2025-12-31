#ifndef FISSION_CORE_DECOMPILER_CONTEXT_H
#define FISSION_CORE_DECOMPILER_CONTEXT_H

#include <string>
#include <map>
#include <vector>
#include <cstdint>
#include "fission/loader/MemoryImage.h"
#include "fission/core/CliArchitecture.h"

namespace fission {
namespace core {

class DecompilerContext {
public:
    bool initialized = false;
    std::string sla_dir;
    
    // Cached architecture objects
    fission::loader::MemoryLoadImage* loader_64bit = nullptr;
    fission::loader::MemoryLoadImage* loader_32bit = nullptr;
    CliArchitecture* arch_64bit = nullptr;
    CliArchitecture* arch_32bit = nullptr;
    
    bool arch_64bit_ready = false;
    bool arch_32bit_ready = false;
    
    // Store IAT symbols for post-processing
    std::map<uint64_t, std::string> iat_symbols;
    
    // Store enum/constant values for constant name substitution (value -> name)
    std::map<uint64_t, std::string> enum_values;
    
    DecompilerContext();
    ~DecompilerContext();
    
    // Initialize Ghidra library (only once)
    bool initialize(const std::string& sleigh_directory);
};

} // namespace core
} // namespace fission

#endif // FISSION_CORE_DECOMPILER_CONTEXT_H
