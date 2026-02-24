#include "fission/analysis/CallGraphAnalyzer.h"

// Ghidra headers
#include "funcdata.hh"
#include "varnode.hh"
#include "op.hh"

#include <iostream>
#include "fission/utils/logger.h"
#include <queue>
#include <algorithm>

namespace fission {
namespace analysis {

CallGraphAnalyzer::CallGraphAnalyzer(GlobalTypeRegistry* reg) : registry(reg) {}
CallGraphAnalyzer::~CallGraphAnalyzer() {}

bool CallGraphAnalyzer::is_valid_function_addr(uint64_t addr) const {
    // Basic heuristics: non-zero, not too small, not obviously invalid
    return addr > 0x1000 && addr < 0x7FFFFFFFFFFFFFFF;
}

void CallGraphAnalyzer::extract_calls(ghidra::Funcdata* fd) {
    if (!fd || !registry) return;
    
    uint64_t caller_addr = fd->getAddress().getOffset();
    all_functions.insert(caller_addr);
    
    auto iter = fd->beginOpAll();
    auto end_iter = fd->endOpAll();
    
    for (; iter != end_iter; ++iter) {
        ghidra::PcodeOp* op = iter->second;
        if (!op || op->isDead()) continue;
        
        if (op->code() == ghidra::CPUI_CALL) {
            // Direct CALL(target, ...)
            ghidra::Varnode* target = op->getIn(0);
            if (target && target->isConstant()) {
                uint64_t callee_addr = target->getOffset();
                if (is_valid_function_addr(callee_addr)) {
                    all_functions.insert(callee_addr);
                    registry->register_call(caller_addr, callee_addr, op->getSeqNum().getAddr().getOffset());
                }
            }
        } else if (op->code() == ghidra::CPUI_CALLIND) {
            // A-1: Indirect CALL (function pointer / virtual call).
            // If the target varnode has been resolved to a constant (e.g. after
            // constant folding or vtable resolution), we can still record the
            // edge so that CallGraph type propagation reaches callee functions.
            ghidra::Varnode* target = op->getIn(0);
            if (target && target->isConstant()) {
                uint64_t callee_addr = target->getOffset();
                if (is_valid_function_addr(callee_addr)) {
                    all_functions.insert(callee_addr);
                    registry->register_call(caller_addr, callee_addr, op->getSeqNum().getAddr().getOffset());
                }
            }
            // Non-constant indirect calls (classic virtual dispatch) are not
            // resolved here; VTableAnalyzer handles those separately.
        }
    }
}

int CallGraphAnalyzer::propagate_types() {
    int total = 0;
    
    // First: backward propagation (callee's types -> caller's arguments)
    total += propagate_backward();
    
    // Then: forward propagation (caller's known types -> callee's params)
    total += propagate_forward();
    
    propagations_done += total;
    return total;
}

int CallGraphAnalyzer::propagate_backward() {
    int propagated = 0;
    
    // For each analyzed function with known param types
    for (uint64_t func_addr : all_functions) {
        const FunctionSignature* sig = registry->get_function_signature(func_addr);
        if (!sig || sig->params.empty()) continue;
        
        // Get all callers of this function
        std::vector<uint64_t> callers = registry->get_callers(func_addr);
        
        for (uint64_t caller : callers) {
            // If caller hasn't been fully analyzed or has outdated types,
            // mark it for re-analysis with new type information
            const FunctionSignature* caller_sig = registry->get_function_signature(caller);
            
            if (!caller_sig || !caller_sig->analyzed) {
                // Caller needs (re-)analysis with knowledge of callee's param types
                registry->mark_for_reanalysis(caller);
                propagated++;
                
                fission::utils::log_stream() << "[CallGraphAnalyzer] Backward: 0x" << std::hex << func_addr
                          << " types -> caller 0x" << caller << std::dec << std::endl;
            }
        }
    }
    
    return propagated;
}

int CallGraphAnalyzer::propagate_forward() {
    int propagated = 0;
    
    // For each caller with known argument types
    for (uint64_t caller : all_functions) {
        const FunctionSignature* caller_sig = registry->get_function_signature(caller);
        if (!caller_sig) continue;
        
        // Get all callees of this function
        std::vector<uint64_t> callees = registry->get_callees(caller);
        
        for (uint64_t callee : callees) {
            const FunctionSignature* callee_sig = registry->get_function_signature(callee);
            
            // If callee hasn't been analyzed, mark it for analysis
            if (!callee_sig || !callee_sig->analyzed) {
                registry->mark_for_reanalysis(callee);
                propagated++;
                
                fission::utils::log_stream() << "[CallGraphAnalyzer] Forward: caller 0x" << std::hex << caller
                          << " types -> 0x" << callee << std::dec << std::endl;
            }
        }
    }
    
    return propagated;
}

std::vector<uint64_t> CallGraphAnalyzer::get_all_functions() const {
    return std::vector<uint64_t>(all_functions.begin(), all_functions.end());
}

std::vector<uint64_t> CallGraphAnalyzer::topological_sort() const {
    // Simple BFS-based topological sort
    // Start from functions with no callees (leaf functions)
    std::vector<uint64_t> result;
    std::set<uint64_t> visited;
    std::queue<uint64_t> queue;
    
    // Find leaf functions (no callees)
    for (uint64_t func : all_functions) {
        std::vector<uint64_t> callees = registry->get_callees(func);
        if (callees.empty()) {
            queue.push(func);
        }
    }
    
    // BFS traversal
    while (!queue.empty()) {
        uint64_t current = queue.front();
        queue.pop();
        
        if (visited.count(current)) continue;
        visited.insert(current);
        result.push_back(current);
        
        // Add callers (reverse edge direction)
        std::vector<uint64_t> callers = registry->get_callers(current);
        for (uint64_t caller : callers) {
            if (!visited.count(caller)) {
                queue.push(caller);
            }
        }
    }
    
    // Add any remaining unvisited
    for (uint64_t func : all_functions) {
        if (!visited.count(func)) {
            result.push_back(func);
        }
    }
    
    return result;
}

CallGraphAnalyzer::Stats CallGraphAnalyzer::get_stats() const {
    Stats s;
    s.total_functions = all_functions.size();
    s.total_calls = registry->get_call_count();
    s.functions_with_types = 0;
    
    for (uint64_t func : all_functions) {
        const FunctionSignature* sig = registry->get_function_signature(func);
        if (sig && !sig->params.empty()) {
            s.functions_with_types++;
        }
    }
    
    s.propagations_done = propagations_done;
    return s;
}

} // namespace analysis
} // namespace fission
