/**
 * Fission Decompiler Core Implementation
 */

#include "fission/decompiler/DecompilationCore.h"
#include "fission/core/ArchInit.h"
#include "fission/types/PrototypeEnforcer.h"
#include "fission/decompiler/PostProcessPipeline.h"
#include "fission/analysis/CallingConvDetector.h"
#include "fission/decompiler/AnalysisPipeline.h"
#include "libdecomp.hh"
#include "address.hh"
#include "block.hh"
#include "funcdata.hh"
#include "op.hh"
#include "override.hh"
#include "varnode.hh"

#include <iostream>
using namespace fission::ffi;
using namespace fission::core;
using namespace fission::types;
using namespace fission::analysis;

static constexpr size_t MAX_FUNCTION_SIZE = 10000;

// Helper function to escape strings for JSON output
static std::string json_escape(const std::string& input) {
    std::string output;
    output.reserve(input.size() + 10);
    for (char c : input) {
        switch (c) {
            case '\"': output += "\\\""; break;
            case '\\': output += "\\\\"; break;
            case '\b': output += "\\b"; break;
            case '\f': output += "\\f"; break;
            case '\n': output += "\\n"; break;
            case '\r': output += "\\r"; break;
            case '\t': output += "\\t"; break;
            default:
                if (static_cast<unsigned char>(c) < 0x20) {
                    // Control characters - output as \uXXXX
                    char buf[8];
                    snprintf(buf, sizeof(buf), "\\u%04x", static_cast<unsigned char>(c));
                    output += buf;
                } else {
                    output += c;
                }
                break;
        }
    }
    return output;
}

static bool apply_tailcall_flow_overrides(
    fission::ffi::DecompContext* ctx,
    ghidra::Funcdata* fd
) {
    if (!ctx || !fd || !ctx->arch || !ctx->arch->symboltab) {
        return false;
    }

    ghidra::Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
    if (!global_scope) {
        return false;
    }

    ghidra::AddrSpace* code_space = ctx->arch->getDefaultCodeSpace();
    if (!code_space) {
        return false;
    }

    bool applied = false;
    for (auto it = fd->beginOpAll(); it != fd->endOpAll(); ++it) {
        ghidra::PcodeOp* op = it->second;
        if (!op || op->code() != ghidra::CPUI_BRANCH) {
            continue;
        }
        if (op->getParent() && op->getParent()->lastOp() != op) {
            continue;
        }
        ghidra::Varnode* dest = op->getIn(0);
        if (!dest) {
            continue;
        }

        ghidra::Address dest_addr = dest->getAddr();
        if (dest_addr.isInvalid()) {
            continue;
        }

        uint64_t target_offset = dest_addr.getOffset();
        if (dest_addr.getSpace() != code_space && !dest->isConstant()) {
            continue;
        }
        if (target_offset == fd->getAddress().getOffset()) {
            continue;
        }

        ghidra::Address target(code_space, target_offset);
        ghidra::Funcdata* target_fd = global_scope->findFunction(target);
        if (!target_fd || target_fd == fd) {
            continue;
        }

        fd->getOverride().insertFlowOverride(op->getAddr(), ghidra::Override::CALL_RETURN);
        applied = true;
    }

    return applied;
}

// ============================================================================
// Helper Functions
// ============================================================================

// ============================================================================
// Public API
// ============================================================================

void fission::decompiler::ensure_architecture(DecompContext* ctx) {
    fission::core::initialize_architecture(ctx);
}

