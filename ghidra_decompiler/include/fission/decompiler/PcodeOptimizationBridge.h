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
