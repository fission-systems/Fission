/**
 * Fission Decompiler Analysis Pipeline
 */

#include "fission/decompiler/AnalysisPipeline.h"
#include "fission/analysis/GlobalDataAnalyzer.h"
#include "fission/analysis/StackFrameAnalyzer.h"
#include "fission/analysis/TypePropagator.h"
#include "fission/analysis/CallGraphAnalyzer.h"
#include "fission/analysis/TypeSharing.h"
#include "fission/types/StructureAnalyzer.h"
#include "fission/types/GlobalTypeRegistry.h"
#include "fission/decompiler/PcodeOptimizationBridge.h"
#include "fission/decompiler/PcodeExtractor.h"
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
            ghidra::Address end = start + 0x1000;
            callee->followFlow(start, end);
        } catch (const ghidra::LowlevelError&) {
            flow_ok = false;
        } catch (...) {
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

    bool updated_self = false;
    try {
        if (returns_allocator_result(fd, ctx->symbols, ctx->arch.get())) {
            updated_self = apply_pointer_return_prototype(ctx->arch.get(), fd);
        }
    } catch (const ghidra::LowlevelError& e) {
        fission::utils::log_stream() << "[DecompilerCore] Pointer inference failed (self): " << e.explain << std::endl;
    } catch (const std::exception& e) {
        fission::utils::log_stream() << "[DecompilerCore] Pointer inference failed (self): " << e.what() << std::endl;
    } catch (...) {
        fission::utils::log_stream() << "[DecompilerCore] Pointer inference failed (self): unknown error" << std::endl;
    }

    bool updated_callee = false;
    try {
        updated_callee = infer_callee_pointer_returns(ctx, fd, action);
    } catch (const ghidra::LowlevelError& e) {
        fission::utils::log_stream() << "[DecompilerCore] Pointer inference failed (callee): " << e.explain << std::endl;
    } catch (const std::exception& e) {
        fission::utils::log_stream() << "[DecompilerCore] Pointer inference failed (callee): " << e.what() << std::endl;
    } catch (...) {
        fission::utils::log_stream() << "[DecompilerCore] Pointer inference failed (callee): unknown error" << std::endl;
    }
    if (updated_self || updated_callee) {
        fission::utils::log_stream() << "[DecompilerCore] Updated callee prototypes, re-running..." << std::endl;
        rerun_action(fd, action);
    }

    // ========================================================================
    // Structure Recovery + Reverse Type Propagation (Ghidra-inspired)
    // ========================================================================
    size_t func_size = fd->getSize();
    if (func_size < max_function_size) {
        StructureAnalyzer struct_analyzer;
        bool structs_found = struct_analyzer.analyze_function_structures(fd);

        if (structs_found) {
            fission::utils::log_stream() << "[DecompilerCore] Inferred structures, re-running..." << std::endl;
            artifacts.inferred_struct_definitions = struct_analyzer.generate_struct_definitions();
            artifacts.captured_structs = struct_analyzer.get_inferred_structs();

            rerun_action(fd, action);

            const ghidra::FuncProto& proto = fd->getFuncProto();
            int num = proto.numParams();
            for (int i = 0; i < num; ++i) {
                ghidra::ProtoParameter* param = proto.getParam(i);
                if (!param) continue;
                uint64_t off = param->getAddress().getOffset();
                if (artifacts.captured_structs.count(off)) {
                    std::string sname = artifacts.captured_structs[off]->getName();
                    ctx->struct_registry[fd->getAddress().getOffset()][i] = sname;
                }
            }
        }
    } else {
        fission::utils::log_stream() << "[DecompilerCore] Skipping structure recovery (function too large: "
                  << func_size << " bytes)" << std::endl;
    }

    // ========================================================================
    // Global Data + Stack Frame Structure Recovery
    // ========================================================================
    bool rerun_for_struct_symbols = false;
    if (func_size < max_function_size) {
        // Global data structures (const/global memory)
        {
            GlobalDataAnalyzer global_analyzer;
            uint64_t data_start = 0;
            uint64_t data_end = 0;
            if (get_data_section_range(ctx, data_start, data_end)) {
                global_analyzer.set_data_section(data_start, data_end);
            }
            global_analyzer.analyze_function(fd);
            global_analyzer.infer_structures();
            int created = global_analyzer.create_types(ctx->arch->types, ctx->arch->types->getSizeOfPointer());
            if (created > 0) {
                fission::utils::log_stream() << "[DecompilerCore] Global data structures created: "
                          << created << std::endl;
            }

            ghidra::Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
            ghidra::AddrSpace* data_space = ctx->arch->getDefaultDataSpace();
            if (global_scope && data_space) {
                for (const auto& gs : global_analyzer.get_structures()) {
                    if (gs.name.empty()) {
                        continue;
                    }
                    ghidra::Datatype* dt = ctx->arch->types->findByName(gs.name);
                    if (!dt || dt->getMetatype() != ghidra::TYPE_STRUCT) {
                        continue;
                    }
                    ghidra::Address addr(data_space, gs.address);
                    if (ghidra::SymbolEntry* entry = global_scope->findAddr(addr, fd->getAddress())) {
                        ghidra::Symbol* sym = entry->getSymbol();
                        if (sym) {
                            try {
                                global_scope->retypeSymbol(sym, dt);
                                global_scope->setAttribute(sym, ghidra::Varnode::typelock);
                                rerun_for_struct_symbols = true;
                            } catch (const ghidra::RecovError&) {
                                // ignore retype failures
                            }
                        }
                        continue;
                    }
                    if (global_scope->addSymbol(gs.name, dt, addr, fd->getAddress())) {
                        rerun_for_struct_symbols = true;
                    }
                }
            }
        }

        // Call Graph Analysis & Type Registry
        {
            uint64_t func_addr_cg = fd->getAddress().getOffset();
            register_signature_from_func(ctx, fd);

            fission::analysis::CallGraphAnalyzer call_analyzer(&ctx->type_registry);
            call_analyzer.extract_calls(fd);
            int propagated = call_analyzer.propagate_types();
            if (propagated > 0) {
                fission::utils::log_stream() << "[DecompilerCore] CallGraph: propagated " << propagated
                          << " type hints" << std::endl;
            }

            // Drain pending reanalysis queue and run a bounded reanalysis loop.
            std::set<uint64_t> processed;
            ghidra::Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
            const int max_rounds = 2;
            int rounds = 0;
            int reanalyzed = 0;

            std::vector<uint64_t> pending = ctx->type_registry.consume_pending_reanalysis();
            while (!pending.empty() && rounds < max_rounds && global_scope != nullptr) {
                ++rounds;
                for (uint64_t target_addr : pending) {
                    if (processed.count(target_addr) != 0) {
                        continue;
                    }
                    processed.insert(target_addr);

                    if (!is_address_in_executable(ctx, target_addr)) {
                        continue;
                    }

                    ghidra::Address func_addr(ctx->arch->getDefaultCodeSpace(), target_addr);
                    ghidra::Funcdata* target_fd = global_scope->findFunction(func_addr);
                    if (target_fd == nullptr) {
                        ghidra::FunctionSymbol* sym = global_scope->addFunction(func_addr, "sub_" + std::to_string(target_addr));
                        if (sym == nullptr) {
                            continue;
                        }
                        target_fd = sym->getFunction();
                    }
                    if (target_fd == nullptr) {
                        continue;
                    }

                    try {
                        target_fd->clear();
                        ghidra::Address end_addr = func_addr + 0x1000;
                        target_fd->followFlow(func_addr, end_addr);
                        action->reset(*target_fd);
                        action->perform(*target_fd);
                    } catch (const ghidra::LowlevelError&) {
                        continue;
                    } catch (...) {
                        continue;
                    }

                    register_signature_from_func(ctx, target_fd);
                    call_analyzer.extract_calls(target_fd);
                    reanalyzed++;
                }

                int newly_propagated = call_analyzer.propagate_types();
                if (newly_propagated <= 0) {
                    break;
                }
                pending = ctx->type_registry.consume_pending_reanalysis();
            }

            if (reanalyzed > 0) {
                fission::utils::log_stream() << "[DecompilerCore] CallGraph: reanalyzed "
                          << reanalyzed << " pending functions" << std::endl;
            }

            if (reanalyzed > 0) {
                rerun_action(fd, action);
            }
        }

        // Cross-function Type Sharing
        {
            fission::analysis::TypeSharing type_sharing(ctx->arch.get());
            std::vector<ghidra::Datatype*> param_types_ts;
            const ghidra::FuncProto& proto_ts = fd->getFuncProto();
            for (int i = 0; i < proto_ts.numParams(); ++i) {
                ghidra::ProtoParameter* param = proto_ts.getParam(i);
                if (param) param_types_ts.push_back(param->getType());
            }
            ghidra::ProtoParameter* ret_ts = proto_ts.getOutput();
            ghidra::Datatype* ret_type_ts = (ret_ts ? ret_ts->getType() : nullptr);
            uint64_t func_addr_ts = fd->getAddress().getOffset();
            type_sharing.register_function_types(func_addr_ts, param_types_ts, ret_type_ts);
            int shared = type_sharing.share_types();
            if (shared > 0) {
                fission::utils::log_stream() << "[DecompilerCore] TypeSharing: shared " << shared
                          << " types" << std::endl;
            }
        }

        // Pcode Optimization Bridge
        if (fission::decompiler::PcodeOptimizationBridge::is_enabled()) {
            try {
                std::string optimized = fission::decompiler::PcodeOptimizationBridge::extract_and_optimize(fd);
                if (!optimized.empty()) {
                    fission::utils::log_stream() << "[DecompilerCore] PcodeOptimization: extracted & optimized ("
                              << optimized.size() << " bytes)" << std::endl;
                    if (fission::decompiler::PcodeExtractor::inject_pcode(fd, optimized)) {
                        fission::utils::log_stream() << "[DecompilerCore] PcodeOptimization: injected optimized Pcode, re-running"
                                  << std::endl;
                        rerun_action(fd, action);
                    }
                }
            } catch (const std::exception& e) {
                fission::utils::log_stream() << "[DecompilerCore] PcodeOptimization error: "
                          << e.what() << std::endl;
            } catch (...) {
                fission::utils::log_stream() << "[DecompilerCore] PcodeOptimization unknown error" << std::endl;
            }
        }

        // Pre-analysis: Propagate call return types (detect pointers from allocators)
        // We do this BEFORE StackFrameAnalyzer so it knows about pointer returns
        {
            TypePropagator initial_propagator(ctx->arch.get(), &ctx->struct_registry);
            initial_propagator.propagate_call_return_types(fd);
        }

        // Stack frame structures
        // DISABLED: Using Ghidra's default local variable mechanism instead
        // This allows individual stack variables (e.g., local_c, local_10) 
        // instead of grouping them into stack structures (e.g., sStack_38.field_44)
        /*
        {
            StackFrameAnalyzer stack_analyzer(ctx->arch.get());
            int detected = stack_analyzer.analyze(fd);
            if (detected > 0) {
                auto stack_structs = stack_analyzer.build_struct_map(ctx->arch->types);
                if (!stack_structs.empty()) {
                    ghidra::ScopeLocal* local_scope = fd->getScopeLocal();
                    ghidra::AddrSpace* stack_space = ctx->arch->getStackSpace();
                    if (local_scope && stack_space) {
                        for (const auto& entry : stack_structs) {
                            int64_t base_offset = entry.first;
                            ghidra::TypeStruct* ts = entry.second;
                            if (!ts) {
                                continue;
                            }
                            ghidra::Address addr(
                                stack_space,
                                static_cast<uint64_t>(base_offset)
                            );
                            if (ghidra::SymbolEntry* sym_entry = local_scope->findAddr(addr, fd->getAddress())) {
                                ghidra::Symbol* sym = sym_entry->getSymbol();
                                if (sym) {
                                    try {
                                        local_scope->retypeSymbol(sym, ts);
                                        local_scope->setAttribute(sym, ghidra::Varnode::typelock);
                                        rerun_for_struct_symbols = true;
                                    } catch (const ghidra::RecovError&) {
                                        // ignore retype failures
                                    }
                                }
                                continue;
                            }
                            if (local_scope->addSymbol(ts->getName(), ts, addr, fd->getAddress())) {
                                rerun_for_struct_symbols = true;
                            }
                        }
                    }
                }

                fission::utils::log_stream() << "[DecompilerCore] Stack frame structures created: "
                          << detected << std::endl;
            }
        }
        */
        
        fission::utils::log_stream() << "[DecompilerCore] Using Ghidra's default stack variable handling" << std::endl;
    }

    if (rerun_for_struct_symbols) {
        fission::utils::log_stream() << "[DecompilerCore] Struct symbols applied, re-running..." << std::endl;
        rerun_action(fd, action);
    }

    TypePropagator type_propagator(ctx->arch.get(), &ctx->struct_registry);
    type_propagator.clear();  
    
    bool struct_changed = type_propagator.propagate_struct_types(fd);
    if (struct_changed) {
        fission::utils::log_stream() << "[DecompilerCore] Struct types propagated, re-running..." << std::endl;
        rerun_action(fd, action);
        type_propagator.clear();
    }

    int types_inferred = type_propagator.propagate(fd);

    bool struct_changed_after = type_propagator.propagate_struct_types(fd);
    if (types_inferred > 0 || struct_changed_after) {
        if (struct_changed_after) {
            fission::utils::log_stream() << "[DecompilerCore] Struct types updated after type propagation, re-running..."
                      << std::endl;
        } else {
            fission::utils::log_stream() << "[DecompilerCore] Type propagation complete (" << types_inferred
                      << " types), re-running for output..." << std::endl;
        }
        rerun_action(fd, action);
    }

    return artifacts;
}

} // namespace decompiler
} // namespace fission
