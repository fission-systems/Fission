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
class CallingConvDetector {
public:
    enum ConvType {
        CONV_UNKNOWN,
        CONV_CDECL,
        CONV_STDCALL,
        CONV_FASTCALL,
        CONV_THISCALL,
        CONV_MS_X64,
        CONV_SYSV_X64
    };

private:
    ghidra::Architecture* arch;
    bool is_64bit;
    
    // Register sets for detection
    std::set<std::string> ms_x64_arg_regs;  // RCX, RDX, R8, R9
    std::set<std::string> sysv_arg_regs;    // RDI, RSI, RDX, RCX, R8, R9
    std::set<std::string> fastcall_regs;    // ECX, EDX
    
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

public:
    CallingConvDetector(ghidra::Architecture* arch);
    ~CallingConvDetector();
    
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
