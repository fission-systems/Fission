#include "fission/analysis/CallingConvDetector.h"
#include "funcdata.hh"
#include "op.hh"
#include "varnode.hh"
#include "architecture.hh"
#include "translate.hh"
#include <iostream>
#include "fission/utils/logger.h"
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
    
    // Check if MS x64 argument registers are used:
    // - Integer/pointer args: RCX, RDX, R8, R9
    // - FP args: XMM0, XMM1, XMM2, XMM3
    std::set<std::string> gpr_regs_used;
    std::set<std::string> xmm_regs_used;
    const Translate* trans = arch->translate;
    
    int total_ops = 0;
    int input_varnodes = 0;
    
    list<PcodeOp*>::const_iterator iter;
    for (iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (!op) continue;
        total_ops++;
        
        // Look for reads of argument registers early in function
        for (int i = 0; i < op->numInput(); ++i) {
            Varnode* vn = op->getIn(i);
            if (!vn || !vn->isInput()) continue;
            input_varnodes++;
            
            AddrSpace* sp = vn->getSpace();
            if (!sp || sp->getName() != "register") continue;
            
            // Get register name from translator
            std::string reg_name = trans->getRegisterName(sp, vn->getOffset(), vn->getSize());
            
            fission::utils::log_stream() << "  Found input register: " << reg_name 
                      << " (offset=0x" << std::hex << vn->getOffset() 
                      << ", size=" << std::dec << vn->getSize() << ")" << std::endl;
            
            // Check if it's an MS x64 arg register (GPR or XMM)
            if (reg_name == "RCX" || reg_name == "RDX" || 
                reg_name == "R8" || reg_name == "R9") {
                gpr_regs_used.insert(reg_name);
                fission::utils::log_stream() << "    -> MS x64 GPR arg register!" << std::endl;
            } else if (reg_name == "XMM0" || reg_name == "XMM1" ||
                       reg_name == "XMM2" || reg_name == "XMM3") {
                xmm_regs_used.insert(reg_name);
                fission::utils::log_stream() << "    -> MS x64 XMM arg register!" << std::endl;
            }
        }
        
        // Early exit if we found enough evidence:
        // - at least 2 GPR arg regs, OR
        // - mixed integer/fp usage (>=1 GPR and >=1 XMM), OR
        // - at least 2 XMM arg regs
        if (gpr_regs_used.size() >= 2 ||
            (gpr_regs_used.size() >= 1 && xmm_regs_used.size() >= 1) ||
            xmm_regs_used.size() >= 2) {
            fission::utils::log_stream() << "[CallingConvDetector] MS x64 detected (gpr="
                      << gpr_regs_used.size() << ", xmm=" << xmm_regs_used.size() << ")" << std::endl;
            return true;
        }
    }
    
    fission::utils::log_stream() << "[CallingConvDetector] MS x64 check: total_ops=" << total_ops 
              << ", input_varnodes=" << input_varnodes 
              << ", gpr_arg_regs=" << gpr_regs_used.size()
              << ", xmm_arg_regs=" << xmm_regs_used.size() << std::endl;
    
    return gpr_regs_used.size() >= 2 ||
           (gpr_regs_used.size() >= 1 && xmm_regs_used.size() >= 1) ||
           xmm_regs_used.size() >= 2;
}

bool CallingConvDetector::check_sysv_x64(Funcdata* fd) {
    if (!is_64bit) return false;
    
    // Check for RDI/RSI usage (SYSV first two args)
    std::set<std::string> regs_used;
    const Translate* trans = arch->translate;
    
    list<PcodeOp*>::const_iterator iter;
    for (iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (!op) continue;
        
        for (int i = 0; i < op->numInput(); ++i) {
            Varnode* vn = op->getIn(i);
            if (!vn || !vn->isInput()) continue;
            
            AddrSpace* sp = vn->getSpace();
            if (!sp || sp->getName() != "register") continue;
            
            // Get register name from translator
            std::string reg_name = trans->getRegisterName(sp, vn->getOffset(), vn->getSize());
            
            // Check for SYSV x64 arg registers (RDI, RSI, RDX, RCX, R8, R9)
            if (reg_name == "RDI" || reg_name == "RSI" || 
                reg_name == "RDX" || reg_name == "RCX" ||
                reg_name == "R8" || reg_name == "R9") {
                regs_used.insert(reg_name);
            }
        }
        
        // Early exit if we found enough evidence
        if (regs_used.size() >= 2) return true;
    }
    
    return regs_used.size() >= 2;
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
    
    fission::utils::log_stream() << "[CallingConvDetector] Detecting convention for function at 0x" 
              << std::hex << fd->getAddress().getOffset() << std::dec 
              << ", is_64bit=" << is_64bit << std::endl;
    
    if (is_64bit) {
        // 64-bit: check MS x64 first (Windows), then SYSV (Linux/Mac)
        fission::utils::log_stream() << "[CallingConvDetector] Checking MS x64..." << std::endl;
        if (check_ms_x64(fd)) return CONV_MS_X64;
        
        fission::utils::log_stream() << "[CallingConvDetector] Checking SYSV x64..." << std::endl;
        if (check_sysv_x64(fd)) return CONV_SYSV_X64;
    } else {
        // 32-bit: check in order of specificity
        if (check_thiscall(fd)) return CONV_THISCALL;
        if (check_fastcall(fd)) return CONV_FASTCALL;
        if (check_stdcall(fd)) return CONV_STDCALL;
        return CONV_CDECL; // Default for 32-bit
    }
    
    fission::utils::log_stream() << "[CallingConvDetector] No convention detected" << std::endl;
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
    
    fission::utils::log_stream() << "[CallingConvDetector] Detected " << conv_name(type) 
              << " for function at 0x" << std::hex 
              << fd->getAddress().getOffset() << std::dec << std::endl;
    
    // Get the appropriate ProtoModel from architecture
    ProtoModel* model = nullptr;
    
    switch (type) {
        case CONV_MS_X64:
            // Windows x64 uses "__fastcall"
            model = arch->getModel("__fastcall");
            break;
        case CONV_SYSV_X64:
            // Linux/Mac x64 System V ABI
            model = arch->getModel("__sysv_abi");
            if (!model) {
                model = arch->getModel("sysv");
            }
            if (!model) {
                model = arch->getModel("__cdecl");
            }
            break;
        case CONV_CDECL:
            model = arch->getModel("__cdecl");
            break;
        case CONV_STDCALL:
            model = arch->getModel("__stdcall");
            break;
        case CONV_FASTCALL:
            model = arch->getModel("__fastcall");
            break;
        case CONV_THISCALL:
            model = arch->getModel("__thiscall");
            break;
        default:
            break;
    }
    
    if (model) {
        FuncProto& proto = fd->getFuncProto();
        proto.setModel(model);
        fission::utils::log_stream() << "[CallingConvDetector] Applied " << model->getName() 
                  << " to function at 0x" << std::hex 
                  << fd->getAddress().getOffset() << std::dec << std::endl;
    } else {
        fission::utils::log_stream() << "[CallingConvDetector] WARNING: Could not find ProtoModel for " 
                  << conv_name(type) << std::endl;
    }
}

} // namespace analysis
} // namespace fission
