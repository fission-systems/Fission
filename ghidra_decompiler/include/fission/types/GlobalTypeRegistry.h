#ifndef __GLOBAL_TYPE_REGISTRY_H__
#define __GLOBAL_TYPE_REGISTRY_H__

#include <cstdint>
#include <map>
#include <unordered_set>
#include <vector>
#include <string>

namespace ghidra {
    class TypeStruct;
}

namespace fission {
namespace types {

/**
 * Parameter type information learned from function analysis
 */
struct ParamTypeInfo {
    int param_index;                    // 0-based parameter index
    ghidra::TypeStruct* struct_type;    // Inferred structure type
    bool is_pointer;                    // Is this a pointer to struct?
    std::string type_name;              // Human-readable name
};

/**
 * Function signature information
 */
struct FunctionSignature {
    uint64_t address;
    std::string name;
    std::vector<ParamTypeInfo> params;
    ghidra::TypeStruct* return_type;    // If return is struct pointer
    bool analyzed;
};

/**
 * Call site information for type propagation
 */
struct CallSite {
    uint64_t caller_addr;       // Calling function
    uint64_t callee_addr;       // Called function
    int call_instruction_addr;  // Address of CALL instruction
};

/**
 * GlobalTypeRegistry - Cross-function type storage and propagation
 * 
 * Maintains a global database of inferred types for all analyzed functions,
 * enabling type propagation across the call graph.
 */
class GlobalTypeRegistry {
public:
    GlobalTypeRegistry();
    ~GlobalTypeRegistry();

    /**
     * Register a function's learned parameter types
     */
    void register_function_types(uint64_t func_addr, const FunctionSignature& sig);
    
    /**
     * Get known parameter types for a function (if any)
     */
    const FunctionSignature* get_function_signature(uint64_t func_addr) const;
    
    /**
     * Register a call site for later propagation
     */
    void register_call(uint64_t caller, uint64_t callee, int call_addr);
    
    /**
     * Get all callers of a function
     */
    std::vector<uint64_t> get_callers(uint64_t callee_addr) const;
    
    /**
     * Get all callees of a function
     */
    std::vector<uint64_t> get_callees(uint64_t caller_addr) const;
    
    /**
     * Check if a function has been analyzed
     */
    bool is_analyzed(uint64_t func_addr) const;
    
    /**
     * Mark a function as needing re-analysis
     */
    void mark_for_reanalysis(uint64_t func_addr);
    
    /**
     * Get functions marked for re-analysis
     */
    std::vector<uint64_t> get_pending_reanalysis() const;

    /**
     * Consume and clear functions marked for re-analysis
     */
    std::vector<uint64_t> consume_pending_reanalysis();
    
    /**
     * Clear all data
     */
    void clear();
    
    /**
     * Get statistics
     */
    size_t get_function_count() const { return signatures.size(); }
    size_t get_call_count() const { return call_sites.size(); }

private:
    // Function address -> Signature
    std::map<uint64_t, FunctionSignature> signatures;
    
    // All call sites
    std::vector<CallSite> call_sites;
    
    // Callee -> list of callers (for backward propagation)
    std::map<uint64_t, std::vector<uint64_t>> callers_map;
    
    // Caller -> list of callees (for forward propagation)
    std::map<uint64_t, std::vector<uint64_t>> callees_map;
    
    // Functions needing re-analysis (unordered_set for O(1) duplicate check)
    std::unordered_set<uint64_t> pending_reanalysis;
};

} // namespace types
} // namespace fission

#endif
