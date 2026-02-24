#ifndef FISSION_PCODE_OPTIMIZATION_BRIDGE_H
#define FISSION_PCODE_OPTIMIZATION_BRIDGE_H

#include <string>

// Forward declarations
namespace ghidra {
    class Funcdata;
}

namespace fission {
namespace decompiler {

/// Bridge between C++ Ghidra decompiler and Rust Pcode optimizer
class PcodeOptimizationBridge {
public:
    /// Enable/disable Pcode optimization
    static void set_enabled(bool enabled);
    
    /// Check if optimization is enabled
    static bool is_enabled();

    /// Register Rust FFI function pointers directly (push-style, avoids dlsym).
    /// Called at startup by the Rust host via decomp_init_pcode_bridge().
    /// @param optimize_fn  Pointer to fission_optimize_pcode_json
    /// @param free_fn      Pointer to fission_free_string
    static void register_rust_fn_ptrs(
        char* (*optimize_fn)(const char*, size_t),
        void  (*free_fn)(char*));
    
    /// Optimize Pcode JSON through Rust FFI
    /// @param pcode_json Input Pcode in JSON format
    /// @return Optimized Pcode in JSON format, or empty string on error
    static std::string optimize_pcode_via_rust(const std::string& pcode_json);
    
    /// Extract Pcode, optimize via Rust, and return optimized JSON
    /// This is a convenience function combining extract + optimize
    /// @param fd Ghidra function data
    /// @return Optimized Pcode JSON
    static std::string extract_and_optimize(ghidra::Funcdata* fd);

private:
    static bool optimization_enabled;
};

} // namespace decompiler
} // namespace fission

#endif // FISSION_PCODE_OPTIMIZATION_BRIDGE_H
