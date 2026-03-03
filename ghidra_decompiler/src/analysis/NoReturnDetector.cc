/**
 * NoReturnDetector — evidence-based no-return function discovery.
 * See NoReturnDetector.h for design notes.
 */

#include "fission/analysis/NoReturnDetector.h"
#include "fission/utils/logger.h"

// Ghidra P-code headers
#include "funcdata.hh"
#include "op.hh"
#include "varnode.hh"
#include "block.hh"
#include "architecture.hh"
#include "database.hh"

#include <sstream>

namespace fission {
namespace analysis {

// ============================================================================
// Static no-return name list — mirrors Ghidra NonReturningFunctionNames
// ============================================================================

bool NoReturnDetector::is_static_no_return(const std::string& name) {
    // Strip leading underscores for normalisation (MSVC / GCC decoration)
    const char* p = name.c_str();
    while (*p == '_') ++p;

    static const char* const k_names[] = {
        // C stdlib
        "abort", "exit", "_exit", "quick_exit", "_Exit",
        // C++ exception
        "terminate", "unexpected", "bad_exception",
        "throw_bad_alloc", "throw_bad_cast",
        "__cxa_throw", "__cxa_rethrow", "__cxa_bad_cast",
        "__cxa_bad_typeid", "__cxa_pure_virtual",
        "__cxa_call_unexpected", "__cxa_call_terminate",
        // MSVC
        "_CxxThrowException", "__fastfail",
        "__report_rangecheckfailure", "__report_gsfailure",
        "longjmp", "_longjmp",
        // Linux / glibc
        "pthread_exit", "__assert_fail", "__assert_perror_fail",
        "__stack_chk_fail", "err", "errx", "verr", "verrx",
        "error", "error_at_line",
        // Windows
        "ExitProcess", "ExitThread", "TerminateProcess",
        "RaiseException", "FatalAppExitA", "FatalAppExitW",
        "FatalExit",
        nullptr
    };

    for (int i = 0; k_names[i]; ++i) {
        if (name == k_names[i] || std::string(p) == k_names[i])
            return true;
    }
    return false;
}

// ============================================================================
// Evidence collection
// ============================================================================

void NoReturnDetector::collect_evidence(ghidra::Funcdata* fd) {
    if (!fd) return;

    const ghidra::BlockGraph& cfg = fd->getBasicBlocks();
    int n_blocks = cfg.getSize();

    for (int bi = 0; bi < n_blocks; ++bi) {
        ghidra::FlowBlock* blk = cfg.getBlock(bi);
        if (!blk || blk->getType() != ghidra::FlowBlock::t_basic)
            continue;

        // Find CALL ops in this block
        ghidra::BlockBasic* bblk = static_cast<ghidra::BlockBasic*>(blk);

        for (auto it = bblk->beginOp(); it != bblk->endOp(); ++it) {
            ghidra::PcodeOp* op = *it;
            if (!op || op->isDead()) continue;

            ghidra::OpCode opc = op->code();
            if (opc != ghidra::CPUI_CALL && opc != ghidra::CPUI_CALLIND)
                continue;

            // Resolve callee address
            uint64_t callee_addr = 0;
            if (opc == ghidra::CPUI_CALL) {
                ghidra::Varnode* tgt = op->getIn(0);
                if (tgt && tgt->isConstant())
                    callee_addr = tgt->getOffset();
            } else {
                // CALLIND — only if target is already constant
                ghidra::Varnode* tgt = op->getIn(0);
                if (tgt && tgt->isConstant())
                    callee_addr = tgt->getOffset();
            }
            if (callee_addr == 0) continue;

            // If the block has NO fall-through successor, the code after
            // this CALL is unreachable — strong no-return evidence.
            bool has_fallthrough = false;
            for (int si = 0; si < blk->sizeOut(); ++si) {
                ghidra::FlowBlock* succ = blk->getOut(si);
                if (!succ) continue;
                // A "fall-through" edge is one where the successor's
                // start address immediately follows the current block.
                // In Ghidra block terms, any out-edge that is NOT a
                // conditional or unconditional jump counts.
                has_fallthrough = true; // any successor still means reachable
                break;
            }

            // The block ends at CALL with no successors: definitive evidence
            bool ends_at_call = false;
            {
                auto last = bblk->endOp();
                if (last != bblk->beginOp()) {
                    --last;
                    ghidra::PcodeOp* last_op = *last;
                    if (last_op == op)
                        ends_at_call = true;
                }
            }

            if (ends_at_call && !has_fallthrough) {
                evidence_[callee_addr]++;
            }
        }
    }
}

// ============================================================================
// Apply confirmed no-return to architecture
// ============================================================================

int NoReturnDetector::apply(ghidra::Architecture* arch, int threshold) {
    if (!arch) return 0;
    if (threshold <= 0) threshold = k_threshold;

    ghidra::Scope* global_scope =
        (arch->symboltab ? arch->symboltab->getGlobalScope() : nullptr);

    int marked = 0;

    // 1. Apply evidence-based candidates (functions with no fall-through callers
    //    >= threshold times — they are very likely no-return).
    for (const auto& kv : evidence_) {
        uint64_t addr  = kv.first;
        int      count = kv.second;

        if (count < threshold) continue;
        if (confirmed_.count(addr)) continue;
        if (!global_scope) continue;

        ghidra::Address func_addr(arch->getDefaultCodeSpace(), addr);
        ghidra::Funcdata* fd = global_scope->findFunction(func_addr);

        if (!fd) {
            // Create a minimal stub symbol so we can set the prototype attribute
            std::ostringstream ss;
            ss << "noret_" << std::hex << addr;
            ghidra::FunctionSymbol* sym =
                global_scope->addFunction(func_addr, ss.str());
            if (sym) fd = sym->getFunction();
        }
        if (!fd) continue;

        if (!fd->getFuncProto().isNoReturn()) {
            fd->getFuncProto().setNoReturn(true);
            confirmed_.insert(addr);
            ++marked;
            fission::utils::log_stream()
                << "[NoReturnDetector] Evidence-based noreturn: 0x"
                << std::hex << addr
                << " (evidence=" << std::dec << count << ")\n";
        }
    }

    // 2. Apply static list: scan all already-known functions by name.
    //    This catches runtime-resolved symbols (e.g. from IAT) that have been
    //    registered in the symbol table before analysis runs.
    if (global_scope) {
        ghidra::MapIterator it  = global_scope->begin();
        ghidra::MapIterator end = global_scope->end();
        for (; it != end; ++it) {
            const ghidra::SymbolEntry* entry = *it;
            if (!entry) continue;
            ghidra::Symbol* sym = entry->getSymbol();
            if (!sym) continue;
            // FunctionSymbol is the only Symbol subclass that wraps a Funcdata.
            // A dynamic_cast is the correct way to identify it in Ghidra 11.4.2
            // (there is no 'function_sym' enum value on Symbol::getType()).
            auto* fsym = dynamic_cast<ghidra::FunctionSymbol*>(sym);
            if (!fsym) continue;
            ghidra::Funcdata* fd = fsym->getFunction();
            if (!fd) continue;

            if (fd->getFuncProto().isNoReturn()) continue; // already set

            if (is_static_no_return(sym->getName())) {
                fd->getFuncProto().setNoReturn(true);
                ++marked;
                // Retrieve the storage address via the first whole mapping entry.
                // Symbol::getAddr() does not exist in Ghidra 11.4.2; the address
                // lives in SymbolEntry, which is returned by getFirstWholeMap().
                ghidra::SymbolEntry* first_entry = sym->getFirstWholeMap();
                fission::utils::log_stream()
                    << "[NoReturnDetector] Static noreturn: "
                    << sym->getName() << " at 0x"
                    << std::hex
                    << (first_entry ? first_entry->getAddr().getOffset() : 0ULL)
                    << std::dec << "\n";
            }
        }
    }

    return marked;
}

// ============================================================================
// Accessors / reset
// ============================================================================

int NoReturnDetector::evidence_count(uint64_t callee_addr) const {
    auto it = evidence_.find(callee_addr);
    return it != evidence_.end() ? it->second : 0;
}

void NoReturnDetector::reset() {
    evidence_.clear();
    confirmed_.clear();
}

} // namespace analysis
} // namespace fission
