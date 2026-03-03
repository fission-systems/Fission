/**
 * Fission Decompiler Core Implementation
 */

#include "fission/decompiler/DecompilationCore.h"
#include "fission/core/ArchInit.h"
#include "fission/types/PrototypeEnforcer.h"
#include "fission/decompiler/PostProcessPipeline.h"
#include "fission/analysis/CallingConvDetector.h"
#include "fission/analysis/TypePropagator.h"
#include "fission/decompiler/AnalysisPipeline.h"
#include "libdecomp.hh"
#include "address.hh"
#include "block.hh"
#include "funcdata.hh"
#include "op.hh"
#include "override.hh"
#include "varnode.hh"

#include <iostream>
#include <set>
#include "fission/utils/logger.h"
using namespace fission::ffi;
using namespace fission::core;
using namespace fission::types;
using namespace fission::analysis;

static constexpr size_t MAX_FUNCTION_SIZE = 10000;

// ============================================================================
// Known noreturn functions — marking these allows Ghidra's FlowInfo to insert
// artificial halts after calls, eliminating dead code in the decompiled output.
// ============================================================================
static const std::set<std::string> KNOWN_NORETURN_FUNCTIONS = {
    // C / POSIX
    "exit", "_exit", "_Exit", "abort", "quick_exit",
    "__assert_fail", "__assert_rtn", "__assert",
    "__stack_chk_fail", "__fortify_fail",
    // POSIX / BSD
    "pthread_exit", "err", "errx", "verr", "verrx",
    // C++ exceptions
    "__cxa_throw", "__cxa_rethrow", "__cxa_bad_cast", "__cxa_bad_typeid",
    "__cxa_call_terminate", "__cxa_call_unexpected",
    "__cxa_pure_virtual", "__cxa_deleted_virtual",
    // GCC / Clang builtins
    "__builtin_abort", "__builtin_unreachable", "__builtin_trap",
    // setjmp/longjmp
    "longjmp", "_longjmp", "siglongjmp",
    // Windows CRT
    "ExitProcess", "TerminateProcess", "FatalExit",
    "RaiseException", "_CxxThrowException",
    // Common wrappers
    "__halt", "__stop",
};

/// Strip common decoration from a function name for noreturn lookup.
static std::string strip_for_noreturn(const std::string& name) {
    std::string s = name;
    // Remove leading underscore (_exit -> exit)
    if (!s.empty() && s[0] == '_' && s.size() > 1 && s[1] != '_') {
        s = s.substr(1);
    }
    // Remove @N suffix (stdcall decoration: _exit@4 -> exit)
    auto at = s.find('@');
    if (at != std::string::npos) {
        s = s.substr(0, at);
    }
    return s;
}

/// Mark known noreturn functions in the Ghidra scope so that FlowInfo
/// inserts artificial halts after calls to them.
static void mark_noreturn_functions(
    DecompContext* ctx,
    const std::map<uint64_t, std::string>& symbols
) {
    if (!ctx || !ctx->arch || !ctx->arch->symboltab) return;

    ghidra::Scope* global = ctx->arch->symboltab->getGlobalScope();
    ghidra::AddrSpace* code_space = ctx->arch->getDefaultCodeSpace();
    if (!global || !code_space) return;

    int count = 0;
    for (const auto& [addr, name] : symbols) {
        // Check both the raw name and the stripped version.
        bool matched = KNOWN_NORETURN_FUNCTIONS.count(name) > 0;
        if (!matched) {
            std::string stripped = strip_for_noreturn(name);
            matched = KNOWN_NORETURN_FUNCTIONS.count(stripped) > 0;
        }
        if (!matched) continue;

        ghidra::Address ga(code_space, addr);
        ghidra::Funcdata* fd_target = global->findFunction(ga);
        if (!fd_target) {
            // Create a stub so the flow analysis knows about this symbol.
            ghidra::FunctionSymbol* sym = global->addFunction(ga, name);
            fd_target = sym ? sym->getFunction() : nullptr;
        }
        if (fd_target && !fd_target->getFuncProto().isNoReturn()) {
            fd_target->getFuncProto().setNoReturn(true);
            ++count;
            fission::utils::log_stream()
                << "[NoReturn] Marked " << name << " @ 0x"
                << std::hex << addr << std::dec << std::endl;
        }
    }

    if (count > 0) {
        fission::utils::log_stream()
            << "[NoReturn] Total: " << count << " functions marked" << std::endl;
    }
}

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

