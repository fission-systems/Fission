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
    if (ffi_attempted && rust_optimize_fn != nullptr) {
        return true; // Already successfully loaded
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
        fission::utils::log_stream() << "[PcodeOptimizationBridge] ERROR: Could not load Rust FFI functions — "
                                        "Pcode optimization disabled for this session" << std::endl;
        fission::utils::log_stream() << "[PcodeOptimizationBridge] GetLastError: " << GetLastError() << std::endl;
        rust_optimize_fn = nullptr;
        rust_free_fn = nullptr;
        // Keep ffi_attempted = true so we don't spam the log on every call.
        // (mirrors the POSIX path behaviour)
        return false;
    }
#else
    // Clear any stale error state before calling dlsym.
    (void)dlerror();

    // RTLD_DEFAULT searches the main executable and every library currently
    // loaded with RTLD_GLOBAL — covers the common case where the Rust binary
    // has linked the fission_* symbols into its own image.
    rust_optimize_fn = (FissionOptimizePcodeJson)dlsym(RTLD_DEFAULT, "fission_optimize_pcode_json");
    const char* err1 = dlerror();

    if (!rust_optimize_fn) {
        fission::utils::log_stream() << "[PcodeOptimizationBridge] RTLD_DEFAULT dlsym failed"
                                     << (err1 ? std::string(": ") + err1 : "") << std::endl;

        // Second attempt: RTLD_NEXT searches libraries loaded after the C
        // decompiler library itself, which helps when the Rust library is
        // loaded as a plugin after the C++ shared object.
        (void)dlerror();
        rust_optimize_fn = (FissionOptimizePcodeJson)dlsym(RTLD_NEXT, "fission_optimize_pcode_json");
        const char* err2 = dlerror();
        if (!rust_optimize_fn) {
            fission::utils::log_stream() << "[PcodeOptimizationBridge] RTLD_NEXT dlsym also failed"
                                         << (err2 ? std::string(": ") + err2 : "") << std::endl;
        }
    }

    if (rust_optimize_fn) {
        // Load the companion free function from the same search scope.
        (void)dlerror();
        rust_free_fn = (FissionFreeString)dlsym(RTLD_DEFAULT, "fission_free_string");
        if (!rust_free_fn) {
            (void)dlerror();
            rust_free_fn = (FissionFreeString)dlsym(RTLD_NEXT, "fission_free_string");
        }
    }

    if (!rust_optimize_fn || !rust_free_fn) {
        fission::utils::log_stream() << "[PcodeOptimizationBridge] Warning: Rust FFI unavailable — "
                                        "Pcode optimization disabled for this session" << std::endl;
        rust_optimize_fn = nullptr;
        rust_free_fn = nullptr;
        // Keep ffi_attempted = true so we don't spam the log on every call,
        // but re-enable retries by clearing the flag only when the symbol
        // was simply not found yet (as opposed to a hard link error).
        return false;
    }
#endif

    fission::utils::log_stream() << "[PcodeOptimizationBridge] Rust FFI functions loaded successfully" << std::endl;
    return true;
}

void PcodeOptimizationBridge::register_rust_fn_ptrs(
    char* (*optimize_fn)(const char*, size_t),
    void  (*free_fn)(char*)
) {
    rust_optimize_fn = reinterpret_cast<FissionOptimizePcodeJson>(optimize_fn);
    rust_free_fn     = reinterpret_cast<FissionFreeString>(free_fn);
    // Mark as attempted+successful so load_rust_ffi() short-circuits immediately
    // without trying dlsym, and won't reset our pointers on failure.
    ffi_attempted = (rust_optimize_fn != nullptr && rust_free_fn != nullptr);
    if (ffi_attempted) {
        fission::utils::log_stream() << "[PcodeOptimizationBridge] Rust FFI registered via push (no dlsym needed)" << std::endl;
    }
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
