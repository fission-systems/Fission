/**
 * Fission Decompiler Analysis Pipeline
 */

#include "fission/decompiler/AnalysisPipeline.h"
#include "fission/analysis/GlobalDataAnalyzer.h"
#include "fission/analysis/TypePropagator.h"
#include "fission/analysis/CallGraphAnalyzer.h"
#include "fission/analysis/TypeSharing.h"
#include "fission/analysis/NoReturnDetector.h"
#include "fission/types/StructureAnalyzer.h"
#include "fission/types/GlobalTypeRegistry.h"
#include "fission/decompiler/PcodeOptimizationBridge.h"
#include "fission/decompiler/PcodeExtractor.h"
#include "fission/decompiler/Limits.h"
#include "fission/ffi/DecompContext.h"

#include "libdecomp.hh"
#include "address.hh"
#include "funcdata.hh"
#include "jumptable.hh"
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

// ============================================================================
// Concrete AnalysisContext adapters
// ============================================================================

// ---------------------------------------------------------------------------
// FfiAnalysisContext — wraps ffi::DecompContext* for the FFI (libdecomp) path.
// ---------------------------------------------------------------------------
class FfiAnalysisContext final : public AnalysisContext {
    fission::ffi::DecompContext* ctx_;

public:
    explicit FfiAnalysisContext(fission::ffi::DecompContext* ctx) : ctx_(ctx) {}

    ghidra::Architecture* get_arch() override { return ctx_->arch.get(); }

    const std::map<uint64_t, std::string>& get_symbols() override {
        return ctx_->symbols;
    }

    std::map<uint64_t, std::map<int, std::string>>* get_struct_registry() override {
        return &ctx_->struct_registry;
    }

    fission::types::GlobalTypeRegistry* get_type_registry() override {
        return &ctx_->type_registry;
    }

    bool get_data_section_range(uint64_t& out_start, uint64_t& out_end) override {
        return ::fission::decompiler::get_data_section_range(ctx_, out_start, out_end);
    }

    bool is_address_executable(uint64_t addr) override {
        return is_address_in_executable(ctx_, addr);
    }

    bool has_pointer_return_inference() const override { return true; }

    bool try_infer_pointer_returns(
        ghidra::Funcdata* fd, ghidra::Action* action) override
    {
        bool updated_self = false;
        try {
            if (returns_allocator_result(fd, ctx_->symbols, ctx_->arch.get())) {
                updated_self = apply_pointer_return_prototype(ctx_->arch.get(), fd);
            }
        } catch (const ghidra::LowlevelError& e) {
            fission::utils::log_stream() << "[AnalysisPipeline] Pointer inference (self) LowlevelError: "
                << e.explain << std::endl;
        } catch (const std::exception& e) {
            fission::utils::log_stream() << "[AnalysisPipeline] Pointer inference (self) error: "
                << e.what() << std::endl;
        } catch (...) {
            fission::utils::log_stream() << "[AnalysisPipeline] Pointer inference (self) unknown error"
                << std::endl;
        }

        bool updated_callee = false;
        try {
            updated_callee = infer_callee_pointer_returns(ctx_, fd, action);
        } catch (const ghidra::LowlevelError& e) {
            fission::utils::log_stream() << "[AnalysisPipeline] Pointer inference (callee) LowlevelError: "
                << e.explain << std::endl;
        } catch (const std::exception& e) {
            fission::utils::log_stream() << "[AnalysisPipeline] Pointer inference (callee) error: "
                << e.what() << std::endl;
        } catch (...) {
            fission::utils::log_stream() << "[AnalysisPipeline] Pointer inference (callee) unknown error"
                << std::endl;
        }

        if (updated_self || updated_callee) {
            fission::utils::log_stream() << "[AnalysisPipeline] Updated prototype(s), flagging stage-1 re-run."
                << std::endl;
        }
        return updated_self || updated_callee;
    }

