/**
 * Fission Decompiler Core Implementation
 */

#include "fission/decompiler/DecompilationCore.h"
#include "fission/core/ArchInit.h"
#include "fission/types/PrototypeEnforcer.h"
#include "fission/decompiler/Limits.h"
#include "fission/decompiler/PostProcessPipeline.h"
#include "fission/analysis/CallingConvDetector.h"
#include "fission/analysis/TypePropagator.h"
#include "fission/decompiler/AnalysisPipeline.h"
#include "fission/utils/json_utils.h"
#include "libdecomp.hh"
#include "error.hh"
#include "address.hh"
#include "block.hh"
#include "funcdata.hh"
#include "op.hh"
#include "override.hh"
#include "varnode.hh"

#include <algorithm>
#include <cctype>
#include <chrono>
#include <iomanip>
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
            fission::utils::log_output()
                << "[NoReturn] Marked " << name << " @ 0x"
                << std::hex << addr << std::dec << std::endl;
        }
    }

    if (count > 0) {
        fission::utils::log_output()
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

static double elapsed_ms(std::chrono::steady_clock::time_point start) {
    return std::chrono::duration<double, std::milli>(
        std::chrono::steady_clock::now() - start
    ).count();
}

static void tighten_follow_flow_bound(uint64_t addr, uint64_t candidate, uint64_t& bound) {
    if (candidate > addr && candidate < bound) {
        bound = candidate;
    }
}

static bool is_executable_address(DecompContext* ctx, uint64_t addr) {
    if (ctx == nullptr) {
        return false;
    }
    for (const auto& block : ctx->memory_blocks) {
        if (!block.is_executable) {
            continue;
        }
        uint64_t size = block.va_size > 0 ? block.va_size : block.file_size;
        if (size == 0) {
            continue;
        }
        uint64_t block_start = block.va_addr;
        uint64_t block_end = block_start + size;
        if (addr >= block_start && addr < block_end) {
            return true;
        }
    }
    return false;
}

static bool is_probable_function_symbol(const std::string& name) {
    if (name.empty()) {
        return false;
    }

    static const char* k_non_function_prefixes[] = {
        "DAT_",
        "LAB_",
        "UNK_",
        "PTR_",
        "caseD_",
        "switchD_",
        "g_",
        "gp_",
    };
    for (const char* prefix : k_non_function_prefixes) {
        if (name.rfind(prefix, 0) == 0) {
            return false;
        }
    }

    if (name.rfind("FUN_", 0) == 0 || name.rfind("sub_", 0) == 0) {
        return true;
    }

    return std::isalpha(static_cast<unsigned char>(name.front())) != 0;
}

static size_t compute_follow_flow_limit(DecompContext* ctx, uint64_t addr) {
    uint64_t bound = addr + fission::decompiler::k_follow_flow_limit;

    if (ctx != nullptr) {
        for (const auto& kv : ctx->symbols) {
            if (is_probable_function_symbol(kv.second) &&
                is_executable_address(ctx, kv.first)) {
                tighten_follow_flow_bound(addr, kv.first, bound);
            }
        }
        for (const auto& kv : ctx->global_symbols) {
            if (is_probable_function_symbol(kv.second) &&
                is_executable_address(ctx, kv.first)) {
                tighten_follow_flow_bound(addr, kv.first, bound);
            }
        }
        for (const auto& block : ctx->memory_blocks) {
            if (!block.is_executable) {
                continue;
            }
            uint64_t size = block.va_size > 0 ? block.va_size : block.file_size;
            if (size == 0) {
                continue;
            }
            uint64_t block_start = block.va_addr;
            uint64_t block_end = block_start + size;
            if (addr >= block_start && addr < block_end) {
                tighten_follow_flow_bound(addr, block_end, bound);
                break;
            }
        }
    }

    size_t limit = static_cast<size_t>(bound - addr);
    // Keep at least 32KB to avoid coverage regression (original FFI used 0x8000).
    limit = std::max<size_t>(limit, 0x8000);
    limit = std::min<size_t>(limit, fission::decompiler::k_follow_flow_limit);
    return limit;
}

static std::string make_native_timing_json(
    uint64_t addr,
    const fission::ffi::NativeDecompTiming& timing
) {
    std::ostringstream ss;
    ss << std::fixed << std::setprecision(3);
    ss << "{"
       << "\"address\":\"0x" << std::hex << addr << std::dec << "\","
       << "\"follow_flow_ms\":" << timing.follow_flow_ms << ","
       << "\"follow_flow_budget_bytes\":" << timing.follow_flow_budget_bytes << ","
       << "\"main_perform_ms\":" << timing.main_perform_ms << ","
       << "\"analysis_passes_ms\":" << timing.analysis_passes_ms << ","
       << "\"callee_preanalysis_ms\":" << timing.callee_preanalysis_ms << ","
       << "\"callgraph_reanalysis_ms\":" << timing.callgraph_reanalysis_ms << ","
       << "\"print_ms\":" << timing.print_ms << ","
       << "\"postprocess_ms\":" << timing.postprocess_ms << ","
       << "\"smart_constant_replace_ms\":" << timing.smart_constant_replace_ms << ","
       << "\"cfg_structurizer_ms\":" << timing.cfg_structurizer_ms << ","
       << "\"loop_normalize_ms\":" << timing.loop_normalize_ms << ","
       << "\"total_native_ms\":" << timing.total_native_ms << ","
       << "\"callee_preanalysis_count\":" << timing.callee_preanalysis_count << ","
       << "\"callgraph_reanalysis_count\":" << timing.callgraph_reanalysis_count << ","
       << "\"stage1_rerun_ms\":" << timing.stage1_rerun_ms << ","
       << "\"stage2_rerun_ms\":" << timing.stage2_rerun_ms
       << "}";
    return ss.str();
}

class ActiveDecompGuard {
public:
    ActiveDecompGuard(DecompContext* ctx, uint64_t addr) : ctx_(ctx), addr_(addr) {
        if (ctx_ != nullptr) {
            inserted_ = ctx_->active_decomp_addrs.insert(addr_).second;
        }
    }

    ~ActiveDecompGuard() {
        if (inserted_ && ctx_ != nullptr) {
            ctx_->active_decomp_addrs.erase(addr_);
        }
    }

    bool inserted() const { return inserted_; }

private:
    DecompContext* ctx_ = nullptr;
    uint64_t addr_ = 0;
    bool inserted_ = false;
};

class FuncdataCleanupGuard {
public:
    FuncdataCleanupGuard(ghidra::Architecture* arch, ghidra::Funcdata* fd)
        : arch_(arch), fd_(fd) {}

    ~FuncdataCleanupGuard() {
        if (fd_ == nullptr) {
            return;
        }
        if (arch_ != nullptr) {
            arch_->clearAnalysis(fd_);
        } else {
            fd_->clear();
        }
    }

private:
    ghidra::Architecture* arch_ = nullptr;
    ghidra::Funcdata* fd_ = nullptr;
};

class NativeTimingRecorder {
public:
    NativeTimingRecorder(DecompContext* ctx, uint64_t addr)
        : ctx_(ctx), addr_(addr), start_(std::chrono::steady_clock::now()) {
        if (ctx_ != nullptr) {
            ctx_->last_timing_json.clear();
            ctx_->last_native_timing = fission::ffi::NativeDecompTiming{};
        }
    }

    ~NativeTimingRecorder() {
        timing.total_native_ms = elapsed_ms(start_);
        if (ctx_ != nullptr) {
            ctx_->last_native_timing = timing;
            ctx_->last_timing_json = make_native_timing_json(addr_, timing);
        }
    }

    fission::ffi::NativeDecompTiming timing;

private:
    DecompContext* ctx_ = nullptr;
    uint64_t addr_ = 0;
    std::chrono::steady_clock::time_point start_;
};

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

std::string fission::decompiler::run_decompilation(DecompContext* ctx, uint64_t addr,
                                                   AnalysisArtifacts* out_artifacts) {
    if (!ctx->memory_image) {
        throw std::runtime_error("No binary loaded");
    }
    
    ensure_architecture(ctx);
    NativeTimingRecorder timing_recorder(ctx, addr);
    
    fission::utils::log_output() << "[DecompilerCore] Starting decompilation at 0x" << std::hex << addr << std::dec << std::endl;
    
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
    
    fission::utils::log_output() << "[DecompilerCore] Global scope in decompilation: " << (void*)global_scope << std::endl;
    
    // Create function address
    ghidra::AddrSpace* code_space = ctx->arch->getDefaultCodeSpace();
    if (!code_space) {
        throw std::runtime_error("Code space not initialized");
    }
    ghidra::Address start_addr(code_space, addr);

    if (ctx->active_decomp_addrs.count(addr) > 0) {
        throw std::runtime_error("Function is already being decompiled (recursive decompilation detected)");
    }
    ActiveDecompGuard active_guard(ctx, addr);
    if (!active_guard.inserted()) {
        throw std::runtime_error("Function is already being decompiled (recursive decompilation detected)");
    }
    
    fission::utils::log_output() << "[DecompilerCore] Requesting decompilation for addr: 0x" << std::hex << addr << std::dec << std::endl;
    fission::utils::log_output() << "[DecompilerCore] Created Address object: " << start_addr.getShortcut() << std::endl;
    start_addr.printRaw(fission::utils::log_output()); fission::utils::log_output() << std::endl;

    fission::utils::log_output() << "[DecompilerCore] Looking up function at code space=" 
              << code_space->getName() << ", addr=0x" << std::hex << addr << std::dec << std::endl;
    
    // Check if function exists at address
    ghidra::Funcdata* fd = global_scope->findFunction(start_addr);
    if (!fd) {
        // Check if we have a registered name for this address
        std::string func_name;
        auto it = ctx->symbols.find(addr);
        if (it != ctx->symbols.end()) {
            func_name = it->second;
            fission::utils::log_output() << "[DecompilerCore] Found registered name for 0x" << std::hex << addr << std::dec << ": " << func_name << std::endl;
        } else {
            // Generate name
            std::ostringstream name_ss;
            name_ss << "sub_" << std::hex << addr;
            func_name = name_ss.str();
            fission::utils::log_output() << "[DecompilerCore] No registered name, using: " << func_name << std::endl;
        }
        
        ghidra::FunctionSymbol* sym = global_scope->addFunction(start_addr, func_name);
        if (!sym) {
            throw std::runtime_error("Failed to add function");
        }
        fd = sym->getFunction();
        fission::utils::log_output() << "[DecompilerCore] Created new function at 0x" << std::hex << addr << std::dec << " with name: " << func_name << std::endl;
    } else {
        fission::utils::log_output() << "[DecompilerCore] Found existing function at 0x" << std::hex << addr << std::dec << ": " << fd->getName() << std::endl;
    }
    
    if (!fd) {
        throw std::runtime_error("Failed to get function data");
    }
    FuncdataCleanupGuard cleanup_guard(ctx->arch.get(), fd);
    
    // By default we force standalone decompilation for inline-marked functions,
    // but this can be relaxed via feature: allow_inline / inline.
    if (fd->getFuncProto().isInline() && !ctx->allow_inline) {
        fission::utils::log_output() << "[DecompilerCore] WARNING: Function at 0x" << std::hex << addr << std::dec
                  << " is marked inline; forcing standalone decompilation" << std::endl;
        fd->getFuncProto().setInline(false);
    }
    
    // Check if function is already being decompiled (recursive call)
    if (fd->isProcStarted()) {
        fission::utils::log_output()
            << "[DecompilerCore] WARNING: stale started state at 0x"
            << std::hex << addr << std::dec
            << ", clearing analysis instead of treating it as recursion"
            << std::endl;
        ctx->arch->clearAnalysis(fd);
    }
    
    // Clear only this function's data for fresh analysis
    fd->clear();
    
    fission::utils::log_output() << "[DecompilerCore] Following control flow..." << std::endl;
    
    // Debug: Check if we can read memory at this address
    uint8_t test_byte;
    try {
        ctx->memory_image->loadFill(&test_byte, 1, start_addr);
        fission::utils::log_output() << "[DecompilerCore] Successfully read first byte at 0x" << std::hex << addr << ": 0x" << (int)test_byte << std::dec << std::endl;
        // If first byte is 0x00, the address is likely not mapped properly
        if (test_byte == 0x00) {
            fission::utils::log_output() << "[DecompilerCore] WARNING: First byte is 0x00 at 0x" << std::hex << addr << std::dec << ", address may be unmapped" << std::endl;
        }
    } catch (const std::exception& e) {
        fission::utils::log_output() << "[DecompilerCore] ERROR: Cannot read memory at 0x" << std::hex << addr << std::dec << ": " << e.what() << std::endl;
        return "// Error: Cannot read memory at address 0x" + ([&]() {
            std::ostringstream s; s << std::hex << addr; return s.str();
        })() + "\n// " + e.what() + "\n";
    }
    
    // Align the FFI path with the batch path by tightening the followFlow
    // window to the next known symbol or executable block end.
    size_t follow_flow_limit = compute_follow_flow_limit(ctx, addr);
    ghidra::Address end_addr = start_addr + follow_flow_limit;
    bool follow_flow_ok = false;
    std::string follow_flow_error;
    auto follow_flow_start = std::chrono::steady_clock::now();
    timing_recorder.timing.follow_flow_budget_bytes = follow_flow_limit;
    try {
        fd->followFlow(start_addr, end_addr);
        fission::utils::log_output() << "[DecompilerCore] Control flow analysis complete" << std::endl;
        follow_flow_ok = true;
    } catch (const ghidra::LowlevelError& e) {
        follow_flow_error = e.explain;
        fission::utils::log_output() << "[DecompilerCore] followFlow LowlevelError: "
                  << e.explain << std::endl;
    } catch (const std::exception& e) {
        follow_flow_error = e.what();
        fission::utils::log_output() << "[DecompilerCore] ERROR in followFlow: " << e.what() << std::endl;
    } catch (...) {
        follow_flow_error = "unknown followFlow error";
        fission::utils::log_output() << "[DecompilerCore] ERROR: Unknown exception in followFlow" << std::endl;
    }
    timing_recorder.timing.follow_flow_ms = elapsed_ms(follow_flow_start);

    if (!follow_flow_ok) {
        const bool has_partial_flow =
            fd->beginOpAll() != fd->endOpAll() || fd->getBasicBlocks().getSize() > 0;
        if (has_partial_flow) {
            fission::utils::log_output()
                << "[DecompilerCore] Continuing with partial control-flow graph after followFlow failure"
                << std::endl;
            follow_flow_ok = true;
        } else {
            std::ostringstream err;
            err << "// Decompilation failed: control flow analysis error\n"
                << "// Function: " << fd->getName() << "\n"
                << "// Address: 0x" << std::hex << addr << "\n"
                << "// The function at this address could not be analyzed.\n"
                << "// Possible causes: unmapped memory, invalid entry point, or corrupted code.\n";
            if (!follow_flow_error.empty()) {
                err << "// followFlow error: " << follow_flow_error << "\n";
            }
            return err.str();
        }
    }

    // TAIL-CALL OVERRIDE REMOVED: It was causing recursive stubs for correctly followed functions.
    // Ghidra's action pipeline handles tail-calls better than our manual p-code override.
    
    // ========================================================================
    // Calling Convention Detection + Application
    // ========================================================================
    try {
        /*
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
        */
    } catch (const std::exception& e) {
        fission::utils::log_output() << "[DecompilerCore] ERROR applying calling convention: " << e.what() << std::endl;
    }
    
    // Check action group
    ghidra::Action* current_action = ctx->arch->allacts.getCurrent();
    if (!current_action) {
        throw std::runtime_error("No current action group");
    }

    // Enforce GDT-based, injected (fission-signatures), and built-in prototypes before action reset.
    {
        fission::types::PrototypeEnforcer proto_enforcer;
        const auto* injected = ctx->injected_signatures.empty() ? nullptr : &ctx->injected_signatures;
        if (!ctx->symbols.empty()) {
            proto_enforcer.enforce_iat_prototypes(ctx->arch.get(), ctx->symbols, injected);
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
        if (func_name.empty() && fd) {
            func_name = fd->getName();
        }
        if (!func_name.empty()) {
            proto_enforcer.enforce_single_prototype(ctx->arch.get(), addr, func_name, injected);
        }
    }

    // ========================================================================
    // noreturn auto-marking — must run AFTER prototype enforcement, BEFORE
    // clearAnalysis/reset so that FlowInfo sees the flags during perform().
    // ========================================================================
    {
        // mark_noreturn_functions(ctx, ctx->symbols);
        // mark_noreturn_functions(ctx, ctx->global_symbols);
    }

    // CRITICAL: Reset action state for this function AFTER prototypes are applied
    fission::utils::log_output() << "[DecompilerCore] Resetting action state..." << std::endl;
    ctx->arch->clearAnalysis(fd);
    current_action->reset(*fd);

    fission::utils::log_output() << "[DecompilerCore] Performing decompilation..." << std::endl;

    // Perform decompilation. On Duplicate VariablePiece (our strict types + Ghidra
    // Merge conflict), retry without seed_before_action to get a valid decompilation.
    auto perform_action = [&]() {
        auto perform_start = std::chrono::steady_clock::now();
        ctx->arch->analysis_start = perform_start; // FISSION: record for timeout check in action.cc
        current_action->perform(*fd);
        timing_recorder.timing.main_perform_ms += elapsed_ms(perform_start);
    };

    auto do_seed_and_perform = [&](bool with_seed) {
        if (with_seed) {
            fission::analysis::TypePropagator seeder(ctx->arch.get(), &ctx->struct_registry);
            seeder.set_compiler_id(ctx->compiler_id.empty() ? "windows" : ctx->compiler_id);
            seeder.seed_before_action(fd);
        }
        perform_action();
    };

    bool did_retry_dvp = false;
    try {
        do_seed_and_perform(true);
    } catch (const ghidra::LowlevelError& e) {
        std::string msg = e.explain;
        if (msg.find("Duplicate VariablePiece") != std::string::npos && !did_retry_dvp) {
            fission::utils::log_output() << "[DecompilerCore] Duplicate VariablePiece, retrying without type seed"
                      << std::endl;
            did_retry_dvp = true;
            ctx->arch->clearAnalysis(fd);
            current_action->reset(*fd);
            try {
                do_seed_and_perform(false);
            } catch (const ghidra::LowlevelError& e2) {
                throw std::runtime_error("Ghidra LowlevelError: " + e2.explain + " (retry without seed failed)");
            }
        } else if (msg.find("Function loaded for inlining") != std::string::npos && !ctx->allow_inline) {
            fission::utils::log_output() << "[DecompilerCore] WARNING: Inline-loaded function, clearing analysis and retrying"
                      << std::endl;
            if (ctx->arch) {
                ctx->arch->clearAnalysis(fd);
            } else {
                fd->clear();
            }
            fd->getFuncProto().setInline(false);
            current_action->reset(*fd);
            perform_action();
        } else {
            throw std::runtime_error("Ghidra LowlevelError: " + e.explain);
        }
    } catch (const std::exception& e) {
        throw;
    } catch (...) {
        throw std::runtime_error("Unknown error during decompilation");
    }

    fission::decompiler::AnalysisArtifacts analysis;
    try {
        analysis = fission::decompiler::run_analysis_passes(ctx, fd, current_action, MAX_FUNCTION_SIZE);
    } catch (const ghidra::LowlevelError& e) {
        if (std::string(e.explain).find("Duplicate VariablePiece") != std::string::npos) {
            fission::utils::log_output() << "[DecompilerCore] Duplicate VariablePiece in analysis passes, using base result"
                      << std::endl;
            analysis = fission::decompiler::AnalysisArtifacts{};
        } else {
            throw std::runtime_error("Ghidra LowlevelError: " + e.explain);
        }
    }
    if (out_artifacts) {
        *out_artifacts = analysis;
    }
    timing_recorder.timing.analysis_passes_ms = analysis.analysis_passes_ms;
    timing_recorder.timing.callee_preanalysis_ms = analysis.callee_preanalysis_ms;
    timing_recorder.timing.callgraph_reanalysis_ms = analysis.callgraph_reanalysis_ms;
    timing_recorder.timing.callee_preanalysis_count = analysis.callee_preanalysis_count;
    timing_recorder.timing.callgraph_reanalysis_count = analysis.callgraph_reanalysis_count;
    timing_recorder.timing.stage1_rerun_ms = analysis.stage1_rerun_ms;
    timing_recorder.timing.stage2_rerun_ms = analysis.stage2_rerun_ms;
    
    fission::utils::log_output() << "[DecompilerCore] Generating output..." << std::endl;
    
    // Check print language
    if (!ctx->arch->print) {
        throw std::runtime_error("Print language not initialized");
    }
    
    // Print decompiled output to string
    std::ostringstream ss;
    ctx->arch->print->setOutputStream(&ss);
    auto print_start = std::chrono::steady_clock::now();
    ctx->arch->print->docFunction(fd);
    timing_recorder.timing.print_ms = elapsed_ms(print_start);
    
    std::string result = ss.str();
    
    // ========================================================================
    // Full Post-Processing Chain
    // ========================================================================
    // Use per-context configurable options (set via set_feature with pp_ prefix)
    const PostProcessOptions& options = ctx->post_process_options;

    // Use the analysis artifacts gathered earlier for post-processing
    auto postprocess_start = std::chrono::steady_clock::now();
    result = run_post_processing(ctx, fd, result, analysis, options, &timing_recorder.timing);
    timing_recorder.timing.postprocess_ms = elapsed_ms(postprocess_start);
    
    fission::utils::log_output() << "[DecompilerCore] Decompilation complete, " << result.size() << " bytes after post-processing" << std::endl;
    
    return result;
}

// Serialize StructureAnalyzer's captured_structs to InferredTypeInfo JSON
// Format matches fission_loader::InferredTypeInfo for Rust serde
static std::string serialize_inferred_types_to_json(const fission::decompiler::AnalysisArtifacts& analysis) {
    if (analysis.captured_structs.empty()) {
        return "[]";
    }
    std::ostringstream out;
    out << "[";
    bool first_type = true;
    for (const auto& [base_key, st] : analysis.captured_structs) {
        if (!st) continue;
        if (!first_type) out << ",";
        first_type = false;

        std::string struct_name = st->getName();
        if (struct_name.empty()) struct_name = "anon";

        out << "{\"name\":\"" << fission::utils::json_escape(struct_name)
            << "\",\"mangled_name\":\"\",\"kind\":\"struct\",\"fields\":[";

        bool first_field = true;
        int total_size = 0;
        for (auto iter = st->beginField(); iter != st->endField(); ++iter) {
            if (!first_field) out << ",";
            first_field = false;

            int foff = iter->offset;
            std::string fname = iter->name;
            if (fname.empty()) fname = "field_" + std::to_string(foff);

            int fsize = 4;
            if (iter->type && iter->type->getSize() > 0) {
                fsize = iter->type->getSize();
            }
            std::string ftype = "unknown";
            if (iter->type && !iter->type->getName().empty()) {
                ftype = iter->type->getName();
            }
            total_size = std::max(total_size, foff + fsize);

            out << "{\"name\":\"" << fission::utils::json_escape(fname)
                << "\",\"type_name\":\"" << fission::utils::json_escape(ftype)
                << "\",\"offset\":" << foff
                << ",\"size\":" << fsize << "}";
        }
        out << "],\"size\":" << total_size << ",\"metadata_address\":0}";
    }
    out << "]";
    return out.str();
}

std::string fission::decompiler::run_decompilation_with_metadata(DecompContext* ctx, uint64_t addr) {
    fission::decompiler::AnalysisArtifacts artifacts;
    std::string code = run_decompilation(ctx, addr, &artifacts);
    std::string inferred_json = serialize_inferred_types_to_json(artifacts);
    std::string escaped_code = fission::utils::json_escape(code);
    return "{\"code\":\"" + escaped_code + "\",\"inferred_types\":" + inferred_json + "}";
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
    
    auto serialize_current_pcode = [&]() -> std::string {
        std::ostringstream json;
        json << "{";
        json << "\"blocks\": [";

        const ghidra::BlockGraph& basic_blocks = fd->getBasicBlocks();
        bool first_block = true;
        size_t op_count = 0;
        for (int i = 0; i < basic_blocks.getSize(); ++i) {
            ghidra::FlowBlock* block = basic_blocks.getBlock(i);
            ghidra::BlockBasic* bb = static_cast<ghidra::BlockBasic*>(block);

            std::ostringstream block_json;
            block_json << "{";
            block_json << "\"index\": " << block->getIndex() << ",";
            block_json << "\"start_addr\": \"0x" << std::hex << block->getStart().getOffset()
                       << "\",";
            block_json << "\"ops\": [";

            bool first_op = true;
            auto iter = bb->beginOp();
            auto end_iter = bb->endOp();
            for (; iter != end_iter; ++iter) {
                ghidra::PcodeOp* op = *iter;
                if (!op) continue;
                ++op_count;

                if (!first_op) block_json << ",";
                first_op = false;

                block_json << "{";
                block_json << "\"seq\": " << std::dec << op->getSeqNum().getTime() << ",";
                block_json << "\"opcode\": \"" << json_escape(op->getOpcode()->getName()) << "\",";
                block_json << "\"addr\": \"0x" << std::hex << op->getAddr().getOffset() << std::dec
                           << "\",";

                try {
                    ghidra::Address asm_addr = op->getAddr();
                    SimpleAssemblyEmit asm_emit;
                    ctx->arch->translate->printAssembly(asm_emit, asm_addr);
                    std::string mnemonic = asm_emit.getMnemonic();
                    std::string body = asm_emit.getBody();
                    if (!mnemonic.empty()) {
                        if (!body.empty()) {
                            block_json << "\"asm\": \"" << json_escape(mnemonic) << " "
                                       << json_escape(body) << "\",";
                        } else {
                            block_json << "\"asm\": \"" << json_escape(mnemonic) << "\",";
                        }
                    } else {
                        block_json << "\"asm\": null,";
                    }
                } catch (...) {
                    block_json << "\"asm\": null,";
                }

                ghidra::Varnode* out = op->getOut();
                if (out) {
                    block_json << "\"output\": {";
                    block_json << "\"offset\": \"0x" << std::hex << out->getOffset() << "\",";
                    block_json << "\"size\": " << std::dec << out->getSize() << ",";
                    block_json << "\"space\": " << out->getSpace()->getType() << ",";
                    block_json << "\"const_val\": "
                               << (out->isConstant() ? std::to_string(out->getOffset()) : "null");
                    block_json << "},";
                } else {
                    block_json << "\"output\": null,";
                }

                block_json << "\"inputs\": [";
                for (int j = 0; j < op->numInput(); ++j) {
                    ghidra::Varnode* in = op->getIn(j);
                    if (j > 0) block_json << ",";
                    block_json << "{";
                    block_json << "\"offset\": \"0x" << std::hex << in->getOffset() << "\",";
                    block_json << "\"size\": " << std::dec << in->getSize() << ",";
                    block_json << "\"space\": " << in->getSpace()->getType() << ",";
                    block_json << "\"const_val\": "
                               << (in->isConstant() ? std::to_string(in->getOffset()) : "null");
                    block_json << "}";
                }
                block_json << "]";
                block_json << "}";
            }
            block_json << "]";
            block_json << "}";

            if (!first_op) {
                if (!first_block) json << ",";
                first_block = false;
                json << block_json.str();
            }
        }

        json << "]";
        json << "}";
        if (op_count == 0) {
            return std::string();
        }
        return json.str();
    };

    fd->clear();

    ghidra::Address end_addr = start_addr + 0x10000;
    try {
        fd->followFlow(start_addr, end_addr);
    } catch (...) {}

    // Preview extraction only needs recovered p-code. Avoid full action-group execution
    // unless the lightweight followFlow path failed to populate any ops.
    std::string lightweight_json = serialize_current_pcode();
    if (!lightweight_json.empty()) {
        return lightweight_json;
    }

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
        
        std::string analyzed_json = serialize_current_pcode();
        if (!analyzed_json.empty()) {
            return analyzed_json;
        }
        throw std::runtime_error("Pcode extraction produced no ops");
    } catch (const ghidra::LowlevelError& e) {
        throw std::runtime_error("Ghidra LowlevelError: " + e.explain);
    } catch (const std::exception& e) {
        throw std::runtime_error("Decompilation error: " + std::string(e.what()));
    } catch (...) {
        throw std::runtime_error("Unknown decompilation error in run_decompilation_pcode");
    }
}
