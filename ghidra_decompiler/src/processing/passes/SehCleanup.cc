#include "fission/processing/PostProcessors.h"

#include <string>
#include <cctype>

namespace fission {
namespace processing {

std::string cleanup_seh_boilerplate(const std::string& code) {
    std::string result = code;
    
    // Replace "unaff_FS_OFFSET" with "TEB" (Thread Environment Block)
    size_t pos = 0;
    while ((pos = result.find("unaff_FS_OFFSET", pos)) != std::string::npos) {
        result.replace(pos, 15, "TEB");
        pos += 3;
    }
    
    // Replace "DWORD *TEB" with "EXCEPTION_REGISTRATION_RECORD *ExceptionList"
    pos = 0;
    while ((pos = result.find("DWORD *TEB", pos)) != std::string::npos) {
        result.replace(pos, 10, "NT_TIB *TIB");
        pos += 11;
    }
    
    // Replace common exception handler patterns
    // Pattern: xStack_XX = *TEB; *TEB = &xStack_XX; ... *TEB = xStack_XX;
    // This is SEH setup/teardown - add comments
    
    // Add comment for SEH setup pattern
    pos = 0;
    while ((pos = result.find("*TIB = &xStack_", pos)) != std::string::npos) {
        // Insert SEH comment before the line
        size_t line_start = result.rfind('\n', pos);
        if (line_start != std::string::npos) {
            result.insert(line_start + 1, "  // SEH: Install exception handler\n");
            pos += 35; // Skip inserted text
        }
        pos += 15;
    }
    
    // Add comment for SEH teardown pattern
    pos = 0;
    while ((pos = result.find("*TIB = xStack_", pos)) != std::string::npos) {
        // Check if this is teardown (assignment back)
        size_t line_start = result.rfind('\n', pos);
        if (line_start != std::string::npos) {
            result.insert(line_start + 1, "  // SEH: Restore exception handler\n");
            pos += 37;
        }
        pos += 14;
    }
    
    // Clean up iRam/pcRam patterns for global variables
    // Replace iRamXXXXXXXX with g_XXXXXXXX
    pos = 0;
    while ((pos = result.find("iRam", pos)) != std::string::npos) {
        // Check if followed by hex address
        if (pos + 4 < result.size() && isxdigit(result[pos + 4])) {
            // Extract the address portion
            size_t addr_start = pos + 4;
            size_t addr_end = addr_start;
            while (addr_end < result.size() && isxdigit(result[addr_end])) {
                addr_end++;
            }
            std::string addr = result.substr(addr_start, addr_end - addr_start);
            std::string replacement = "g_" + addr;
            result.replace(pos, addr_end - pos, replacement);
            pos += replacement.length();
        } else {
            pos++;
        }
    }

    // Replace uRamXXXXXXXX with g_XXXXXXXX
    pos = 0;
    while ((pos = result.find("uRam", pos)) != std::string::npos) {
        if (pos + 4 < result.size() && isxdigit(result[pos + 4])) {
            size_t addr_start = pos + 4;
            size_t addr_end = addr_start;
            while (addr_end < result.size() && isxdigit(result[addr_end])) {
                addr_end++;
            }
            std::string addr = result.substr(addr_start, addr_end - addr_start);
            std::string replacement = "g_" + addr;
            result.replace(pos, addr_end - pos, replacement);
            pos += replacement.length();
        } else {
            pos++;
        }
    }

    // Replace xRamXXXXXXXX with g_XXXXXXXX
    pos = 0;
    while ((pos = result.find("xRam", pos)) != std::string::npos) {
        if (pos + 4 < result.size() && isxdigit(result[pos + 4])) {
            size_t addr_start = pos + 4;
            size_t addr_end = addr_start;
            while (addr_end < result.size() && isxdigit(result[addr_end])) {
                addr_end++;
            }
            std::string addr = result.substr(addr_start, addr_end - addr_start);
            std::string replacement = "g_" + addr;
            result.replace(pos, addr_end - pos, replacement);
            pos += replacement.length();
        } else {
            pos++;
        }
    }
    
    // Replace pcRamXXXXXXXX with gp_XXXXXXXX (pointer)
    pos = 0;
    while ((pos = result.find("pcRam", pos)) != std::string::npos) {
        if (pos + 5 < result.size() && isxdigit(result[pos + 5])) {
            size_t addr_start = pos + 5;
            size_t addr_end = addr_start;
            while (addr_end < result.size() && isxdigit(result[addr_end])) {
                addr_end++;
            }
            std::string addr = result.substr(addr_start, addr_end - addr_start);
            std::string replacement = "gp_" + addr;
            result.replace(pos, addr_end - pos, replacement);
            pos += replacement.length();
        } else {
            pos++;
        }
    }

    // Normalize pg_XXXXXXXX (global pointer) to g_XXXXXXXX
    pos = 0;
    while ((pos = result.find("pg_", pos)) != std::string::npos) {
        size_t addr_start = pos + 3;
        size_t addr_end = addr_start;
        while (addr_end < result.size() && isxdigit(result[addr_end])) {
            addr_end++;
        }
        if (addr_end > addr_start) {
            std::string addr = result.substr(addr_start, addr_end - addr_start);
            std::string replacement = "g_" + addr;
            result.replace(pos, addr_end - pos, replacement);
            pos += replacement.length();
        } else {
            pos++;
        }
    }

    // Normalize pxRamXXXXXXXX (pointer) to gp_XXXXXXXX
    pos = 0;
    while ((pos = result.find("pxRam", pos)) != std::string::npos) {
        size_t addr_start = pos + 5;
        size_t addr_end = addr_start;
        while (addr_end < result.size() && isxdigit(result[addr_end])) {
            addr_end++;
        }
        if (addr_end > addr_start) {
            std::string addr = result.substr(addr_start, addr_end - addr_start);
            std::string replacement = "gp_" + addr;
            result.replace(pos, addr_end - pos, replacement);
            pos += replacement.length();
        } else {
            pos++;
        }
    }

    return result;
}

} // namespace processing
} // namespace fission