#include "fission/analysis/TypeSharing.h"
#include "funcdata.hh"
#include "op.hh"
#include "architecture.hh"
#include "database.hh"
#include "fspec.hh"
#include "type.hh"
#include <iostream>
#include <algorithm>
#include "fission/utils/logger.h"

namespace fission {
namespace analysis {

using namespace ghidra;

TypeSharing::TypeSharing(Architecture* a) : arch(a) {}
TypeSharing::~TypeSharing() {}

// ---------------------------------------------------------------------------
// build_call_graph
//
// For every function address recorded in func_param_types, finds the
// corresponding Funcdata (if it has been through at least one decompilation
// pass) and walks its alive Pcode ops to discover CALL / CALLIND targets.
// Results are stored in call_graph[caller] = { callee0, callee1, ... }.
// ---------------------------------------------------------------------------
void TypeSharing::build_call_graph() {
    Scope* global = arch->symboltab->getGlobalScope();
    if (!global) {
        fission::utils::log_stream() << "[TypeSharing] No global scope available" << std::endl;
        return;
    }

    fission::utils::log_stream() << "[TypeSharing] Building call graph from "
                                 << func_param_types.size()
                                 << " registered function(s)..." << std::endl;

    for (const auto& [caller_addr, param_vec] : func_param_types) {
        Address func_addr(arch->getDefaultCodeSpace(), caller_addr);
        Funcdata* caller_fd = global->findFunction(func_addr);
        if (!caller_fd || !caller_fd->isProcStarted()) continue;

        for (auto iter = caller_fd->beginOpAlive();
             iter != caller_fd->endOpAlive(); ++iter) {
            PcodeOp* op = *iter;
            if (!op) continue;
            if (op->code() != CPUI_CALL && op->code() != CPUI_CALLIND) continue;

            uint64_t callee_addr = 0;
            if (FuncCallSpecs* fc = caller_fd->getCallSpecs(op)) {
                callee_addr = fc->getEntryAddress().getOffset();
            } else if (op->code() == CPUI_CALL && op->numInput() > 0) {
                Varnode* target = op->getIn(0);
                if (target && target->isConstant()) {
                    callee_addr = target->getOffset();
                }
            }

            if (callee_addr == 0) continue;
            call_graph[caller_addr].insert(callee_addr);
        }
    }

    fission::utils::log_stream() << "[TypeSharing] Call graph built: "
                                 << call_graph.size() << " caller(s)" << std::endl;
}

// ---------------------------------------------------------------------------
// propagate_to_callers
//
// Backward propagation: callee knows its parameter types — push those as
// type hints onto the argument varnodes at every call-site inside each
// known caller.  Uses setTempType so we don't lock anything permanently.
// Returns the number of varnodes that received a new type hint.
// ---------------------------------------------------------------------------
int TypeSharing::propagate_to_callers(uint64_t callee_addr) {
    int propagated = 0;

    auto param_it = func_param_types.find(callee_addr);
    if (param_it == func_param_types.end()) return 0;
    const std::vector<Datatype*>& callee_params = param_it->second;
    if (callee_params.empty()) return 0;

    Scope* global = arch->symboltab->getGlobalScope();
    if (!global) return 0;

    // Walk every recorded caller → look for CALL ops targeting callee_addr
    for (const auto& [caller_addr, callees] : call_graph) {
        if (callees.find(callee_addr) == callees.end()) continue;

        Address caller_func_addr(arch->getDefaultCodeSpace(), caller_addr);
        Funcdata* caller_fd = global->findFunction(caller_func_addr);
        if (!caller_fd || !caller_fd->isProcStarted()) continue;

        for (auto iter = caller_fd->beginOpAlive();
             iter != caller_fd->endOpAlive(); ++iter) {
            PcodeOp* op = *iter;
            if (!op) continue;
            if (op->code() != CPUI_CALL && op->code() != CPUI_CALLIND) continue;

            FuncCallSpecs* fc = caller_fd->getCallSpecs(op);
            if (!fc) continue;
            if (fc->getEntryAddress().getOffset() != callee_addr) continue;

            // op->getIn(0) = callee target, getIn(1..N) = actual arguments
            int num_args   = op->numInput() - 1;
            int num_params = static_cast<int>(callee_params.size());
            int limit      = std::min(num_args, num_params);

            for (int i = 0; i < limit; ++i) {
                Datatype* hint = callee_params[i];
                if (!hint || hint->getMetatype() == TYPE_UNKNOWN) continue;

                Varnode* arg_vn = op->getIn(i + 1);
                if (!arg_vn) continue;

                // Only apply when the varnode doesn't already carry a
                // concrete (non-unknown) type.
                Datatype* cur = arg_vn->getType();
                if (cur && cur->getMetatype() != TYPE_UNKNOWN) continue;

                try {
                    arg_vn->setTempType(hint);
                    ++propagated;
                } catch (...) {
                    // setTempType can throw on stale varnodes; ignore.
                }
            }
        }
    }

    return propagated;
}

// ---------------------------------------------------------------------------
// propagate_to_callees
//
// Forward propagation: caller has typed arguments — harvest the concrete
// types of actual argument varnodes at each call-site and register them
// into func_param_types for the callee so subsequent backward passes can
// push them further.
// Returns the number of callee parameter slots newly populated.
// ---------------------------------------------------------------------------
int TypeSharing::propagate_to_callees(uint64_t caller_addr) {
    int propagated = 0;

    auto graph_it = call_graph.find(caller_addr);
    if (graph_it == call_graph.end()) return 0;

    Scope* global = arch->symboltab->getGlobalScope();
    if (!global) return 0;

    Address caller_func_addr(arch->getDefaultCodeSpace(), caller_addr);
    Funcdata* caller_fd = global->findFunction(caller_func_addr);
    if (!caller_fd || !caller_fd->isProcStarted()) return 0;

    for (auto iter = caller_fd->beginOpAlive();
         iter != caller_fd->endOpAlive(); ++iter) {
        PcodeOp* op = *iter;
        if (!op) continue;
        if (op->code() != CPUI_CALL && op->code() != CPUI_CALLIND) continue;

        FuncCallSpecs* fc = caller_fd->getCallSpecs(op);
        if (!fc) continue;

        uint64_t callee_addr = fc->getEntryAddress().getOffset();
        if (graph_it->second.find(callee_addr) == graph_it->second.end()) continue;

        // Harvest argument types from actual call arguments
        int num_args = op->numInput() - 1;
        if (num_args <= 0) continue;

        std::vector<Datatype*>& callee_entry = func_param_types[callee_addr];
        if (callee_entry.size() < static_cast<size_t>(num_args)) {
            callee_entry.resize(num_args, nullptr);
        }

        for (int i = 0; i < num_args; ++i) {
            if (callee_entry[i] && callee_entry[i]->getMetatype() != TYPE_UNKNOWN) {
                continue; // Already known — don't overwrite
            }
            Varnode* arg_vn = op->getIn(i + 1);
            if (!arg_vn) continue;

            Datatype* arg_type = arg_vn->getType();
            if (!arg_type || arg_type->getMetatype() == TYPE_UNKNOWN) continue;

            callee_entry[i] = arg_type;
            ++propagated;
        }
    }

    return propagated;
}

int TypeSharing::share_types() {
    int total_shared = 0;

    build_call_graph();

    if (call_graph.empty()) {
        fission::utils::log_stream() << "[TypeSharing] No call edges found, skipping propagation" << std::endl;
        return 0;
    }

    const int MAX_ITERATIONS = 5;
    for (int iter = 0; iter < MAX_ITERATIONS; ++iter) {
        int round_shared = 0;

        // Forward pass: caller → callee (harvest caller arg types for callees)
        for (const auto& [caller_addr, callees] : call_graph) {
            round_shared += propagate_to_callees(caller_addr);
        }

        // Backward pass: callee → caller (push callee param types to call sites)
        for (const auto& [func_addr, params] : func_param_types) {
            round_shared += propagate_to_callers(func_addr);
        }

        fission::utils::log_stream() << "[TypeSharing] Iteration " << iter
                                     << ": propagated " << round_shared << " type(s)" << std::endl;

        total_shared += round_shared;
        if (round_shared == 0) break; // Fixpoint reached
    }

    fission::utils::log_stream() << "[TypeSharing] Total types shared: " << total_shared << std::endl;
    return total_shared;
}

void TypeSharing::register_function_types(
    uint64_t func_addr,
    const std::vector<Datatype*>& params,
    Datatype* return_type
) {
    if (!params.empty()) {
        func_param_types[func_addr] = params;
    }
    if (return_type) {
        func_return_types[func_addr] = return_type;
    }
}

void TypeSharing::clear() {
    call_graph.clear();
    func_param_types.clear();
    func_return_types.clear();
}

} // namespace analysis
} // namespace fission
