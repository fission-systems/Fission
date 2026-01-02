#ifndef __CALL_GRAPH_ANALYZER_H__
#define __CALL_GRAPH_ANALYZER_H__

#include "fission/types/GlobalTypeRegistry.h"
#include <cstdint>
#include <map>
#include <vector>
#include <set>
#include <string>

namespace ghidra {
    class Funcdata;
    class Architecture;
}

namespace fission {
namespace analysis {

using namespace fission::types;

/**
 * CallGraphAnalyzer - Build and traverse call graph for type propagation
 * 
 * This class:
 * 1. Extracts CALL instructions from all functions
 * 2. Builds a complete call graph
 * 3. Propagates types from callees to callers (backward propagation)
 * 4. Propagates types from callers to callees (forward propagation)
 */
class CallGraphAnalyzer {
public:
    CallGraphAnalyzer(GlobalTypeRegistry* registry);
    ~CallGraphAnalyzer();

    /**
     * Analyze a function to extract call sites
     * @param fd The function to analyze
     */
    void extract_calls(ghidra::Funcdata* fd);
    
    /**
     * After analyzing all functions, propagate types across call graph
     * @return Number of functions that need re-analysis
     */
    int propagate_types();
    
    /**
     * Backward propagation: callee's param types -> caller's argument types
     * When we know a callee expects Struct*, we can type the caller's argument
     */
    int propagate_backward();
    
    /**
     * Forward propagation: caller's types -> callee's params
     * When we know the type of an argument at call site, propagate to callee
     */
    int propagate_forward();
    
    /**
     * Get all function addresses in the call graph
     */
    std::vector<uint64_t> get_all_functions() const;
    
    /**
     * Get topologically sorted functions (leaf functions first)
     * Useful for analysis order
     */
    std::vector<uint64_t> topological_sort() const;
    
    /**
     * Get call graph statistics
     */
    struct Stats {
        size_t total_functions;
        size_t total_calls;
        size_t functions_with_types;
        size_t propagations_done;
    };
    Stats get_stats() const;

private:
    GlobalTypeRegistry* registry;
    
    // All known function addresses
    std::set<uint64_t> all_functions;
    
    // Statistics
    size_t propagations_done = 0;
    
    // Helper: check if address looks like a valid function
    bool is_valid_function_addr(uint64_t addr) const;
};

} // namespace analysis
} // namespace fission

#endif