std::string fission::decompiler::run_decompilation(DecompContext* ctx, uint64_t addr) {
    if (!ctx->memory_image) {
        throw std::runtime_error("No binary loaded");
    }
    
    ensure_architecture(ctx);
    
    std::cerr << "[DecompilerCore] Starting decompilation at 0x" << std::hex << addr << std::dec << std::endl;
    
    // Validate architecture components
    if (!ctx->arch) {
        throw std::runtime_error("Architecture not initialized");
    }
    if (!ctx->arch->symboltab) {
        throw std::runtime_error("Symbol table not initialized");
    }
    
    // Get global scope
    ghidra::Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
    if (!global_scope) {
        throw std::runtime_error("Global scope not initialized");
    }
    
    std::cerr << "[DecompilerCore] Global scope in decompilation: " << (void*)global_scope << std::endl;
    
    // Create function address
    ghidra::AddrSpace* code_space = ctx->arch->getDefaultCodeSpace();
    if (!code_space) {
        throw std::runtime_error("Code space not initialized");
    }
    ghidra::Address start_addr(code_space, addr);
    
    std::cerr << "[DecompilerCore] Requesting decompilation for addr: 0x" << std::hex << addr << std::dec << std::endl;
    std::cerr << "[DecompilerCore] Created Address object: " << start_addr.getShortcut() << std::endl;
    start_addr.printRaw(std::cerr); std::cerr << std::endl;

    std::cerr << "[DecompilerCore] Looking up function at code space=" 
              << code_space->getName() << ", addr=0x" << std::hex << addr << std::dec << std::endl;
    
    // Check if function exists at address
    ghidra::Funcdata* fd = global_scope->findFunction(start_addr);
    if (!fd) {
        // Check if we have a registered name for this address
        std::string func_name;
        auto it = ctx->symbols.find(addr);
        if (it != ctx->symbols.end()) {
            func_name = it->second;
            std::cerr << "[DecompilerCore] Found registered name for 0x" << std::hex << addr << std::dec << ": " << func_name << std::endl;
        } else {
            // Generate name
            std::ostringstream name_ss;
            name_ss << "sub_" << std::hex << addr;
            func_name = name_ss.str();
            std::cerr << "[DecompilerCore] No registered name, using: " << func_name << std::endl;
        }
        
        ghidra::FunctionSymbol* sym = global_scope->addFunction(start_addr, func_name);
        if (!sym) {
            throw std::runtime_error("Failed to add function");
        }
        fd = sym->getFunction();
        std::cerr << "[DecompilerCore] Created new function at 0x" << std::hex << addr << std::dec << " with name: " << func_name << std::endl;
    } else {
        std::cerr << "[DecompilerCore] Found existing function at 0x" << std::hex << addr << std::dec << ": " << fd->getName() << std::endl;
    }
    
    if (!fd) {
        throw std::runtime_error("Failed to get function data");
    }
    
    // If function is marked inline, warn and clear inline flag for standalone decompilation
    if (fd->getFuncProto().isInline()) {
        std::cerr << "[DecompilerCore] WARNING: Function at 0x" << std::hex << addr << std::dec
                  << " is marked inline; forcing standalone decompilation" << std::endl;
        fd->getFuncProto().setInline(false);
    }
    
    // Check if function is already being decompiled (recursive call)
    if (fd->isProcStarted()) {
        std::cerr << "[DecompilerCore] WARNING: Function at 0x" << std::hex << addr << std::dec << " is already being processed" << std::endl;
        throw std::runtime_error("Function is already being decompiled (recursive decompilation detected)");
    }
    
    // Clear only this function's data for fresh analysis
    fd->clear();
    
    std::cerr << "[DecompilerCore] Following control flow..." << std::endl;
    
    // Debug: Check if we can read memory at this address
    uint8_t test_byte;
    try {
        ctx->memory_image->loadFill(&test_byte, 1, start_addr);
        std::cerr << "[DecompilerCore] Successfully read first byte at 0x" << std::hex << addr << ": 0x" << (int)test_byte << std::dec << std::endl;
    } catch (const std::exception& e) {
        std::cerr << "[DecompilerCore] ERROR: Cannot read memory at 0x" << std::hex << addr << std::dec << ": " << e.what() << std::endl;
    }
    
    // CRITICAL: Follow control flow to discover instructions
    ghidra::Address end_addr = start_addr + 0x1000;
    try {
        fd->followFlow(start_addr, end_addr);
        std::cerr << "[DecompilerCore] Control flow analysis complete" << std::endl;
    } catch (const std::exception& e) {
        std::cerr << "[DecompilerCore] ERROR in followFlow: " << e.what() << std::endl;
    } catch (...) {
        std::cerr << "[DecompilerCore] ERROR: Unknown exception in followFlow" << std::endl;
    }

    if (apply_tailcall_flow_overrides(ctx, fd)) {
        std::cerr << "[DecompilerCore] Applied tail-call flow overrides; restarting flow analysis"
                  << std::endl;
        if (ctx->arch) {
            ctx->arch->clearAnalysis(fd);
        } else {
            fd->clear();
        }
        try {
            fd->followFlow(start_addr, end_addr);
            std::cerr << "[DecompilerCore] Control flow analysis complete (override pass)" << std::endl;
        } catch (const std::exception& e) {
            std::cerr << "[DecompilerCore] ERROR in followFlow (override pass): " << e.what() << std::endl;
        } catch (...) {
            std::cerr << "[DecompilerCore] ERROR: Unknown exception in followFlow (override pass)" << std::endl;
        }
    }
    
    // ========================================================================
    // Calling Convention Detection + Application
    // ========================================================================
    try {
        fission::analysis::CallingConvDetector detector(ctx->arch.get());
        auto conv = detector.detect(fd);
        if (conv == fission::analysis::CallingConvDetector::CONV_UNKNOWN) {
            conv = ctx->is_64bit
                ? fission::analysis::CallingConvDetector::CONV_MS_X64
                : fission::analysis::CallingConvDetector::CONV_CDECL;
        }
        detector.apply(fd, conv);
    } catch (const std::exception& e) {
        std::cerr << "[DecompilerCore] ERROR applying calling convention: " << e.what() << std::endl;
    }
    
    // Check action group
    ghidra::Action* current_action = ctx->arch->allacts.getCurrent();
    if (!current_action) {
        throw std::runtime_error("No current action group");
    }

    // Enforce GDT-based and built-in prototypes before action reset.
    {
        fission::types::PrototypeEnforcer proto_enforcer;
        if (!ctx->symbols.empty()) {
            proto_enforcer.enforce_iat_prototypes(ctx->arch.get(), ctx->symbols);
        }

        std::string func_name;
        auto it = ctx->symbols.find(addr);
        if (it != ctx->symbols.end()) {
            func_name = it->second;
        } else {
            auto it_global = ctx->global_symbols.find(addr);
            if (it_global != ctx->global_symbols.end()) {
                func_name = it_global->second;
            }
        }

        if (!func_name.empty()) {
            proto_enforcer.enforce_single_prototype(ctx->arch.get(), addr, func_name);
        }
    }

    // CRITICAL: Reset action state for this function AFTER prototypes are applied
    std::cerr << "[DecompilerCore] Resetting action state..." << std::endl;
    current_action->reset(*fd);
    
    std::cerr << "[DecompilerCore] Performing decompilation..." << std::endl;
    
    // Perform decompilation
    try {
        current_action->perform(*fd);
    } catch (const ghidra::LowlevelError& e) {
        std::string msg = e.explain;
        if (msg.find("Function loaded for inlining") != std::string::npos) {
            std::cerr << "[DecompilerCore] WARNING: Inline-loaded function, clearing analysis and retrying"
                      << std::endl;
            if (ctx->arch) {
                ctx->arch->clearAnalysis(fd);
            } else {
                fd->clear();
            }
            fd->getFuncProto().setInline(false);
            current_action->reset(*fd);
            current_action->perform(*fd);
        } else {
            throw std::runtime_error("Ghidra LowlevelError: " + e.explain);
        }
    } catch (const std::exception& e) {
        throw;
    } catch (...) {
        throw std::runtime_error("Unknown error during decompilation");
    }

    fission::decompiler::AnalysisArtifacts analysis =
        fission::decompiler::run_analysis_passes(ctx, fd, current_action, MAX_FUNCTION_SIZE);
    
    std::cerr << "[DecompilerCore] Generating output..." << std::endl;
    
    // Check print language
    if (!ctx->arch->print) {
        throw std::runtime_error("Print language not initialized");
    }
    
    // Print decompiled output to string
    std::ostringstream ss;
    ctx->arch->print->setOutputStream(&ss);
    ctx->arch->print->docFunction(fd);
    
    std::string result = ss.str();
    
    std::cerr << "[DecompilerCore] Raw output: " << result.size() << " bytes, post-processing..." << std::endl;

    // ========================================================================
    // Full Post-Processing Chain
    // ========================================================================
    fission::decompiler::PostProcessOptions options;
    result = fission::decompiler::run_post_processing(ctx, fd, result, analysis, options);
    
    std::cerr << "[DecompilerCore] Decompilation complete, " << result.size() << " bytes after post-processing" << std::endl;
    
    return result;
}

