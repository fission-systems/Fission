#ifndef __CALLING_CONV_DETECTOR_H__
#define __CALLING_CONV_DETECTOR_H__

#include <string>
#include <set>

namespace ghidra {
    class Funcdata;
    class Architecture;
}

namespace fission {
namespace analysis {

/// \brief Calling Convention Detection
///
/// Analyzes register usage patterns to detect calling convention.
/// Supports format hints (compiler_id) to adjust heuristic thresholds
/// and prioritize the correct ABI check order.
class CallingConvDetector {
public:
    enum ConvType {
        CONV_UNKNOWN,
        CONV_CDECL,
        CONV_STDCALL,
        CONV_FASTCALL,
        CONV_THISCALL,
        CONV_MS_X64,
        CONV_SYSV_X64,
        CONV_AAPCS64    ///< ARM64 AAPCS (x0-x7 integer args, v0-v7 FP args)
    };

private:
    ghidra::Architecture* arch;
    bool is_64bit;
    
    /// Binary format hint from compiler_id (e.g. "windows", "gcc", "clang").
    /// Used to adjust detection thresholds and check order.
    std::string format_hint_;
    
    // Register sets for detection
    std::set<std::string> ms_x64_arg_regs;  // RCX, RDX, R8, R9
    std::set<std::string> sysv_arg_regs;    // RDI, RSI, RDX, RCX, R8, R9
    std::set<std::string> fastcall_regs;    // ECX, EDX
    std::set<std::string> aapcs64_arg_regs; // x0-x7, v0-v7
    
    /// Check if function uses MS x64 ABI
    bool check_ms_x64(ghidra::Funcdata* fd);
    
    /// Check if function uses SYSV x64 ABI
    bool check_sysv_x64(ghidra::Funcdata* fd);
    
    /// Check for STDCALL (callee cleanup)
    bool check_stdcall(ghidra::Funcdata* fd);
    
    /// Check for FASTCALL (ECX/EDX args)
    bool check_fastcall(ghidra::Funcdata* fd);
    
    /// Check for THISCALL (ECX = this)
    bool check_thiscall(ghidra::Funcdata* fd);

    /// Check if function uses AAPCS64 (ARM64) ABI: x0-x7, v0-v7
    bool check_aapcs64(ghidra::Funcdata* fd);

public:
    CallingConvDetector(ghidra::Architecture* arch);
    ~CallingConvDetector();
    
    /// \brief Set binary format hint (compiler_id) to improve detection accuracy.
    ///
    /// When set, the detector will:
    /// - Adjust check order (e.g. SYSV before MS x64 for "gcc"/"clang")
    /// - Lower thresholds for the preferred ABI (accept 1 register instead of 2)
    /// \param hint compiler_id string: "windows", "gcc", "clang", "default"
    void set_format_hint(const std::string& hint);
    
    /// \brief Detect calling convention for a function
    /// \param fd The function to analyze
    /// \return Detected calling convention
    ConvType detect(ghidra::Funcdata* fd);
    
    /// \brief Get string name for convention type
    static const char* conv_name(ConvType type);
    
    /// \brief Apply detected convention to function prototype
    void apply(ghidra::Funcdata* fd, ConvType type);
};

} // namespace analysis
} // namespace fission

#endif // __CALLING_CONV_DETECTOR_H__
