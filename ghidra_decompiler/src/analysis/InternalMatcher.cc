#include "fission/analysis/InternalMatcher.h"
#include <iostream>
#include "fission/utils/logger.h"
#include <algorithm>
#include <cstring>

namespace fission {
namespace analysis {

InternalMatcher::InternalMatcher() {
    load_pyinstaller_signatures();
    load_common_crt_signatures();
}

InternalMatcher::~InternalMatcher() {}

void InternalMatcher::load_pyinstaller_signatures() {
    // PyInstaller runtime functions - based on string references
    
    // pyi_launch_setup - initializes the archive
    {
        InternalSignature sig;
        sig.name = "pyi_launch_setup";
        sig.strings = {"PYINSTALLER_RESET_ENVIRONMENT", "_PYI_ARCHIVE_FILE"};
        sig.min_size = 0x500;
        signatures.push_back(sig);
    }
    
    // pyi_launch_run - main execution
    {
        InternalSignature sig;
        sig.name = "pyi_launch_run";
        sig.strings = {"_PYI_APPLICATION_HOME_DIR", "_PYI_PARENT_PROCESS_LEVEL"};
        sig.min_size = 0x200;
        signatures.push_back(sig);
    }
    
    // pyi_archive_open
    {
        InternalSignature sig;
        sig.name = "pyi_archive_open";
        sig.strings = {"pyi-runtime-tmpdir", "pyi-contents-directory"};
        sig.min_size = 0x100;
        signatures.push_back(sig);
    }
    
    // pyi_splash_setup
    {
        InternalSignature sig;
        sig.name = "pyi_splash_setup";
        sig.strings = {"_PYI_SPLASH_IPC"};
        sig.min_size = 0x50;
        signatures.push_back(sig);
    }
    
    // pyi_hide_console
    {
        InternalSignature sig;
        sig.name = "pyi_hide_console";
        sig.strings = {"pyi-hide-console", "hide-early", "minimize-early"};
        sig.min_size = 0x50;
        signatures.push_back(sig);
    }
    
    // pyi_python_flag
    {
        InternalSignature sig;
        sig.name = "pyi_process_python_flags";
        sig.strings = {"pyi-python-flag", "Py_GIL_DISABLED"};
        sig.min_size = 0x30;
        signatures.push_back(sig);
    }

    // pyi_getenv
    {
        InternalSignature sig;
        sig.name = "pyi_getenv";
        sig.strings = {};  // Generic - will match by prologue
        sig.min_size = 0x20;
        sig.max_size = 0x100;
        signatures.push_back(sig);
    }

    fission::utils::log_stream() << "[InternalMatcher] Loaded " << signatures.size() 
              << " PyInstaller signatures" << std::endl;
}

void InternalMatcher::load_common_crt_signatures() {
    // Common C runtime functions
    
    // __security_init_cookie
    {
        InternalSignature sig;
        sig.name = "__security_init_cookie";
        sig.prologue = {0x48, 0x83, 0xEC};  // sub rsp, XX
        sig.min_size = 0x40;
        sig.max_size = 0x150;
        signatures.push_back(sig);
    }
    
    // __security_check_cookie
    {
        InternalSignature sig;
        sig.name = "__security_check_cookie";
        sig.prologue = {0x48, 0x3B, 0x0D};  // cmp rcx, [rip+XX]
        sig.min_size = 0x10;
        sig.max_size = 0x40;
        signatures.push_back(sig);
    }
}

std::string InternalMatcher::match_by_strings(uint64_t address, 
                                              const std::vector<std::string>& strings) {
    if (strings.empty()) return "";
    
    // Check cache
    auto it = matched.find(address);
    if (it != matched.end()) return it->second;
    
    for (const auto& sig : signatures) {
        if (sig.strings.empty()) continue;
        
        // Count how many signature strings are found
        int found = 0;
        for (const auto& needle : sig.strings) {
            for (const auto& s : strings) {
                if (s.find(needle) != std::string::npos) {
                    ++found;
                    break;
                }
            }
        }
        
        // Require at least half of signature strings to match
        if (found > 0 && found >= (int)sig.strings.size() / 2 + 1) {
            matched[address] = sig.name;
            fission::utils::log_stream() << "[InternalMatcher] Matched " << sig.name 
                      << " at 0x" << std::hex << address << std::dec 
                      << " (" << found << "/" << sig.strings.size() << " strings)" << std::endl;
            return sig.name;
        }
    }
    
    return "";
}

std::string InternalMatcher::match_by_prologue(uint64_t address, 
                                                const uint8_t* bytes, int size) {
    if (!bytes || size < 3) return "";
    
    // Check cache
    auto it = matched.find(address);
    if (it != matched.end()) return it->second;
    
    for (const auto& sig : signatures) {
        if (sig.prologue.empty()) continue;
        if ((int)sig.prologue.size() > size) continue;
        
        // Compare prologue
        if (std::memcmp(bytes, sig.prologue.data(), sig.prologue.size()) == 0) {
            matched[address] = sig.name;
            fission::utils::log_stream() << "[InternalMatcher] Matched " << sig.name 
                      << " at 0x" << std::hex << address << std::dec 
                      << " (by prologue)" << std::endl;
            return sig.name;
        }
    }
    
    return "";
}

} // namespace analysis
} // namespace fission
