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

    // AAPCS64 (ARM64) integer + FP arg registers
    aapcs64_arg_regs = {"x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7",
                        "v0", "v1", "v2", "v3", "v4", "v5", "v6", "v7"};
}

CallingConvDetector::~CallingConvDetector() {}

void CallingConvDetector::set_format_hint(const std::string& hint) {
    format_hint_ = hint;
    fission::utils::log_stream() << "[CallingConvDetector] Format hint set to: " << hint << std::endl;
}

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
    // Also track SYSV-unique registers (RDI, RSI) that are NOT in MS x64 ABI.
    // If we see these, even one is strong evidence for SYSV.
    bool has_sysv_unique = false;
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
            // Also match sub-registers of RDI/RSI: EDI, ESI, DI, SI, DIL, SIL
            if (reg_name == "EDI" || reg_name == "ESI" ||
                reg_name == "DI"  || reg_name == "SI"  ||
                reg_name == "DIL" || reg_name == "SIL") {
                regs_used.insert(reg_name);
                has_sysv_unique = true;
            }
            // RDI and RSI are SYSV-unique (not used as args in MS x64)
            if (reg_name == "RDI" || reg_name == "RSI") {
                has_sysv_unique = true;
            }
        }
        
        // Early exit if we found enough evidence
        if (regs_used.size() >= 2) return true;
    }
    
    // If format hint indicates non-Windows, a single SYSV-unique register
    // (RDI, RSI, or their sub-registers) is sufficient evidence.
    // This handles single-argument functions that would otherwise go undetected.
    bool hint_is_nonwindows = (!format_hint_.empty() &&
                               format_hint_ != "windows");
    if (has_sysv_unique && hint_is_nonwindows) {
        fission::utils::log_stream() << "[CallingConvDetector] SYSV x64 detected via single "
                  << "SYSV-unique register + non-Windows hint" << std::endl;
        return true;
    }
    
    return regs_used.size() >= 2;
}

bool CallingConvDetector::check_stdcall(Funcdata* fd) {
    if (is_64bit) return false;

    // Detect stdcall by inspecting the RET instruction opcode.
    // x86 RET imm16 (opcode 0xC2) cleans the stack — this is the hallmark of
    // __stdcall.  Normal cdecl uses plain RET (opcode 0xC3).
    //
    // This mirrors Ghidra's own fillinExtrapop() logic in funcdata.cc.

    list<PcodeOp*>::const_iterator iter = fd->beginOp(CPUI_RETURN);
    if (iter == fd->endOp(CPUI_RETURN)) {
        return false; // No return statements — cannot determine convention
    }

    PcodeOp* retop = *iter;
    uint1 buffer[4];

    try {
        arch->loader->loadFill(buffer, 4, retop->getAddr());
    } catch (...) {
        // If we cannot read the instruction bytes, fall back to cdecl
        return false;
    }

    // 0xC2 = RET imm16 (near return, pops imm16 bytes)
    // 0xCA = RETF imm16 (far return, pops imm16 bytes) — rare but possible
    if (buffer[0] == 0xC2 || buffer[0] == 0xCA) {
        int stack_cleanup = buffer[1] | (buffer[2] << 8);
        fission::utils::log_stream() << "[CallingConvDetector] stdcall detected: RET 0x"
                  << std::hex << stack_cleanup << std::dec
                  << " (stack cleanup " << stack_cleanup << " bytes + 4 for retaddr)"
                  << std::endl;
        return true;
    }

    return false;
}

bool CallingConvDetector::check_fastcall(Funcdata* fd) {
    if (is_64bit) return false;

    // A-3: Use getRegisterName() instead of raw offset comparisons.
    // Hardcoded offsets (0x8=ECX, 0x10=EDX) are SLEIGH-version-dependent and
    // would silently misfire on other architectures or updated SLEIGH specs.
    const Translate* trans = arch->translate;
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

            std::string reg_name = trans->getRegisterName(sp, vn->getOffset(), vn->getSize());
            if (reg_name == "ECX" || reg_name == "EDX") {
                ecx_edx_count++;
            }
        }
    }

    return ecx_edx_count >= 2;
}