// ============================================================================
// Helper Functions
// ============================================================================

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
    
    fission::utils::log_stream() << "[DecompilerCore] Starting decompilation at 0x" << std::hex << addr << std::dec << std::endl;
    
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
    
    fission::utils::log_stream() << "[DecompilerCore] Global scope in decompilation: " << (void*)global_scope << std::endl;
    
    // Create function address
    ghidra::AddrSpace* code_space = ctx->arch->getDefaultCodeSpace();
    if (!code_space) {
        throw std::runtime_error("Code space not initialized");
    }
    ghidra::Address start_addr(code_space, addr);
    
    fission::utils::log_stream() << "[DecompilerCore] Requesting decompilation for addr: 0x" << std::hex << addr << std::dec << std::endl;
    fission::utils::log_stream() << "[DecompilerCore] Created Address object: " << start_addr.getShortcut() << std::endl;
    start_addr.printRaw(fission::utils::log_stream()); fission::utils::log_stream() << std::endl;

    fission::utils::log_stream() << "[DecompilerCore] Looking up function at code space=" 
              << code_space->getName() << ", addr=0x" << std::hex << addr << std::dec << std::endl;
    
    // Check if function exists at address
    ghidra::Funcdata* fd = global_scope->findFunction(start_addr);
    if (!fd) {
        // Check if we have a registered name for this address
        std::string func_name;
        auto it = ctx->symbols.find(addr);
        if (it != ctx->symbols.end()) {
            func_name = it->second;
            fission::utils::log_stream() << "[DecompilerCore] Found registered name for 0x" << std::hex << addr << std::dec << ": " << func_name << std::endl;
        } else {
            // Generate name
            std::ostringstream name_ss;
            name_ss << "sub_" << std::hex << addr;
            func_name = name_ss.str();
            fission::utils::log_stream() << "[DecompilerCore] No registered name, using: " << func_name << std::endl;
        }
        
        ghidra::FunctionSymbol* sym = global_scope->addFunction(start_addr, func_name);
        if (!sym) {
            throw std::runtime_error("Failed to add function");
        }
        fd = sym->getFunction();
        fission::utils::log_stream() << "[DecompilerCore] Created new function at 0x" << std::hex << addr << std::dec << " with name: " << func_name << std::endl;
    } else {
        fission::utils::log_stream() << "[DecompilerCore] Found existing function at 0x" << std::hex << addr << std::dec << ": " << fd->getName() << std::endl;
    }
    
    if (!fd) {
        throw std::runtime_error("Failed to get function data");
    }
    
    // By default we force standalone decompilation for inline-marked functions,
    // but this can be relaxed via feature: allow_inline / inline.
    if (fd->getFuncProto().isInline() && !ctx->allow_inline) {
        fission::utils::log_stream() << "[DecompilerCore] WARNING: Function at 0x" << std::hex << addr << std::dec
                  << " is marked inline; forcing standalone decompilation" << std::endl;
        fd->getFuncProto().setInline(false);
    }
    
    // Check if function is already being decompiled (recursive call)
    if (fd->isProcStarted()) {
        fission::utils::log_stream() << "[DecompilerCore] WARNING: Function at 0x" << std::hex << addr << std::dec << " is already being processed" << std::endl;
        throw std::runtime_error("Function is already being decompiled (recursive decompilation detected)");
    }
    
    // Clear only this function's data for fresh analysis
    fd->clear();
    
    fission::utils::log_stream() << "[DecompilerCore] Following control flow..." << std::endl;
    
    // Debug: Check if we can read memory at this address
    uint8_t test_byte;
    try {
        ctx->memory_image->loadFill(&test_byte, 1, start_addr);
        fission::utils::log_stream() << "[DecompilerCore] Successfully read first byte at 0x" << std::hex << addr << ": 0x" << (int)test_byte << std::dec << std::endl;
        // If first byte is 0x00, the address is likely not mapped properly
        if (test_byte == 0x00) {
            fission::utils::log_stream() << "[DecompilerCore] WARNING: First byte is 0x00 at 0x" << std::hex << addr << std::dec << ", address may be unmapped" << std::endl;
        }
    } catch (const std::exception& e) {
        fission::utils::log_stream() << "[DecompilerCore] ERROR: Cannot read memory at 0x" << std::hex << addr << std::dec << ": " << e.what() << std::endl;
        return "// Error: Cannot read memory at address 0x" + ([&]() {
            std::ostringstream s; s << std::hex << addr; return s.str();
        })() + "\n// " + e.what() + "\n";
    }
    
    // CRITICAL: Follow control flow to discover instructions
    // Higher range (32KB) to handle large functions. 
    // Ghidra's followFlow will stop at returns anyway.
    ghidra::Address end_addr = start_addr + 0x8000;
    bool follow_flow_ok = false;
    try {
        fd->followFlow(start_addr, end_addr);
        fission::utils::log_stream() << "[DecompilerCore] Control flow analysis complete" << std::endl;
        follow_flow_ok = true;
    } catch (const std::exception& e) {
        fission::utils::log_stream() << "[DecompilerCore] ERROR in followFlow: " << e.what() << std::endl;
    } catch (...) {
        fission::utils::log_stream() << "[DecompilerCore] ERROR: Unknown exception in followFlow" << std::endl;
    }

    // If control flow analysis failed, do NOT proceed to decompilation
    // (action->perform on empty function data causes hangs)
    if (!follow_flow_ok) {
        std::ostringstream err;
        err << "// Decompilation failed: control flow analysis error\n"
            << "// Function: " << fd->getName() << "\n"
            << "// Address: 0x" << std::hex << addr << "\n"
            << "// The function at this address could not be analyzed.\n"
            << "// Possible causes: unmapped memory, invalid entry point, or corrupted code.\n";
        return err.str();
    }

    // TAIL-CALL OVERRIDE REMOVED: It was causing recursive stubs for correctly followed functions.
    // Ghidra's action pipeline handles tail-calls better than our manual p-code override.
    
    // ========================================================================
    // Calling Convention Detection + Application
    // ========================================================================
    try {
        fission::analysis::CallingConvDetector detector(ctx->arch.get());
        // Provide binary format hint so the detector can adjust heuristics
        // and choose the correct fallback when detection is ambiguous.
        detector.set_format_hint(ctx->compiler_id);
        auto conv = detector.detect(fd);
        if (conv == fission::analysis::CallingConvDetector::CONV_UNKNOWN) {
            if (ctx->is_64bit) {
                // Use compiler_id / binary format to pick the correct 64-bit ABI.
                // PE/windows -> MS x64 (__fastcall), ELF/Mach-O -> SYSV x64.
                const auto& cid = ctx->compiler_id;
                conv = (cid == "windows")
                    ? fission::analysis::CallingConvDetector::CONV_MS_X64
                    : fission::analysis::CallingConvDetector::CONV_SYSV_X64;
            } else {
                conv = fission::analysis::CallingConvDetector::CONV_CDECL;
            }
        }
        detector.apply(fd, conv);
    } catch (const std::exception& e) {
        fission::utils::log_stream() << "[DecompilerCore] ERROR applying calling convention: " << e.what() << std::endl;
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

        // Fallback to the actual function symbol name when the address is not in
        // import/global maps (common for internal/user functions like cpp_*).
        if (func_name.empty() && fd) {
            func_name = fd->getName();
        }

        if (!func_name.empty()) {
            proto_enforcer.enforce_single_prototype(ctx->arch.get(), addr, func_name);
        }
    }

    // ========================================================================
    // noreturn auto-marking — must run AFTER prototype enforcement, BEFORE
    // clearAnalysis/reset so that FlowInfo sees the flags during perform().
    // ========================================================================
    {
        mark_noreturn_functions(ctx, ctx->symbols);
        mark_noreturn_functions(ctx, ctx->global_symbols);
    }

    // CRITICAL: Reset action state for this function AFTER prototypes are applied
    fission::utils::log_stream() << "[DecompilerCore] Resetting action state..." << std::endl;
    ctx->arch->clearAnalysis(fd);
    current_action->reset(*fd);

    // P1-A: Seed Ghidra's type recommendation system BEFORE action->perform()
    // so that ActionInferTypes picks up API-derived types in its own loop.
    {
        fission::analysis::TypePropagator seeder(ctx->arch.get(), &ctx->struct_registry);
        seeder.set_compiler_id(ctx->compiler_id.empty() ? "windows" : ctx->compiler_id);
        seeder.seed_before_action(fd);
    }

    fission::utils::log_stream() << "[DecompilerCore] Performing decompilation..." << std::endl;
    
    // Perform decompilation
    try {
        current_action->perform(*fd);
    } catch (const ghidra::LowlevelError& e) {
        std::string msg = e.explain;
        if (msg.find("Function loaded for inlining") != std::string::npos && !ctx->allow_inline) {
            fission::utils::log_stream() << "[DecompilerCore] WARNING: Inline-loaded function, clearing analysis and retrying"
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
    
    fission::utils::log_stream() << "[DecompilerCore] Generating output..." << std::endl;
    
    // Check print language
    if (!ctx->arch->print) {
        throw std::runtime_error("Print language not initialized");
    }
    
    // Print decompiled output to string
    std::ostringstream ss;
    ctx->arch->print->setOutputStream(&ss);
    ctx->arch->print->docFunction(fd);
    
    std::string result = ss.str();
    
    // ========================================================================
    // Full Post-Processing Chain
    // ========================================================================
    // Use per-context configurable options (set via set_feature with pp_ prefix)
    const PostProcessOptions& options = ctx->post_process_options;

    // Use the analysis artifacts gathered earlier for post-processing
    result = run_post_processing(ctx, fd, result, analysis, options);
    
    fission::utils::log_stream() << "[DecompilerCore] Decompilation complete, " << result.size() << " bytes after post-processing" << std::endl;
    
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