    void register_function_signature(ghidra::Funcdata* fd) override {
        register_signature_from_func(ctx_, fd);
    }
};

// ---------------------------------------------------------------------------
// BatchAnalysisAdapter — wraps BatchAnalysisContext& for the batch
// (fission_decomp CLI) path.
// ---------------------------------------------------------------------------

static bool batch_is_addr_executable(const BatchAnalysisContext& ctx, uint64_t addr) {
    if (ctx.executable_ranges.empty()) return true;
    for (const auto& r : ctx.executable_ranges) {
        if (addr >= r.first && addr < r.second) return true;
    }
    return false;
}

class BatchAnalysisAdapter final : public AnalysisContext {
    BatchAnalysisContext& ctx_;

public:
    explicit BatchAnalysisAdapter(BatchAnalysisContext& ctx) : ctx_(ctx) {}

    ghidra::Architecture* get_arch() override { return ctx_.arch; }

    const std::map<uint64_t, std::string>& get_symbols() override {
        static const std::map<uint64_t, std::string> empty;
        return ctx_.symbols ? *ctx_.symbols : empty;
    }

    std::map<uint64_t, std::map<int, std::string>>* get_struct_registry() override {
        return ctx_.struct_registry;
    }

    fission::types::GlobalTypeRegistry* get_type_registry() override {
        return ctx_.type_registry;
    }

    bool get_data_section_range(uint64_t& out_start, uint64_t& out_end) override {
        if (ctx_.data_start >= ctx_.data_end) return false;
        out_start = ctx_.data_start;
        out_end = ctx_.data_end;
        return true;
    }

    bool is_address_executable(uint64_t addr) override {
        return batch_is_addr_executable(ctx_, addr);
    }

    bool has_pointer_return_inference() const override { return false; }

    // try_infer_pointer_returns: uses default no-op (returns false).

    void register_function_signature(ghidra::Funcdata* fd) override {
        if (!ctx_.type_registry || !fd) return;
        fission::types::FunctionSignature sig = build_function_signature(fd);
        ctx_.type_registry->register_function_types(sig.address, sig);
    }
};

// ============================================================================
// Unified run_analysis_passes — single implementation driven by
// AnalysisContext.  Follows the FFI order (the more complete path):
//   Stage-1: pointer-return → structure → reverse-type → global-data → call-return
//   Barrier-1
//   Stage-2: callgraph → type-sharing → pcode-opt → forward-type
//   Barrier-2
//   post-barrier: merge_split_double_args
// ============================================================================

