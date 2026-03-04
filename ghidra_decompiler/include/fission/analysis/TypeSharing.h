#ifndef __TYPE_SHARING_H__
#define __TYPE_SHARING_H__

#include <cstdint>
#include <map>
#include <set>
#include <vector>
#include <string>

namespace ghidra {
    class Architecture;
    class Funcdata;
    class Datatype;
}

namespace fission {
namespace analysis {

/// \brief Cross-function Type Sharing
///
/// Propagates discovered types across the call graph to improve
/// type consistency between callers and callees.
class TypeSharing {
private:
    ghidra::Architecture* arch;
    
    // Call graph: caller -> set of callee addresses
    std::map<uint64_t, std::set<uint64_t>> call_graph;
    
    // Function types: address -> (param types, return type)
    std::map<uint64_t, std::vector<ghidra::Datatype*>> func_param_types;
    std::map<uint64_t, ghidra::Datatype*> func_return_types;
    
    /// Build call graph from all registered functions
    void build_call_graph();
    
    /// Propagate types from callee to caller (backwards); returns propagation count
    int propagate_to_callers(uint64_t callee_addr);
    
    /// Propagate types from caller to callee (forwards); returns propagation count
    int propagate_to_callees(uint64_t caller_addr);

public:
    TypeSharing(ghidra::Architecture* arch);
    ~TypeSharing();
    
    /// \brief Run type sharing across all analyzed functions
    /// \return Number of types shared
    int share_types();
    
    /// \brief Add function's discovered types
    void register_function_types(
        uint64_t func_addr,
        const std::vector<ghidra::Datatype*>& params,
        ghidra::Datatype* return_type
    );
    
    /// \brief Clear all state
    void clear();
};

} // namespace analysis
} // namespace fission

#endif // __TYPE_SHARING_H__
