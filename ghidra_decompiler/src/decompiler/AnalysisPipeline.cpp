/**
 * Fission Decompiler Analysis Pipeline
 */

#include "fission/decompiler/AnalysisPipeline.h"
#include "fission/analysis/GlobalDataAnalyzer.h"
#include "fission/analysis/TypePropagator.h"
#include "fission/analysis/CallGraphAnalyzer.h"
#include "fission/analysis/TypeSharing.h"
#include "fission/types/StructureAnalyzer.h"
#include "fission/types/GlobalTypeRegistry.h"
#include "fission/decompiler/PcodeOptimizationBridge.h"
#include "fission/decompiler/PcodeExtractor.h"
#include "fission/decompiler/Limits.h"
#include "fission/ffi/DecompContext.h"

#include "libdecomp.hh"
#include "address.hh"
#include "funcdata.hh"
#include "op.hh"
#include "varnode.hh"
#include "type.hh"

#include <cctype>
#include <iostream>
#include "fission/utils/logger.h"
#include <set>

using namespace fission::analysis;
using namespace fission::types;

namespace fission {
namespace decompiler {

static std::string normalize_symbol_name(const std::string& name) {
    std::string norm = name;
    while (!norm.empty() && norm[0] == '_') {
        norm.erase(norm.begin());
    }
    for (char& ch : norm) {
        ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
    }
    return norm;
}

static bool is_allocator_name(const std::string& name) {
    std::string norm = normalize_symbol_name(name);
    return norm == "malloc" || norm == "calloc" || norm == "realloc";
}

static bool is_address_in_executable(const fission::ffi::DecompContext* ctx, uint64_t addr) {
    if (!ctx) {
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
        uint64_t start = block.va_addr;
        uint64_t end = start + size;
        if (addr >= start && addr < end) {
            return true;
        }
    }
    return false;
}

static bool get_data_section_range(
    const fission::ffi::DecompContext* ctx,
    uint64_t& out_start,
    uint64_t& out_end
) {
    bool found = false;
    uint64_t start = 0;
    uint64_t end = 0;

    if (!ctx) {
        return false;
    }

    for (const auto& block : ctx->memory_blocks) {
        if (block.is_executable) {
            continue;
        }
        uint64_t size = block.va_size > 0 ? block.va_size : block.file_size;
        if (size == 0) {
            continue;
        }
        uint64_t block_start = block.va_addr;
        uint64_t block_end = block_start + size;
        if (!found) {
            start = block_start;
            end = block_end;
            found = true;
        } else {
            if (block_start < start) {
                start = block_start;
            }
            if (block_end > end) {
                end = block_end;
            }
        }
    }

    if (!found) {
        return false;
    }

    out_start = start;
    out_end = end;
    return true;
}

static bool same_high_var(ghidra::Varnode* lhs, ghidra::Varnode* rhs) {
    if (!lhs || !rhs) {
        return false;
    }
    ghidra::HighVariable* high_lhs = lhs->getHigh();
    ghidra::HighVariable* high_rhs = rhs->getHigh();
    if (high_lhs && high_rhs) {
        return high_lhs == high_rhs;
    }
    return lhs == rhs;
}

static bool flows_from_allocator(
    ghidra::Varnode* vn,
    const std::vector<ghidra::Varnode*>& alloc_returns,
    int depth
) {
    if (!vn || depth > 6) {
        return false;
    }
    for (auto* alloc : alloc_returns) {
        if (same_high_var(vn, alloc)) {
            return true;
        }
    }
    if (!vn->isWritten()) {
        return false;
    }
    ghidra::PcodeOp* def = vn->getDef();
    if (!def || def->isDead()) {
        return false;
    }
    switch (def->code()) {
        case ghidra::CPUI_COPY:
        case ghidra::CPUI_CAST:
        case ghidra::CPUI_PTRSUB:
        case ghidra::CPUI_PTRADD:
        case ghidra::CPUI_INT_ZEXT:
        case ghidra::CPUI_INT_SEXT:
            for (int slot = 0; slot < def->numInput(); ++slot) {
                if (flows_from_allocator(def->getIn(slot), alloc_returns, depth + 1)) {
                    return true;
                }
            }
            break;
        default:
            break;
    }
    return false;
}

static bool returns_allocator_result(
    ghidra::Funcdata* fd,
    const std::map<uint64_t, std::string>& symbols,
    ghidra::Architecture* arch
) {
    if (!fd) {
        return false;
    }

    std::vector<ghidra::Varnode*> alloc_returns;
    for (auto iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        ghidra::PcodeOp* op = *iter;
        if (!op || (op->code() != ghidra::CPUI_CALL && op->code() != ghidra::CPUI_CALLIND)) {
            continue;
        }
        std::string target_name;
        uint64_t target_addr = 0;
        if (ghidra::FuncCallSpecs* fc = fd->getCallSpecs(op)) {
            target_name = fc->getName();
            target_addr = fc->getEntryAddress().getOffset();
        }
        if (target_name.empty()) {
            ghidra::Varnode* target = op->getIn(0);
            if (target && target->isConstant()) {
                target_addr = target->getOffset();
            }
        }
        if (!target_name.empty()) {
            // keep name
        } else if (target_addr != 0) {
            auto name_it = symbols.find(target_addr);
            if (name_it != symbols.end()) {
                target_name = name_it->second;
            } else if (arch && arch->symboltab) {
                ghidra::Scope* scope = arch->symboltab->getGlobalScope();
                if (scope) {
                    ghidra::Funcdata* target_fd =
                        scope->findFunction(ghidra::Address(arch->getDefaultCodeSpace(), target_addr));
                    if (target_fd) {
                        target_name = target_fd->getName();
                    }
                }
            }
        }
        if (target_name.empty() || !is_allocator_name(target_name)) {
            continue;
        }
        ghidra::Varnode* out = op->getOut();
        if (out) {
            alloc_returns.push_back(out);
        }
    }

    if (alloc_returns.empty()) {
        return false;
    }

    for (auto iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        ghidra::PcodeOp* op = *iter;
        if (!op || op->code() != ghidra::CPUI_RETURN) {
            continue;
        }
        for (int slot = 0; slot < op->numInput(); ++slot) {
            ghidra::Varnode* ret = op->getIn(slot);
            if (flows_from_allocator(ret, alloc_returns, 0)) {
                return true;
            }
        }
    }

    return false;
}

static bool apply_pointer_return_prototype(ghidra::Architecture* arch, ghidra::Funcdata* fd) {
    if (!arch || !fd) {
        return false;
    }
    ghidra::FuncProto& proto = fd->getFuncProto();
    if (proto.isOutputLocked()) {
        return false;
    }
    ghidra::Datatype* outtype = proto.getOutputType();
    if (outtype && outtype->getMetatype() == ghidra::TYPE_PTR) {
        return false;
    }

    ghidra::TypeFactory* factory = arch->types;
    if (!factory) {
        return false;
    }

    ghidra::Datatype* void_type = factory->getTypeVoid();
    if (!void_type) {
        return false;
    }
    ghidra::int4 ptr_size = factory->getSizeOfPointer();
    ghidra::Datatype* void_ptr = factory->getTypePointer(ptr_size, void_type, 0);
    if (!void_ptr) {
        return false;
    }

    ghidra::PrototypePieces pieces;
    proto.getPieces(pieces);
    pieces.outtype = void_ptr;
    proto.setPieces(pieces);
    proto.setInputLock(false);
    return true;
}

static bool infer_callee_pointer_returns(
    fission::ffi::DecompContext* ctx,
    ghidra::Funcdata* caller_fd,
    ghidra::Action* action
) {
    if (!ctx || !caller_fd || !action || !ctx->arch) {
        return false;
    }

    std::set<uint64_t> callee_addrs;
    for (auto iter = caller_fd->beginOpAlive(); iter != caller_fd->endOpAlive(); ++iter) {
        ghidra::PcodeOp* op = *iter;
        if (!op || (op->code() != ghidra::CPUI_CALL && op->code() != ghidra::CPUI_CALLIND)) {
            continue;
        }
        uint64_t target_addr = 0;
        if (ghidra::FuncCallSpecs* fc = caller_fd->getCallSpecs(op)) {
            target_addr = fc->getEntryAddress().getOffset();
        }
        if (target_addr == 0) {
            ghidra::Varnode* target = op->getIn(0);
            if (target && target->isConstant()) {
                target_addr = target->getOffset();
            }
        }
        if (target_addr == 0) {
            continue;
        }
        if (!is_address_in_executable(ctx, target_addr)) {
            continue;
        }
        callee_addrs.insert(target_addr);
    }

    if (callee_addrs.empty()) {
        return false;
    }

    bool updated = false;
    ghidra::Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
    if (!global_scope) {
        return false;
    }

    for (uint64_t addr : callee_addrs) {
        ghidra::Address func_addr(ctx->arch->getDefaultCodeSpace(), addr);
        ghidra::Funcdata* callee = global_scope->findFunction(func_addr);
        if (!callee) {
            ghidra::FunctionSymbol* sym = global_scope->addFunction(func_addr, "sub_" + std::to_string(addr));
            if (!sym) {
                continue;
            }
            callee = sym->getFunction();
        }
        if (!callee) {
            continue;
        }

        if (callee->isProcStarted() || callee->getFuncProto().isInline()) {
            continue;
        }

        auto sym_it = ctx->symbols.find(addr);
        if (sym_it != ctx->symbols.end() && is_allocator_name(sym_it->second)) {
            continue;
        }

        callee->clear();
        bool flow_ok = true;
        try {
            ghidra::Address start(func_addr);
            // A-3: Use k_callee_follow_limit (16 KB) instead of the previous
            // 4 KB hard-coded limit so larger callee functions are fully
            // covered during pointer-return type inference.
            ghidra::Address end = start + fission::decompiler::k_callee_follow_limit;
            callee->followFlow(start, end);
        } catch (const ghidra::LowlevelError& e) {
            fission::utils::log_stream() << "[AnalysisPipeline] followFlow LowlevelError at 0x"
                << std::hex << addr << ": " << e.explain << std::endl;
            flow_ok = false;
        } catch (...) {
            fission::utils::log_stream() << "[AnalysisPipeline] followFlow unknown error at 0x"
                << std::hex << addr << std::endl;
            flow_ok = false;
        }
        if (!flow_ok) {
            continue;
        }

        action->reset(*callee);
        action->perform(*callee);

        if (returns_allocator_result(callee, ctx->symbols, ctx->arch.get())) {
            if (apply_pointer_return_prototype(ctx->arch.get(), callee)) {
                updated = true;
            }
        }
    }

    return updated;
}

static void rerun_action(ghidra::Funcdata* fd, ghidra::Action* action) {
    fd->clear();
    action->reset(*fd);
    try {
        action->perform(*fd);
    } catch (const ghidra::LowlevelError& e) {
        throw std::runtime_error("Ghidra LowlevelError: " + e.explain);
    } catch (const std::exception&) {
        throw;
    } catch (...) {
        throw std::runtime_error("Unknown error during decompilation");
    }
}

static fission::types::FunctionSignature build_function_signature(ghidra::Funcdata* fd) {
    using namespace fission::types;

    FunctionSignature sig;
    if (fd == nullptr) {
        return sig;
    }

    sig.address = fd->getAddress().getOffset();
    sig.return_type = nullptr;

    const ghidra::FuncProto& proto = fd->getFuncProto();
    ghidra::ProtoParameter* ret = proto.getOutput();
    if (ret != nullptr && ret->getType() != nullptr) {
        ghidra::Datatype* rt = ret->getType();
        if (rt->getMetatype() == ghidra::TYPE_STRUCT) {
            sig.return_type = dynamic_cast<ghidra::TypeStruct*>(rt);
        }
    }

    int num = proto.numParams();
    for (int i = 0; i < num; ++i) {
        ghidra::ProtoParameter* param = proto.getParam(i);
        if (param == nullptr || param->getType() == nullptr) {
            continue;
        }

        ParamTypeInfo pinfo;
        pinfo.param_index = i;
        pinfo.struct_type = nullptr;

        ghidra::Datatype* ptype = param->getType();
        pinfo.type_name = ptype->getName();
        pinfo.is_pointer = (ptype->getMetatype() == ghidra::TYPE_PTR);

        if (ptype->getMetatype() == ghidra::TYPE_STRUCT) {
            pinfo.struct_type = dynamic_cast<ghidra::TypeStruct*>(ptype);
        } else if (pinfo.is_pointer) {
            ghidra::Datatype* pointed = static_cast<ghidra::TypePointer*>(ptype)->getPtrTo();
            if (pointed != nullptr && pointed->getMetatype() == ghidra::TYPE_STRUCT) {
                pinfo.struct_type = dynamic_cast<ghidra::TypeStruct*>(pointed);
            }
        }

        sig.params.push_back(pinfo);
    }

    return sig;
}

static void register_signature_from_func(fission::ffi::DecompContext* ctx, ghidra::Funcdata* fd) {
    if (ctx == nullptr || fd == nullptr) {
        return;
    }
    fission::types::FunctionSignature sig = build_function_signature(fd);
    ctx->type_registry.register_function_types(sig.address, sig);
}

AnalysisArtifacts run_analysis_passes(
    fission::ffi::DecompContext* ctx,
    ghidra::Funcdata* fd,
    ghidra::Action* action,
    size_t max_function_size
) {
    AnalysisArtifacts artifacts;
    if (!ctx || !fd || !action || !ctx->arch) {
        return artifacts;
    }

    size_t func_size = fd->getSize();
    bool needs_rerun_stage1 = false;

    // ========================================================================
    // Stage-1 analysis passes — changes accumulated, one rerun at Barrier-1
    // ========================================================================

    // ---- Pointer-return prototype inference --------------------------------
    {
        bool updated_self = false;
        try {
            if (returns_allocator_result(fd, ctx->symbols, ctx->arch.get())) {
                updated_self = apply_pointer_return_prototype(ctx->arch.get(), fd);
            }
        } catch (const ghidra::LowlevelError& e) {
            fission::utils::log_stream() << "[DecompilerCore] Pointer inference (self) LowlevelError: " << e.explain << std::endl;
        } catch (const std::exception& e) {
            fission::utils::log_stream() << "[DecompilerCore] Pointer inference (self) error: " << e.what() << std::endl;
        } catch (...) {
            fission::utils::log_stream() << "[DecompilerCore] Pointer inference (self) unknown error" << std::endl;
        }

        bool updated_callee = false;
        try {
            updated_callee = infer_callee_pointer_returns(ctx, fd, action);
        } catch (const ghidra::LowlevelError& e) {
            fission::utils::log_stream() << "[DecompilerCore] Pointer inference (callee) LowlevelError: " << e.explain << std::endl;
        } catch (const std::exception& e) {
            fission::utils::log_stream() << "[DecompilerCore] Pointer inference (callee) error: " << e.what() << std::endl;
        } catch (...) {
            fission::utils::log_stream() << "[DecompilerCore] Pointer inference (callee) unknown error" << std::endl;
        }

        if (updated_self || updated_callee) {
            fission::utils::log_stream() << "[DecompilerCore] Updated prototype(s), flagging stage-1 re-run." << std::endl;
            needs_rerun_stage1 = true;
        }
    }

    if (func_size < max_function_size) {
        // ---- Structure recovery --------------------------------------------
        {
            StructureAnalyzer struct_analyzer;
            bool structs_found = struct_analyzer.analyze_function_structures(fd);
            if (structs_found) {
                fission::utils::log_stream() << "[DecompilerCore] Inferred structures, flagging stage-1 re-run." << std::endl;
                artifacts.inferred_struct_definitions = struct_analyzer.generate_struct_definitions();
                artifacts.captured_structs            = struct_analyzer.get_inferred_structs();
                needs_rerun_stage1 = true;

                const ghidra::FuncProto& proto = fd->getFuncProto();
                int num = proto.numParams();
                for (int i = 0; i < num; ++i) {
                    ghidra::ProtoParameter* param = proto.getParam(i);
                    if (!param) continue;
                    uint64_t off = param->getAddress().getOffset();
                    if (artifacts.captured_structs.count(off)) {
                        ctx->struct_registry[fd->getAddress().getOffset()][i] =
                            artifacts.captured_structs[off]->getName();
                    }
                }
            }
        }

        // ---- Reverse struct type propagation --------------------------------
        {
            TypePropagator rev_tp(ctx->arch.get(), &ctx->struct_registry);
            rev_tp.clear();
            bool sc = rev_tp.propagate_struct_types(fd);
            if (sc) {
                fission::utils::log_stream() << "[DecompilerCore] Reverse struct propagation detected, flagging stage-1 re-run." << std::endl;
                needs_rerun_stage1 = true;
                rev_tp.clear();
            }
        }

        // ---- Global data structure recovery --------------------------------
        {
            GlobalDataAnalyzer global_analyzer;
            uint64_t data_start = 0, data_end = 0;
            if (get_data_section_range(ctx, data_start, data_end)) {
                global_analyzer.set_data_section(data_start, data_end);
            }
            global_analyzer.analyze_function(fd);
            global_analyzer.infer_structures();
            int created = global_analyzer.create_types(ctx->arch->types, ctx->arch->types->getSizeOfPointer());
            if (created > 0) {
                fission::utils::log_stream() << "[DecompilerCore] Global data structures created: " << created << std::endl;
            }

            ghidra::Scope*     global_scope = ctx->arch->symboltab->getGlobalScope();
            ghidra::AddrSpace* data_space   = ctx->arch->getDefaultDataSpace();
            if (global_scope && data_space) {
                for (const auto& gs : global_analyzer.get_structures()) {
                    if (gs.name.empty()) continue;
                    ghidra::Datatype* dt = ctx->arch->types->findByName(gs.name);
                    if (!dt || dt->getMetatype() != ghidra::TYPE_STRUCT) continue;
                    ghidra::Address addr(data_space, gs.address);
                    if (ghidra::SymbolEntry* entry = global_scope->findAddr(addr, fd->getAddress())) {
                        ghidra::Symbol* sym = entry->getSymbol();
                        if (sym) {
                            try {
                                global_scope->retypeSymbol(sym, dt);
                                global_scope->setAttribute(sym, ghidra::Varnode::typelock);
                                needs_rerun_stage1 = true;
                            } catch (const ghidra::RecovError&) {}
                        }
                        continue;
                    }
                    if (global_scope->addSymbol(gs.name, dt, addr, fd->getAddress())) {
                        needs_rerun_stage1 = true;
                    }
                }
            }
        }

        // ---- Pre-analysis: call return type propagation --------------------
        {
            TypePropagator initial_propagator(ctx->arch.get(), &ctx->struct_registry);
            initial_propagator.propagate_call_return_types(fd);
        }
    } else {
        fission::utils::log_stream() << "[DecompilerCore] Skipping structure recovery (function too large: "
                  << func_size << " bytes)" << std::endl;
    }

    // ========================================================================
    // Barrier-1: single re-run for all stage-1 changes
    // (was up to 4 separate rerun_action() calls)
    // ========================================================================
    if (needs_rerun_stage1) {
        fission::utils::log_stream() << "[DecompilerCore] Stage-1 re-run (struct/prototype/global-data changes)." << std::endl;
        rerun_action(fd, action);
    }

    bool needs_rerun_stage2 = false;

    // ========================================================================
    // Stage-2 analysis passes
    // ========================================================================
    if (func_size < max_function_size) {
        // ---- Call graph analysis + pending reanalysis ----------------------
        {
            register_signature_from_func(ctx, fd);

            fission::analysis::CallGraphAnalyzer call_analyzer(&ctx->type_registry);
            call_analyzer.extract_calls(fd);
            int propagated = call_analyzer.propagate_types();
            if (propagated > 0) {
                fission::utils::log_stream() << "[DecompilerCore] CallGraph: propagated " << propagated
                          << " type hints" << std::endl;
            }

            std::set<uint64_t> processed;
            ghidra::Scope*     global_scope = ctx->arch->symboltab->getGlobalScope();
            const int          max_rounds   = 2;
            int rounds = 0, reanalyzed = 0;

            std::vector<uint64_t> pending = ctx->type_registry.consume_pending_reanalysis();
            while (!pending.empty() && rounds < max_rounds && global_scope) {
                ++rounds;
                for (uint64_t target_addr : pending) {
                    if (processed.count(target_addr)) continue;
                    processed.insert(target_addr);
                    if (!is_address_in_executable(ctx, target_addr)) continue;

                    ghidra::Address func_addr(ctx->arch->getDefaultCodeSpace(), target_addr);
                    ghidra::Funcdata* target_fd = global_scope->findFunction(func_addr);
                    if (!target_fd) {
                        ghidra::FunctionSymbol* sym =
                            global_scope->addFunction(func_addr, "sub_" + std::to_string(target_addr));
                        if (!sym) continue;
                        target_fd = sym->getFunction();
                    }
                    if (!target_fd) continue;

                    try {
                        target_fd->clear();
                        ghidra::Address end_addr = func_addr + fission::decompiler::k_callee_follow_limit;
                        target_fd->followFlow(func_addr, end_addr);
                        action->reset(*target_fd);
                        action->perform(*target_fd);
                    } catch (const ghidra::LowlevelError& e) {
                        fission::utils::log_stream() << "[DecompilerCore] CallGraph LowlevelError at 0x"
                            << std::hex << target_addr << ": " << e.explain << std::endl; continue;
                    } catch (const std::exception& e) {
                        fission::utils::log_stream() << "[DecompilerCore] CallGraph error at 0x"
                            << std::hex << target_addr << ": " << e.what() << std::endl; continue;
                    } catch (...) {
                        fission::utils::log_stream() << "[DecompilerCore] CallGraph unknown error at 0x"
                            << std::hex << target_addr << std::endl; continue;
                    }

                    register_signature_from_func(ctx, target_fd);
                    call_analyzer.extract_calls(target_fd);
                    ++reanalyzed;
                }
                int newly_propagated = call_analyzer.propagate_types();
                if (newly_propagated <= 0) break;
                pending = ctx->type_registry.consume_pending_reanalysis();
            }

            if (reanalyzed > 0) {
                fission::utils::log_stream() << "[DecompilerCore] CallGraph: reanalyzed "
                          << reanalyzed << " pending functions" << std::endl;
                needs_rerun_stage2 = true;
            }
        }

        // ---- Cross-function type sharing -----------------------------------
        {
            fission::analysis::TypeSharing type_sharing(ctx->arch.get());
            std::vector<ghidra::Datatype*> param_types_ts;
            const ghidra::FuncProto& proto_ts = fd->getFuncProto();
            for (int i = 0; i < proto_ts.numParams(); ++i) {
                ghidra::ProtoParameter* param = proto_ts.getParam(i);
                if (param) param_types_ts.push_back(param->getType());
            }
            ghidra::ProtoParameter* ret_ts   = proto_ts.getOutput();
            ghidra::Datatype*       ret_type = ret_ts ? ret_ts->getType() : nullptr;
            type_sharing.register_function_types(fd->getAddress().getOffset(), param_types_ts, ret_type);
            int shared = type_sharing.share_types();
            if (shared > 0) {
                fission::utils::log_stream() << "[DecompilerCore] TypeSharing: shared " << shared
                          << " types" << std::endl;
            }
        }

        // ---- Pcode optimization bridge -------------------------------------
        if (fission::decompiler::PcodeOptimizationBridge::is_enabled()) {
            try {
                std::string optimized = fission::decompiler::PcodeOptimizationBridge::extract_and_optimize(fd);
                if (!optimized.empty()) {
                    fission::utils::log_stream() << "[DecompilerCore] PcodeOptimization: extracted & optimized ("
                              << optimized.size() << " bytes)" << std::endl;
                    if (fission::decompiler::PcodeExtractor::inject_pcode(fd, optimized)) {
                        fission::utils::log_stream() << "[DecompilerCore] PcodeOptimization: injected, flagging stage-2 re-run." << std::endl;
                        needs_rerun_stage2 = true;
                    }
                }
            } catch (const std::exception& e) {
                fission::utils::log_stream() << "[DecompilerCore] PcodeOptimization error: " << e.what() << std::endl;
            } catch (...) {
                fission::utils::log_stream() << "[DecompilerCore] PcodeOptimization unknown error" << std::endl;
            }
        }

        // ---- Forward type propagation (API inference) ----------------------
        {
            TypePropagator type_propagator(ctx->arch.get(), &ctx->struct_registry);
            type_propagator.clear();
            int types_inferred = type_propagator.propagate(fd);
            bool struct_changed_after = type_propagator.propagate_struct_types(fd);
            if (types_inferred > 0 || struct_changed_after) {
                fission::utils::log_stream() << "[DecompilerCore] Type propagation: "
                          << types_inferred << " type(s) inferred, flagging stage-2 re-run." << std::endl;
                needs_rerun_stage2 = true;
            }
        }
    }

    // ========================================================================
    // Barrier-2: single re-run for all stage-2 changes
    // (was up to 3 separate rerun_action() calls)
    // ========================================================================
    if (needs_rerun_stage2) {
        fission::utils::log_stream() << "[DecompilerCore] Stage-2 re-run (callgraph/pcode/type changes)." << std::endl;
        rerun_action(fd, action);
    }

    return artifacts;
}

// ============================================================================
// Batch analysis path — same passes, BatchAnalysisContext instead of
// ffi::DecompContext.  All logic is shared via free functions above.
// ============================================================================

static bool batch_is_addr_executable(const BatchAnalysisContext& ctx, uint64_t addr) {
    // When no ranges are configured, allow all addresses (conservative: don't
    // silently drop pending reanalysis just because the caller forgot to set ranges).
    if (ctx.executable_ranges.empty()) return true;
    for (const auto& r : ctx.executable_ranges) {
        if (addr >= r.first && addr < r.second) return true;
    }
    return false;
}

AnalysisArtifacts run_analysis_passes(
    BatchAnalysisContext& ctx,
    ghidra::Funcdata* fd,
    ghidra::Action* action,
    size_t max_function_size
) {
    AnalysisArtifacts artifacts;
    if (!fd || !action || !ctx.arch) return artifacts;

    size_t func_size = fd->getSize();
    if (func_size >= max_function_size) {
        fission::utils::log_stream() << "[AnalysisPipeline] Skipping structure recovery (function too large: "
                  << func_size << " bytes)" << std::endl;
        return artifacts;
    }

    bool needs_rerun_stage1 = false;

    // ---- Structure recovery ------------------------------------------------
    {
        StructureAnalyzer struct_analyzer;
        bool structs_found = struct_analyzer.analyze_function_structures(fd);
        if (structs_found) {
            fission::utils::log_stream() << "[AnalysisPipeline] Inferred structures, flagging stage-1 re-run..." << std::endl;
            artifacts.inferred_struct_definitions = struct_analyzer.generate_struct_definitions();
            artifacts.captured_structs            = struct_analyzer.get_inferred_structs();
            needs_rerun_stage1 = true;

            if (ctx.struct_registry) {
                const ghidra::FuncProto& proto = fd->getFuncProto();
                int num = proto.numParams();
                for (int i = 0; i < num; ++i) {
                    ghidra::ProtoParameter* param = proto.getParam(i);
                    if (!param) continue;
                    uint64_t off = param->getAddress().getOffset();
                    if (artifacts.captured_structs.count(off)) {
                        std::string sname = artifacts.captured_structs[off]->getName();
                        (*ctx.struct_registry)[fd->getAddress().getOffset()][i] = sname;
                    }
                }
            }
        }
    }

    // ---- Reverse type propagation ------------------------------------------
    if (ctx.struct_registry) {
        TypePropagator tp(ctx.arch, ctx.struct_registry);
        tp.clear();
        bool sc = tp.propagate_struct_types(fd);
        if (sc) {
            fission::utils::log_stream() << "[AnalysisPipeline] Reverse struct propagation detected." << std::endl;
            needs_rerun_stage1 = true;
            tp.clear();
        }
        int ti = tp.propagate(fd);
        bool sc2 = tp.propagate_struct_types(fd);
        if (ti > 0 || sc2) {
            fission::utils::log_stream() << "[AnalysisPipeline] Type propagation complete (" << ti << " types)." << std::endl;
            needs_rerun_stage1 = true;
        }
    }

    // ---- Global data structure recovery ------------------------------------
    bool rerun_for_struct_symbols = false;
    {
        GlobalDataAnalyzer global_analyzer;
        if (ctx.data_start < ctx.data_end) {
            global_analyzer.set_data_section(ctx.data_start, ctx.data_end);
        }
        global_analyzer.analyze_function(fd);
        global_analyzer.infer_structures();
        int created = global_analyzer.create_types(ctx.arch->types, ctx.arch->types->getSizeOfPointer());
        if (created > 0) {
            fission::utils::log_stream() << "[AnalysisPipeline] Global data structures created: "
                      << created << std::endl;
        }

        ghidra::Scope*    gscope = ctx.arch->symboltab->getGlobalScope();
        ghidra::AddrSpace* dspace = ctx.arch->getDefaultDataSpace();
        if (gscope && dspace) {
            for (const auto& gs : global_analyzer.get_structures()) {
                if (gs.name.empty()) continue;
                ghidra::Datatype* dt = ctx.arch->types->findByName(gs.name);
                if (!dt || dt->getMetatype() != ghidra::TYPE_STRUCT) continue;
                ghidra::Address addr_gd(dspace, gs.address);
                if (ghidra::SymbolEntry* entry = gscope->findAddr(addr_gd, fd->getAddress())) {
                    ghidra::Symbol* sym = entry->getSymbol();
                    if (sym) {
                        try {
                            gscope->retypeSymbol(sym, dt);
                            gscope->setAttribute(sym, ghidra::Varnode::typelock);
                            rerun_for_struct_symbols = true;
                        } catch (const ghidra::RecovError&) {}
                    }
                    continue;
                }
                if (gscope->addSymbol(gs.name, dt, addr_gd, fd->getAddress())) {
                    rerun_for_struct_symbols = true;
                }
            }
        }
    }

    if (rerun_for_struct_symbols) {
        fission::utils::log_stream() << "[AnalysisPipeline] Struct symbols applied, flagging stage-1 re-run." << std::endl;
        needs_rerun_stage1 = true;
    }

    // ---- Barrier 1: single re-run for all stage-1 analysis changes ---------
    // B-1: Consolidates structure recovery, type propagation, and global data
    // symbol registration (previously up to 4 separate rerun_action() calls).
    if (needs_rerun_stage1) {
        fission::utils::log_stream() << "[AnalysisPipeline] Stage-1 re-run (structure/type/global-data changes)." << std::endl;
        rerun_action(fd, action);
    }

    bool needs_rerun_stage2 = false;

    // ---- Call graph analysis + pending reanalysis --------------------------
    if (ctx.type_registry) {
        fission::types::FunctionSignature sig = build_function_signature(fd);
        ctx.type_registry->register_function_types(sig.address, sig);

        fission::analysis::CallGraphAnalyzer call_analyzer(ctx.type_registry);
        call_analyzer.extract_calls(fd);
        int propagated = call_analyzer.propagate_types();
        if (propagated > 0) {
            fission::utils::log_stream() << "[AnalysisPipeline] CallGraph: propagated "
                      << propagated << " type hints" << std::endl;
        }

        // Bounded pending reanalysis loop
        ghidra::Scope*       cg_scope = ctx.arch->symboltab->getGlobalScope();
        std::set<uint64_t>   processed;
        const int            max_rounds = 2;
        int rounds = 0, reanalyzed = 0;

        std::vector<uint64_t> pending = ctx.type_registry->consume_pending_reanalysis();
        while (!pending.empty() && rounds < max_rounds && cg_scope) {
            ++rounds;
            for (uint64_t ta : pending) {
                if (processed.count(ta)) continue;
                processed.insert(ta);
                if (!batch_is_addr_executable(ctx, ta)) continue;

                ghidra::Address tfa(ctx.arch->getDefaultCodeSpace(), ta);
                ghidra::Funcdata* tfd = cg_scope->findFunction(tfa);
                if (!tfd) {
                    ghidra::FunctionSymbol* sym = cg_scope->addFunction(tfa, "sub_" + std::to_string(ta));
                    if (!sym) continue;
                    tfd = sym->getFunction();
                }
                if (!tfd) continue;

                try {
                    tfd->clear();
                    tfd->followFlow(tfa, tfa + fission::decompiler::k_callee_follow_limit);
                    action->reset(*tfd);
                    action->perform(*tfd);
                } catch (const ghidra::LowlevelError& e) {
                    fission::utils::log_stream() << "[AnalysisPipeline] callgraph reanalysis LowlevelError at 0x"
                        << std::hex << tfa.getOffset() << ": " << e.explain << std::endl;
                    continue;
                } catch (const std::exception& e) {
                    fission::utils::log_stream() << "[AnalysisPipeline] callgraph reanalysis error at 0x"
                        << std::hex << tfa.getOffset() << ": " << e.what() << std::endl;
                    continue;
                } catch (...) {
                    fission::utils::log_stream() << "[AnalysisPipeline] callgraph reanalysis unknown error at 0x"
                        << std::hex << tfa.getOffset() << std::endl;
                    continue;
                }

                fission::types::FunctionSignature tsig = build_function_signature(tfd);
                ctx.type_registry->register_function_types(tsig.address, tsig);
                call_analyzer.extract_calls(tfd);
                ++reanalyzed;
            }
            int newly_propagated = call_analyzer.propagate_types();
            if (newly_propagated <= 0) break;
            pending = ctx.type_registry->consume_pending_reanalysis();
        }

        if (reanalyzed > 0) {
            fission::utils::log_stream() << "[AnalysisPipeline] CallGraph: reanalyzed "
                      << reanalyzed << " pending functions." << std::endl;
            needs_rerun_stage2 = true;
        }
    }

    // ---- Cross-function type sharing ----------------------------------------
    {
        fission::analysis::TypeSharing type_sharing(ctx.arch);
        std::vector<ghidra::Datatype*> param_types_ts;
        const ghidra::FuncProto& proto_ts = fd->getFuncProto();
        for (int i = 0; i < proto_ts.numParams(); ++i) {
            ghidra::ProtoParameter* p = proto_ts.getParam(i);
            if (p) param_types_ts.push_back(p->getType());
        }
        ghidra::ProtoParameter* ret_ts    = proto_ts.getOutput();
        ghidra::Datatype*       ret_type  = ret_ts ? ret_ts->getType() : nullptr;
        type_sharing.register_function_types(fd->getAddress().getOffset(), param_types_ts, ret_type);
        int shared = type_sharing.share_types();
        if (shared > 0) {
            fission::utils::log_stream() << "[AnalysisPipeline] TypeSharing: shared " << shared
                      << " types" << std::endl;
        }
    }

    // ---- Pcode optimization bridge -----------------------------------------
    if (fission::decompiler::PcodeOptimizationBridge::is_enabled()) {
        try {
            std::string opt = fission::decompiler::PcodeOptimizationBridge::extract_and_optimize(fd);
            if (!opt.empty() && fission::decompiler::PcodeExtractor::inject_pcode(fd, opt)) {
                fission::utils::log_stream() << "[AnalysisPipeline] PcodeOptimization: injected, flagging stage-2 re-run." << std::endl;
                needs_rerun_stage2 = true;
            }
        } catch (const std::exception& e) {
            fission::utils::log_stream() << "[AnalysisPipeline] PcodeOptimization error: " << e.what() << std::endl;
        } catch (...) {
            fission::utils::log_stream() << "[AnalysisPipeline] PcodeOptimization unknown error" << std::endl;
        }
    }

    // ---- Pre-analysis type propagation (call return types) -----------------
    if (ctx.struct_registry) {
        TypePropagator initial_tp(ctx.arch, ctx.struct_registry);
        initial_tp.propagate_call_return_types(fd);
    }

    // ---- Barrier 2: single re-run for callgraph + optimisation changes -----
    // B-1: Consolidates callgraph reanalysis and Pcode bridge injection
    // (previously up to 2 separate rerun_action() calls).
    if (needs_rerun_stage2) {
        fission::utils::log_stream() << "[AnalysisPipeline] Stage-2 re-run (callgraph/pcode changes)." << std::endl;
        rerun_action(fd, action);
    }

    return artifacts;
}

} // namespace decompiler
} // namespace fission
