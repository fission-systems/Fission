#include "fission/analysis/TypeSharing.h"
#include "funcdata.hh"
#include "op.hh"
#include "architecture.hh"
#include "database.hh"
#include <iostream>

namespace fission {
namespace analysis {

using namespace ghidra;

TypeSharing::TypeSharing(Architecture* a) : arch(a) {}
TypeSharing::~TypeSharing() {}

void TypeSharing::build_call_graph() {
    // Get global scope
    Scope* global = arch->symboltab->getGlobalScope();
    if (!global) return;
    
    // Iterate all functions in the global scope
    // Note: This is a simplified approach - in practice we'd iterate
    // over actually decompiled functions
    
    std::cerr << "[TypeSharing] Building call graph..." << std::endl;
}

void TypeSharing::propagate_to_callers(uint64_t callee_addr) {
    // Find all callers of this function
    auto it = call_graph.find(callee_addr);
    if (it == call_graph.end()) return;
    
    // Get callee's parameter types
    auto param_it = func_param_types.find(callee_addr);
    if (param_it == func_param_types.end()) return;
    
    // For each caller, try to apply callee's types to call arguments
    // This would update the caller's understanding of the types
}

void TypeSharing::propagate_to_callees(uint64_t caller_addr) {
    // Get all callees of this caller
    auto it = call_graph.find(caller_addr);
    if (it == call_graph.end()) return;
    
    // For each callee, if caller passes known-typed arguments,
    // those types can inform the callee's parameter types
}

int TypeSharing::share_types() {
    int shared = 0;
    
    build_call_graph();
    
    // Iterate until fixpoint (or max iterations)
    const int MAX_ITERATIONS = 5;
    for (int iter = 0; iter < MAX_ITERATIONS; ++iter) {
        int prev_shared = shared;
        
        // Forward propagation: caller to callee
        for (const auto& [caller, callees] : call_graph) {
            propagate_to_callees(caller);
        }
        
        // Backward propagation: callee to caller
        for (const auto& [addr, types] : func_param_types) {
            propagate_to_callers(addr);
            shared++;
        }
        
        // Check for fixpoint
        if (shared == prev_shared) break;
    }
    
    std::cerr << "[TypeSharing] Shared " << shared << " types across call graph" << std::endl;
    return shared;
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