bool CallingConvDetector::check_thiscall(Funcdata* fd) {
    if (is_64bit) return false;

    // A-3: Use getRegisterName() instead of raw offset comparison (0x8=ECX).
    const Translate* trans = arch->translate;

    bool ecx_as_ptr = false;
    bool edx_as_input = false;

    list<PcodeOp*>::const_iterator iter;
    for (iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (!op) continue;

        // Look for ECX used as a pointer base in LOAD/STORE ("this" pointer pattern)
        if (op->code() == CPUI_LOAD || op->code() == CPUI_STORE) {
            for (int i = 0; i < op->numInput(); ++i) {
                Varnode* vn = op->getIn(i);
                if (!vn || !vn->isInput()) continue;

                AddrSpace* sp = vn->getSpace();
                if (!sp || sp->getName() != "register") continue;

                std::string reg_name = trans->getRegisterName(sp, vn->getOffset(), vn->getSize());
                if (reg_name == "ECX") {
                    ecx_as_ptr = true;
                }
            }
        }

        // Also check if EDX is used as a register input anywhere.
        // If both ECX (ptr) and EDX are inputs, this is __fastcall, not __thiscall.
        for (int i = 0; i < op->numInput(); ++i) {
            Varnode* vn = op->getIn(i);
            if (!vn || !vn->isInput()) continue;

            AddrSpace* sp = vn->getSpace();
            if (!sp || sp->getName() != "register") continue;

            std::string reg_name = trans->getRegisterName(sp, vn->getOffset(), vn->getSize());
            if (reg_name == "EDX") {
                edx_as_input = true;
            }
        }
    }

    // ECX as pointer + EDX as input => __fastcall (both parameter regs used)
    if (ecx_as_ptr && edx_as_input) {
        return false;
    }

    return ecx_as_ptr;
}

bool CallingConvDetector::check_aapcs64(Funcdata* fd) {
    if (!is_64bit) return false;

    // Verify this is actually an AArch64 architecture by probing for register "x0".
    // If the translate spec does not know "x0", we are not on ARM64.
    const Translate* trans = arch->translate;
    bool arch_is_aarch64 = false;
    try {
        VarnodeData vd = trans->getRegister("x0");
        arch_is_aarch64 = (vd.size > 0);
    } catch (...) {}
    if (!arch_is_aarch64) return false;

    // Count AAPCS64 argument registers used (x0-x7, v0-v7)
    std::set<std::string> regs_used;
    list<PcodeOp*>::const_iterator iter;
    for (iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (!op) continue;

        for (int i = 0; i < op->numInput(); ++i) {
            Varnode* vn = op->getIn(i);
            if (!vn || !vn->isInput()) continue;

            AddrSpace* sp = vn->getSpace();
            if (!sp || sp->getName() != "register") continue;

            std::string reg_name = trans->getRegisterName(sp, vn->getOffset(), vn->getSize());
            if (aapcs64_arg_regs.count(reg_name)) {
                regs_used.insert(reg_name);
            }
        }

        if (regs_used.size() >= 2) {
            fission::utils::log_stream() << "[CallingConvDetector] AAPCS64 detected (regs="
                      << regs_used.size() << ")" << std::endl;
            return true;
        }
    }

    return regs_used.size() >= 2;
}

CallingConvDetector::ConvType CallingConvDetector::detect(Funcdata* fd) {
    if (!fd) return CONV_UNKNOWN;
    
    fission::utils::log_stream() << "[CallingConvDetector] Detecting convention for function at 0x" 
              << std::hex << fd->getAddress().getOffset() << std::dec 
              << ", is_64bit=" << is_64bit
              << ", format_hint=" << (format_hint_.empty() ? "(none)" : format_hint_) << std::endl;
    
    if (is_64bit) {
        // AArch64 has priority: probe for x0 register presence
        if (check_aapcs64(fd)) return CONV_AAPCS64;

        // Use format hint to determine check order.
        // For non-Windows binaries (gcc, clang, default ELF/Mach-O), check
        // SYSV first to avoid false MS x64 positives (RDX, RCX overlap).
        bool prefer_sysv = (!format_hint_.empty() && format_hint_ != "windows");
        
        if (prefer_sysv) {
            fission::utils::log_stream() << "[CallingConvDetector] Checking SYSV x64 first (non-Windows hint)..." << std::endl;
            if (check_sysv_x64(fd)) return CONV_SYSV_X64;
            
            fission::utils::log_stream() << "[CallingConvDetector] Checking MS x64..." << std::endl;
            if (check_ms_x64(fd)) return CONV_MS_X64;
        } else {
            // Windows or unknown: check MS x64 first
            fission::utils::log_stream() << "[CallingConvDetector] Checking MS x64..." << std::endl;
            if (check_ms_x64(fd)) return CONV_MS_X64;
            
            fission::utils::log_stream() << "[CallingConvDetector] Checking SYSV x64..." << std::endl;
            if (check_sysv_x64(fd)) return CONV_SYSV_X64;
        }
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
        case CONV_AAPCS64: return "__aapcs64";
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
        case CONV_AAPCS64:
            model = arch->getModel("__aapcs64");
            if (!model) model = arch->getModel("default");
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
