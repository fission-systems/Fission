#include "fission/types/RttiAnalyzer.h"
#include <cstring>
#include <iostream>

namespace fission {
namespace types {

std::map<uint64_t, std::string> RttiAnalyzer::recover_class_names(
    const std::vector<uint8_t>& bytes,
    uint64_t image_base,
    bool is_64bit
) {
    std::map<uint64_t, std::string> result;
    
    // Simplistic scan for ".?AV" signature which indicates a TypeDescriptor name field in MSVC
    // This is a robust heuristic for finding TypeDescriptors.
    
    const char* sig = ".?AV";
    size_t sig_len = 4;
    
    for (size_t i = 0; i < bytes.size() - sig_len; ++i) {
        if (std::memcmp(&bytes[i], sig, sig_len) == 0) {
            // Found a potential TypeDescriptor name.
            // Format: void* pVFTable; void* spare; char name[];
            // So the start of the TypeDescriptor structure is: i - (ptr_size * 2)
            
            // Extract the name
            std::string class_name;
            size_t name_start = i;
            while (name_start < bytes.size() && bytes[name_start] != 0) {
                class_name += (char)bytes[name_start];
                name_start++;
            }
            
            // Clean up name: ".?AVClassName@@" -> "ClassName"
            // Remove ".?AV"
            if (class_name.length() > 4) {
                class_name = class_name.substr(4); 
                // Remove trailing "@@" if present
                if (class_name.length() > 2 && class_name.substr(class_name.length()-2) == "@@") {
                    class_name = class_name.substr(0, class_name.length()-2);
                }
                
                // Store result (using file offset as key for now, ideally we map back to VA)
                // For now, allow mapping offset to name for simple reference
                // Ideally we find the VTable pointing to this.
                
                // To keep this lightweight: Just logging found classes for now or populating a simple symbol table
                // passed to Ghidra if we had symbol integration.
                // For this step, we'll just populate a map of "Known Classes"
                result[image_base + i] = class_name;
            }
        }
    }
    
    return result;
}

} // namespace types
} // namespace fission
