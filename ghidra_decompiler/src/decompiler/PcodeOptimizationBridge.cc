#include "fission/decompiler/PcodeOptimizationBridge.h"
#include "fission/decompiler/PcodeExtractor.h"
#include <iostream>
#include "fission/utils/logger.h"

#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#endif

// Function pointer types for Rust FFI functions
typedef char* (*FissionOptimizePcodeJson)(const char*, size_t);
typedef void (*FissionFreeString)(char*);

namespace fission {
namespace decompiler {

// Static member initialization
bool PcodeOptimizationBridge::optimization_enabled = true;

// Lazy-loaded function pointers
static FissionOptimizePcodeJson rust_optimize_fn = nullptr;
static FissionFreeString rust_free_fn = nullptr;
static bool ffi_attempted = false;

// Try to load Rust FFI functions from the main executable
static bool load_rust_ffi() {
    if (ffi_attempted) {
        return (rust_optimize_fn != nullptr);
    }
    
    ffi_attempted = true;
    
#ifdef _WIN32
    // On Windows, use GetProcAddress to find symbols in the current process
    HMODULE hModule = GetModuleHandleA(NULL);
    if (hModule) {
        rust_optimize_fn = (FissionOptimizePcodeJson)GetProcAddress(hModule, "fission_optimize_pcode_json");
        rust_free_fn = (FissionFreeString)GetProcAddress(hModule, "fission_free_string");
    }
    
    if (!rust_optimize_fn || !rust_free_fn) {
        fission::utils::log_stream() << "[PcodeOptimizationBridge] Warning: Could not load Rust FFI functions" << std::endl;
        fission::utils::log_stream() << "[PcodeOptimizationBridge] GetLastError: " << GetLastError() << std::endl;
        return false;
    }
#else
    // RTLD_DEFAULT searches in the main executable and all loaded libraries
    rust_optimize_fn = (FissionOptimizePcodeJson)dlsym(RTLD_DEFAULT, "fission_optimize_pcode_json");
    rust_free_fn = (FissionFreeString)dlsym(RTLD_DEFAULT, "fission_free_string");
    
    if (!rust_optimize_fn || !rust_free_fn) {
        fission::utils::log_stream() << "[PcodeOptimizationBridge] Warning: Could not load Rust FFI functions" << std::endl;
        fission::utils::log_stream() << "[PcodeOptimizationBridge] dlsym error: " << dlerror() << std::endl;
        return false;
    }
#endif
    
    fission::utils::log_stream() << "[PcodeOptimizationBridge] Rust FFI functions loaded successfully" << std::endl;
    return true;
}

void PcodeOptimizationBridge::set_enabled(bool enabled) {
    optimization_enabled = enabled;
    fission::utils::log_stream() << "[PcodeOptimizationBridge] Optimization " 
              << (enabled ? "ENABLED" : "DISABLED") << std::endl;
}

bool PcodeOptimizationBridge::is_enabled() {
    return optimization_enabled;
}

std::string PcodeOptimizationBridge::optimize_pcode_via_rust(const std::string& pcode_json) {
    if (!optimization_enabled) {
        return pcode_json; // Pass through if disabled
    }
    
    if (pcode_json.empty()) {
        fission::utils::log_stream() << "[PcodeOptimizationBridge] Warning: empty Pcode JSON" << std::endl;
        return pcode_json;
    }
    
    // Load Rust FFI functions if not already loaded
    if (!load_rust_ffi()) {
        fission::utils::log_stream() << "[PcodeOptimizationBridge] FFI not available, returning unoptimized" << std::endl;
        return pcode_json;
    }
    
    try {
        // Call Rust optimizer
        char* optimized_ptr = rust_optimize_fn(pcode_json.c_str(), pcode_json.length());
        
        if (!optimized_ptr) {
            fission::utils::log_stream() << "[PcodeOptimizationBridge] Error: Rust optimizer returned null" << std::endl;
            return pcode_json; // Fallback to original
        }
        
        // Copy to C++ string
        std::string optimized(optimized_ptr);
        
        // Free Rust-allocated memory
        rust_free_fn(optimized_ptr);
        
        fission::utils::log_stream() << "[PcodeOptimizationBridge] Optimization successful: " 
                  << pcode_json.length() << " -> " << optimized.length() << " bytes" << std::endl;
        
        return optimized;
        
    } catch (const std::exception& e) {
        fission::utils::log_stream() << "[PcodeOptimizationBridge] Exception during optimization: " 
                  << e.what() << std::endl;
        return pcode_json; // Fallback
    } catch (...) {
        fission::utils::log_stream() << "[PcodeOptimizationBridge] Unknown error during optimization" << std::endl;
        return pcode_json; // Fallback
    }
}

std::string PcodeOptimizationBridge::extract_and_optimize(ghidra::Funcdata* fd) {
    if (!fd) {
        return "";
    }
    
    // Extract Pcode
    std::string pcode_json = PcodeExtractor::extract_pcode_json(fd);
    
    if (pcode_json.empty()) {
        fission::utils::log_stream() << "[PcodeOptimizationBridge] Failed to extract Pcode" << std::endl;
        return "";
    }
    
    // Optimize
    return optimize_pcode_via_rust(pcode_json);
}

} // namespace decompiler
} // namespace fission