AnalysisArtifacts run_analysis_passes(
    AnalysisContext& ctx,
    ghidra::Funcdata* fd,
    ghidra::Action* action,
    size_t max_function_size
) {
    AnalysisArtifacts artifacts;
    ghidra::Architecture* arch = ctx.get_arch();
    if (!fd || !action || !arch) {
        return artifacts;
    }

    size_t func_size = fd->getSize();
    bool needs_rerun_stage1 = false;

    auto* struct_registry = ctx.get_struct_registry();
    auto* type_registry   = ctx.get_type_registry();

    // ========================================================================
    // Stage-1 analysis passes — changes accumulated, one rerun at Barrier-1
    // ========================================================================

    // ---- Pointer-return prototype inference (FFI only) ---------------------
    if (ctx.has_pointer_return_inference()) {
        bool updated = ctx.try_infer_pointer_returns(fd, action);
        if (updated) {
            needs_rerun_stage1 = true;
        }
    }

    if (func_size < max_function_size) {
        // ---- Structure recovery --------------------------------------------
        {
            StructureAnalyzer struct_analyzer;
            bool structs_found = struct_analyzer.analyze_function_structures(fd);
            if (structs_found) {
                fission::utils::log_stream() << "[AnalysisPipeline] Inferred structures, flagging stage-1 re-run."
                    << std::endl;
                artifacts.inferred_struct_definitions = struct_analyzer.generate_struct_definitions();
                artifacts.inferred_union_definitions  = struct_analyzer.generate_union_definitions();
                artifacts.captured_structs            = struct_analyzer.get_inferred_structs();
                artifacts.type_replacements           = struct_analyzer.get_type_replacements();
                needs_rerun_stage1 = true;

                if (struct_registry) {
                    const ghidra::FuncProto& proto = fd->getFuncProto();
                    int num = proto.numParams();
                    for (int i = 0; i < num; ++i) {
                        ghidra::ProtoParameter* param = proto.getParam(i);
                        if (!param) continue;
                        uint64_t off = param->getAddress().getOffset();
                        if (artifacts.captured_structs.count(off)) {
                            (*struct_registry)[fd->getAddress().getOffset()][i] =
                                artifacts.captured_structs[off]->getName();
                        }
                    }
                }
            }
        }

        // ---- Reverse struct type propagation --------------------------------
        if (struct_registry) {
            TypePropagator rev_tp(arch, struct_registry);
            rev_tp.clear();
            bool sc = rev_tp.propagate_struct_types(fd);
            if (sc) {
                fission::utils::log_stream() << "[AnalysisPipeline] Reverse struct propagation detected, "
                    "flagging stage-1 re-run." << std::endl;
                needs_rerun_stage1 = true;
                rev_tp.clear();
            }
        }

        // ---- Global data structure recovery --------------------------------
        {
            GlobalDataAnalyzer global_analyzer;
            uint64_t data_start = 0, data_end = 0;
            if (ctx.get_data_section_range(data_start, data_end)) {
                global_analyzer.set_data_section(data_start, data_end);
            }
            global_analyzer.analyze_function(fd);
            global_analyzer.infer_structures();
            int created = global_analyzer.create_types(arch->types,
                arch->types->getSizeOfPointer());
            if (created > 0) {
                fission::utils::log_stream() << "[AnalysisPipeline] Global data structures created: "
                    << created << std::endl;
            }

            ghidra::Scope*     global_scope = arch->symboltab->getGlobalScope();
            ghidra::AddrSpace* data_space   = arch->getDefaultDataSpace();
            if (global_scope && data_space) {
                // --- Structured globals ---
                for (const auto& gs : global_analyzer.get_structures()) {
                    if (gs.name.empty()) continue;
                    ghidra::Datatype* dt = arch->types->findByName(gs.name);
                    if (!dt || dt->getMetatype() != ghidra::TYPE_STRUCT) continue;
                    ghidra::Address addr(data_space, gs.address);
                    if (ghidra::SymbolEntry* entry =
                            global_scope->findAddr(addr, fd->getAddress())) {
                        ghidra::Symbol* sym = entry->getSymbol();
                        if (sym) {
                            try {
                                global_scope->retypeSymbol(sym, dt);
                                global_scope->setAttribute(sym,
                                    ghidra::Varnode::typelock);
                                needs_rerun_stage1 = true;
                            } catch (const ghidra::RecovError&) {}
                        }
                        continue;
                    }
                    if (global_scope->addSymbol(gs.name, dt, addr,
                            fd->getAddress())) {
                        needs_rerun_stage1 = true;
                    }
                }

                // --- GAP-1 FIX: Scalar float/double DAT_ symbol registration ---
                // Ghidra's ConstantPropagationAnalyzer creates typed data symbols
                // for scalar float/double globals so that ActionInferTypes can
                // propagate the type through LOAD opcodes, replacing raw hex
                // constants (e.g. 0x4048feb851eb851f) with named DAT_ symbols.
                ghidra::TypeFactory* tf = arch->types;
                for (const auto& sf : global_analyzer.get_scalar_floats()) {
                    ghidra::Address sf_addr(data_space, sf.address);
                    // Skip if a symbol already exists at this address
                    if (global_scope->findAddr(sf_addr, fd->getAddress()))
                        continue;

                    // Build a DAT_<hex> name consistent with Ghidra convention
                    char dat_name[32];
                    std::snprintf(dat_name, sizeof(dat_name), "DAT_%08llx",
                        (unsigned long long)sf.address);

                    ghidra::Datatype* scalar_type =
                        (sf.size == 4)
                            ? tf->getBase(4, ghidra::TYPE_FLOAT)
                            : tf->getBase(8, ghidra::TYPE_FLOAT);

                    if (!scalar_type) continue;

                    try {
                        if (global_scope->addSymbol(dat_name, scalar_type,
                                sf_addr, fd->getAddress())) {
                            fission::utils::log_stream()
                                << "[AnalysisPipeline] Registered scalar "
                                << (sf.size == 4 ? "float" : "double")
                                << " symbol " << dat_name << " at 0x"
                                << std::hex << sf.address << std::dec << "\n";
                            needs_rerun_stage1 = true;
                        }
                    } catch (const ghidra::RecovError&) {}
                }
            }
        }

        // --- GAP-2: No-return evidence collection per function ---
        // Collect evidence that this function's callees may be no-return.
        // Accumulated evidence is applied below at the Stage-1 barrier.
        {
            static NoReturnDetector s_noret_detector;
            s_noret_detector.collect_evidence(fd);
            int marked = s_noret_detector.apply(arch);
            if (marked > 0) {
                fission::utils::log_stream()
                    << "[AnalysisPipeline] NoReturnDetector: marked "
                    << marked << " function(s) noreturn\n";
                needs_rerun_stage1 = true;
            }
        }

        // ---- Call return type propagation (Stage-1 end) --------------------
        if (struct_registry) {
            TypePropagator initial_propagator(arch, struct_registry);
            initial_propagator.propagate_call_return_types(fd);
        }
    } else {
        fission::utils::log_stream() << "[AnalysisPipeline] Skipping structure recovery "
            "(function too large: " << func_size << " bytes)" << std::endl;
    }

    // ========================================================================
    // Barrier-1: single re-run for all stage-1 changes
    // ========================================================================
    if (needs_rerun_stage1) {
        fission::utils::log_stream() << "[AnalysisPipeline] Stage-1 re-run "
            "(struct/prototype/global-data changes)." << std::endl;
        rerun_action(fd, action);
    }

    bool needs_rerun_stage2 = false;

    // ========================================================================
    // Stage-2 analysis passes
    // ========================================================================
    if (func_size < max_function_size) {
        // ---- Call graph analysis + pending reanalysis ----------------------
        if (type_registry) {
            ctx.register_function_signature(fd);

            fission::analysis::CallGraphAnalyzer call_analyzer(type_registry);
            call_analyzer.extract_calls(fd);
            int propagated = call_analyzer.propagate_types();
            if (propagated > 0) {
                fission::utils::log_stream() << "[AnalysisPipeline] CallGraph: propagated "
                    << propagated << " type hints" << std::endl;
            }

            std::set<uint64_t> processed;
            ghidra::Scope*     global_scope = arch->symboltab->getGlobalScope();
            const int          max_rounds   = 2;
            int rounds = 0, reanalyzed = 0;

            std::vector<uint64_t> pending =
                type_registry->consume_pending_reanalysis();
            while (!pending.empty() && rounds < max_rounds && global_scope) {
                ++rounds;
                for (uint64_t target_addr : pending) {
                    if (processed.count(target_addr)) continue;
                    processed.insert(target_addr);
                    if (!ctx.is_address_executable(target_addr)) continue;

                    ghidra::Address func_addr(arch->getDefaultCodeSpace(),
                        target_addr);
                    ghidra::Funcdata* target_fd =
                        global_scope->findFunction(func_addr);
                    if (!target_fd) {
                        ghidra::FunctionSymbol* sym =
                            global_scope->addFunction(func_addr,
                                "sub_" + std::to_string(target_addr));
                        if (!sym) continue;
                        target_fd = sym->getFunction();
                    }
                    if (!target_fd) continue;

                    try {
                        target_fd->clear();
                        ghidra::Address end_addr =
                            func_addr + fission::decompiler::k_callee_follow_limit;
                        target_fd->followFlow(func_addr, end_addr);
                        action->reset(*target_fd);
                        action->perform(*target_fd);
                    } catch (const ghidra::LowlevelError& e) {
                        fission::utils::log_stream()
                            << "[AnalysisPipeline] callgraph LowlevelError at 0x"
                            << std::hex << target_addr << ": " << e.explain
                            << std::endl;
                        continue;
                    } catch (const std::exception& e) {
                        fission::utils::log_stream()
                            << "[AnalysisPipeline] callgraph error at 0x"
                            << std::hex << target_addr << ": " << e.what()
                            << std::endl;
                        continue;
                    } catch (...) {
                        fission::utils::log_stream()
                            << "[AnalysisPipeline] callgraph unknown error at 0x"
                            << std::hex << target_addr << std::endl;
                        continue;
                    }

                    ctx.register_function_signature(target_fd);
                    call_analyzer.extract_calls(target_fd);
                    ++reanalyzed;
                }
                int newly_propagated = call_analyzer.propagate_types();
                if (newly_propagated <= 0) break;
                pending = type_registry->consume_pending_reanalysis();
            }

            if (reanalyzed > 0) {
                fission::utils::log_stream() << "[AnalysisPipeline] CallGraph: reanalyzed "
                    << reanalyzed << " pending functions." << std::endl;
                needs_rerun_stage2 = true;
            }
        }

        // ---- Cross-function type sharing -----------------------------------
        {
            fission::analysis::TypeSharing type_sharing(arch);
            std::vector<ghidra::Datatype*> param_types_ts;
            const ghidra::FuncProto& proto_ts = fd->getFuncProto();
            for (int i = 0; i < proto_ts.numParams(); ++i) {
                ghidra::ProtoParameter* param = proto_ts.getParam(i);
                if (param) param_types_ts.push_back(param->getType());
            }
            ghidra::ProtoParameter* ret_ts   = proto_ts.getOutput();
            ghidra::Datatype*       ret_type = ret_ts ? ret_ts->getType() : nullptr;
            type_sharing.register_function_types(
                fd->getAddress().getOffset(), param_types_ts, ret_type);
            int shared = type_sharing.share_types();
            if (shared > 0) {
                fission::utils::log_stream() << "[AnalysisPipeline] TypeSharing: shared "
                    << shared << " types" << std::endl;
            }
        }

        // ---- Pcode optimization bridge -------------------------------------
        if (fission::decompiler::PcodeOptimizationBridge::is_enabled()) {
            try {
                std::string optimized =
                    fission::decompiler::PcodeOptimizationBridge
                        ::extract_and_optimize(fd);
                if (!optimized.empty()) {
                    fission::utils::log_stream()
                        << "[AnalysisPipeline] PcodeOptimization: extracted & optimized ("
                        << optimized.size() << " bytes)" << std::endl;
                    if (fission::decompiler::PcodeExtractor::inject_pcode(
                            fd, optimized)) {
                        fission::utils::log_stream()
                            << "[AnalysisPipeline] PcodeOptimization: injected, "
                               "flagging stage-2 re-run." << std::endl;
                        needs_rerun_stage2 = true;
                    }
                }
            } catch (const std::exception& e) {
                fission::utils::log_stream()
                    << "[AnalysisPipeline] PcodeOptimization error: "
                    << e.what() << std::endl;
            } catch (...) {
                fission::utils::log_stream()
                    << "[AnalysisPipeline] PcodeOptimization unknown error"
                    << std::endl;
            }
        }

        // ---- Forward type propagation (API inference) ----------------------
        if (struct_registry) {
            TypePropagator type_propagator(arch, struct_registry);
            type_propagator.clear();
            int types_inferred = type_propagator.propagate(fd);
            bool struct_changed_after =
                type_propagator.propagate_struct_types(fd);
            if (types_inferred > 0 || struct_changed_after) {
                fission::utils::log_stream()
                    << "[AnalysisPipeline] Type propagation: "
                    << types_inferred << " type(s) inferred, flagging "
                       "stage-2 re-run." << std::endl;
                needs_rerun_stage2 = true;
            }
        }
    }

    // ========================================================================
    // Barrier-2: single re-run for all stage-2 changes
    // ========================================================================
    if (needs_rerun_stage2) {
        fission::utils::log_stream() << "[AnalysisPipeline] Stage-2 re-run "
            "(callgraph/pcode/type changes)." << std::endl;
        rerun_action(fd, action);
    }

    // Post-barrier: x86 32-bit cdecl double-arg synthesis.
    // merge_split_double_args modifies Pcode (CPUI_CALL inputs) so it must
    // run AFTER the last rerun_action; rerun_action calls fd->clear() which
    // would otherwise undo the Pcode modifications.
    {
        TypePropagator double_merger(arch, struct_registry);
        double_merger.merge_split_double_args(fd);
    }

    // ========================================================================
    // GAP-4: Jump/switch table target registration
    // After all Ghidra action passes, BRANCHIND switch tables have been
    // resolved by ActionSwitchNorm.  Collect their targets, register any
    // unknown ones as functions in the global scope so that cross-reference
    // analysis finds them, and surface the list in AnalysisArtifacts so the
    // Rust side can enqueue them for decompilation.
    // ========================================================================
    if (arch->symboltab) {
        ghidra::Scope* global_scope = arch->symboltab->getGlobalScope();
        if (global_scope) {
            int num_jt = fd->numJumpTables();
            for (int ti = 0; ti < num_jt; ++ti) {
                ghidra::JumpTable* jt = fd->getJumpTable(ti);
                if (!jt || !jt->isRecovered()) continue;
                int num_entries = jt->numEntries();
                for (int ei = 0; ei < num_entries; ++ei) {
                    ghidra::Address target = jt->getAddressByIndex(ei);
                    if (target.isInvalid()) continue;
                    uint64_t target_va = target.getOffset();
                    artifacts.jump_table_targets.push_back(target_va);
                    // Register as a named function stub if not already known
                    if (!global_scope->findAddr(target, fd->getAddress())) {
                        char jmp_name[32];
                        std::snprintf(jmp_name, sizeof(jmp_name),
                                      "case_%08llx",
                                      (unsigned long long)target_va);
                        try {
                            global_scope->addFunction(target, jmp_name);
                        } catch (const ghidra::RecovError&) {}
                    }
                }
            }
            if (!artifacts.jump_table_targets.empty()) {
                fission::utils::log_stream()
                    << "[AnalysisPipeline] Registered "
                    << artifacts.jump_table_targets.size()
                    << " jump-table target(s) from "
                    << num_jt << " table(s)." << std::endl;
            }
        }
    }

    return artifacts;
}

// ============================================================================
// Legacy API wrappers — thin adapters that forward to the unified path.
// ============================================================================

AnalysisArtifacts run_analysis_passes(
    fission::ffi::DecompContext* ctx,
    ghidra::Funcdata* fd,
    ghidra::Action* action,
    size_t max_function_size
) {
    if (!ctx) {
        return AnalysisArtifacts{};
    }
    FfiAnalysisContext ac(ctx);
    return run_analysis_passes(ac, fd, action, max_function_size);
}

AnalysisArtifacts run_analysis_passes(
    BatchAnalysisContext& ctx,
    ghidra::Funcdata* fd,
    ghidra::Action* action,
    size_t max_function_size
) {
    BatchAnalysisAdapter ac(ctx);
    return run_analysis_passes(ac, fd, action, max_function_size);
}

} // namespace decompiler
} // namespace fission
