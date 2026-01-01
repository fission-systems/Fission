#include "fission/analysis/CallingConvDetector.h"
#include "funcdata.hh"
#include "op.hh"
#include "varnode.hh"
#include "architecture.hh"
#include <iostream>
#include <algorithm>

namespace fission {
namespace analysis {

using namespace ghidra;

CallingConvDetector::CallingConvDetector(Architecture* a) : arch(a) {
    // Determine if 64-bit based on default address size
    is_64bit = (arch->getDefaultDataSpace()->getAddrSize() >= 8);
    
    // Initialize register sets for MS x64 ABI
    ms_x64_arg_regs = {"RCX", "RDX", "R8", "R9", "XMM0", "XMM1", "XMM2", "XMM3"};
    
    // SYSV x64 ABI (Linux/Mac)
    sysv_arg_regs = {"RDI", "RSI", "RDX", "RCX", "R8", "R9"};
    
    // x86 FASTCALL
    fastcall_regs = {"ECX", "EDX"};
}

CallingConvDetector::~CallingConvDetector() {}

bool CallingConvDetector::check_ms_x64(Funcdata* fd) {
    if (!is_64bit) return false;
    
    // Check if RCX/RDX/R8/R9 are used as input parameters
    int arg_reg_count = 0;
    
    list<PcodeOp*>::const_iterator iter;
    for (iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (!op) continue;
        
        // Look for reads of argument registers early in function
        for (int i = 0; i < op->numInput(); ++i) {
            Varnode* vn = op->getIn(i);
            if (!vn || !vn->isInput()) continue;
            
            AddrSpace* sp = vn->getSpace();
            if (!sp || sp->getName() != "register") continue;
            
            // Check if it's an MS x64 arg register
            // This is a simplified check - in practice would use register mappings
            if (vn->getOffset() == 0x10) arg_reg_count++; // RCX
            if (vn->getOffset() == 0x18) arg_reg_count++; // RDX
        }
    }
    
    return arg_reg_count >= 2;
}

bool CallingConvDetector::check_sysv_x64(Funcdata* fd) {
    if (!is_64bit) return false;
    
    // Similar to MS x64 but check for RDI/RSI usage
    int arg_reg_count = 0;
    
    list<PcodeOp*>::const_iterator iter;
    for (iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (!op) continue;
        
        for (int i = 0; i < op->numInput(); ++i) {
            Varnode* vn = op->getIn(i);
            if (!vn || !vn->isInput()) continue;
            
            AddrSpace* sp = vn->getSpace();
            if (!sp || sp->getName() != "register") continue;
            
            // Check for RDI/RSI (SYSV first two args)
            if (vn->getOffset() == 0x38) arg_reg_count++; // RDI
            if (vn->getOffset() == 0x30) arg_reg_count++; // RSI
        }
    }
    
    return arg_reg_count >= 2;
}

bool CallingConvDetector::check_stdcall(Funcdata* fd) {
    if (is_64bit) return false;
    
    // Look for RET with immediate (callee cleans stack)
    list<PcodeOp*>::const_iterator iter;
    for (iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (!op || op->code() != CPUI_RETURN) continue;
        
        // STDCALL typically shows in the adjustment at function end
        // This is a simplified heuristic
        return true;
    }
    
    return false;
}

bool CallingConvDetector::check_fastcall(Funcdata* fd) {
    if (is_64bit) return false;
    
    // Check for ECX/EDX usage as first two parameters
    int ecx_edx_count = 0;
    
    list<PcodeOp*>::const_iterator iter;
    for (iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (!op) continue;
        
        for (int i = 0; i < op->numInput(); ++i) {
            Varnode* vn = op->getIn(i);
            if (!vn || !vn->isInput()) continue;
            
            AddrSpace* sp = vn->getSpace();
            if (!sp || sp->getName() != "register") continue;
            
            // ECX or EDX
            if (vn->getOffset() == 0x8 || vn->getOffset() == 0x10) {
                ecx_edx_count++;
            }
        }
    }
    
    return ecx_edx_count >= 2;
}

bool CallingConvDetector::check_thiscall(Funcdata* fd) {
    if (is_64bit) return false;
    
    // Check if ECX is used as "this" pointer (first arg, pointer type)
    list<PcodeOp*>::const_iterator iter;
    for (iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (!op) continue;
        
        // Look for early ECX usage that appears to be a pointer
        if (op->code() == CPUI_LOAD || op->code() == CPUI_STORE) {
            for (int i = 0; i < op->numInput(); ++i) {
                Varnode* vn = op->getIn(i);
                if (!vn || !vn->isInput()) continue;
                
                AddrSpace* sp = vn->getSpace();
                if (!sp || sp->getName() != "register") continue;
                
                // ECX used as pointer base
                if (vn->getOffset() == 0x8) {
                    return true;
                }
            }
        }
    }
    
    return false;
}

CallingConvDetector::ConvType CallingConvDetector::detect(Funcdata* fd) {
    if (!fd) return CONV_UNKNOWN;
    
    if (is_64bit) {
        // 64-bit: check MS x64 first (Windows), then SYSV (Linux/Mac)
        if (check_ms_x64(fd)) return CONV_MS_X64;
        if (check_sysv_x64(fd)) return CONV_SYSV_X64;
    } else {
        // 32-bit: check in order of specificity
        if (check_thiscall(fd)) return CONV_THISCALL;
        if (check_fastcall(fd)) return CONV_FASTCALL;
        if (check_stdcall(fd)) return CONV_STDCALL;
        return CONV_CDECL; // Default for 32-bit
    }
    
    return CONV_UNKNOWN;
}

const char* CallingConvDetector::conv_name(ConvType type) {
    switch (type) {
        case CONV_CDECL: return "__cdecl";
        case CONV_STDCALL: return "__stdcall";
        case CONV_FASTCALL: return "__fastcall";
        case CONV_THISCALL: return "__thiscall";
        case CONV_MS_X64: return "__fastcall"; // MS x64 uses fastcall name
        case CONV_SYSV_X64: return "__sysv_abi";
        default: return "unknown";
    }
}

void CallingConvDetector::apply(Funcdata* fd, ConvType type) {
    if (!fd || type == CONV_UNKNOWN) return;
    
    std::cerr << "[CallingConvDetector] Detected " << conv_name(type) 
              << " for function at 0x" << std::hex 
              << fd->getAddress().getOffset() << std::dec << std::endl;
    
    // Note: Actually applying the calling convention requires modifying
    // the FuncProto, which is done through Architecture/ProtoModel
}

} // namespace analysis
} // namespace fission