// Simple AssemblyEmit implementation for capturing disassembly
class SimpleAssemblyEmit : public ghidra::AssemblyEmit {
    std::string mnemonic_;
    std::string body_;
    
public:
    virtual void dump(const ghidra::Address& addr, const std::string& mnem, const std::string& body) override {
        mnemonic_ = mnem;
        body_ = body;
    }
    
    const std::string& getMnemonic() const { return mnemonic_; }
    const std::string& getBody() const { return body_; }
};

std::string fission::decompiler::run_decompilation_pcode(DecompContext* ctx, uint64_t addr) {
    if (!ctx) return "{}";
    
    ensure_architecture(ctx);
    
    if (!ctx->arch->symboltab) throw std::runtime_error("Symbol table not initialized");
    ghidra::Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
    if (!global_scope) throw std::runtime_error("Global scope not initialized");
    
    ghidra::AddrSpace* code_space = ctx->arch->getDefaultCodeSpace();
    if (!code_space) throw std::runtime_error("Code space not initialized");
    ghidra::Address start_addr(code_space, addr);
    
    ghidra::Funcdata* fd = global_scope->findFunction(start_addr);
    if (!fd) {
        std::string func_name = "sub_" + std::to_string(addr);
        ghidra::FunctionSymbol* sym = global_scope->addFunction(start_addr, func_name);
        if (!sym) throw std::runtime_error("Failed to add function");
        fd = sym->getFunction();
    }
    
    if (!fd) throw std::runtime_error("Failed to get function data");
    
    fd->clear();
    
    ghidra::Address end_addr = start_addr + 0x10000;
    try {
        fd->followFlow(start_addr, end_addr);
    } catch (...) {}
    
    ghidra::Action* current_action = ctx->arch->allacts.getCurrent();
    if (!current_action) throw std::runtime_error("No current action group");
    
    try {
        // Clear only this function's data for fresh analysis
        fd->clear();
        
        // Follow control flow to discover instructions
        // We use a reasonable limit for the end address
        ghidra::Address end_addr = start_addr + 0x10000; 
        fd->followFlow(start_addr, end_addr);
        
        current_action->reset(*fd);
        current_action->perform(*fd);
        
        std::ostringstream json;
        json << "{";
        json << "\"blocks\": [";
        
        const ghidra::BlockGraph& basic_blocks = fd->getBasicBlocks();
        for (int i = 0; i < basic_blocks.getSize(); ++i) {
            ghidra::FlowBlock* block = basic_blocks.getBlock(i);
            ghidra::BlockBasic* bb = static_cast<ghidra::BlockBasic*>(block);
            
            if (i > 0) json << ",";
            
            json << "{";
            json << "\"index\": " << block->getIndex() << ",";
            json << "\"start_addr\": \"0x" << std::hex << block->getStart().getOffset() << "\",";
            json << "\"ops\": [";
            
            bool first_op = true;
            auto iter = bb->beginOp();
            auto end_iter = bb->endOp();
            
            for (; iter != end_iter; ++iter) {
                ghidra::PcodeOp* op = *iter;
                if (!op) continue;
                
                if (!first_op) json << ",";
                first_op = false;
                
                json << "{";
                json << "\"seq\": " << std::dec << op->getSeqNum().getTime() << ",";
                json << "\"opcode\": \"" << json_escape(op->getOpcode()->getName()) << "\",";
                json << "\"addr\": \"0x" << std::hex << op->getAddr().getOffset() << std::dec << "\",";
                
                // Try to get assembly mnemonic
                try {
                    ghidra::Address asm_addr = op->getAddr();
                    SimpleAssemblyEmit asm_emit;
                    ctx->arch->translate->printAssembly(asm_emit, asm_addr);
                    std::string mnemonic = asm_emit.getMnemonic();
                    std::string body = asm_emit.getBody();
                    if (!mnemonic.empty()) {
                        if (!body.empty()) {
                            json << "\"asm\": \"" << json_escape(mnemonic) << " " << json_escape(body) << "\",";
                        } else {
                            json << "\"asm\": \"" << json_escape(mnemonic) << "\",";
                        }
                    } else {
                        json << "\"asm\": null,";
                    }
                } catch (...) {
                    json << "\"asm\": null,";
                }
                
                ghidra::Varnode* out = op->getOut();
                if (out) {
                    json << "\"output\": {";
                    json << "\"offset\": \"0x" << std::hex << out->getOffset() << "\",";
                    json << "\"size\": " << std::dec << out->getSize() << ",";
                    json << "\"space\": " << out->getSpace()->getType() << ","; // Use type ID for space
                    json << "\"const_val\": " << (out->isConstant() ? std::to_string(out->getOffset()) : "null");
                    json << "},";
                } else {
                    json << "\"output\": null,";
                }
                
                json << "\"inputs\": [";
                for (int j = 0; j < op->numInput(); ++j) {
                    ghidra::Varnode* in = op->getIn(j);
                    if (j > 0) json << ",";
                    json << "{";
                    json << "\"offset\": \"0x" << std::hex << in->getOffset() << "\",";
                    json << "\"size\": " << std::dec << in->getSize() << ",";
                    json << "\"space\": " << in->getSpace()->getType() << ",";
                    json << "\"const_val\": " << (in->isConstant() ? std::to_string(in->getOffset()) : "null");
                    json << "}";
                }
                json << "]";
                json << "}";
            }
            json << "]";
            json << "}";
        }
        
        json << "]";
        json << "}";
        
        return json.str();
    } catch (const ghidra::LowlevelError& e) {
        throw std::runtime_error("Ghidra LowlevelError: " + e.explain);
    } catch (const std::exception& e) {
        throw std::runtime_error("Decompilation error: " + std::string(e.what()));
    } catch (...) {
        throw std::runtime_error("Unknown decompilation error in run_decompilation_pcode");
    }
}
