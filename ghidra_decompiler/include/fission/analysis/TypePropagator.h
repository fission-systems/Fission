#ifndef __TYPE_PROPAGATOR_H__
#define __TYPE_PROPAGATOR_H__

#include <map>
#include <set>
#include <string>
#include <vector>

// Forward declarations
namespace ghidra {
    class Architecture;
    class Funcdata;
    class Varnode;
    class PcodeOp;
    class Datatype;
    class TypeFactory;
}

namespace fission {
namespace analysis {

/// \brief Type Propagation Engine
///
/// Propagates types from known API calls back to function parameters
/// and local variables. Uses iterative dataflow analysis.
class TypePropagator {
private:
    ghidra::Architecture* arch;
    
    // Track type assignments: varnode unique ID -> inferred type
    std::map<uint64_t, ghidra::Datatype*> inferred_types;
    
    // Track which varnodes have been processed
    std::set<uint64_t> processed;
    
    /// Get varnode unique ID for tracking
    uint64_t get_varnode_id(ghidra::Varnode* vn);
    
    /// Propagate type from a CALL operation's parameters
    void propagate_from_call(ghidra::Funcdata* fd, ghidra::PcodeOp* call_op);
    
    /// Propagate type backwards through assignment chain
    void propagate_backwards(ghidra::Varnode* vn, ghidra::Datatype* type);
    
    /// Apply inferred types to high-level representation
    void apply_inferred_types(ghidra::Funcdata* fd);

public:
    TypePropagator(ghidra::Architecture* arch);
    ~TypePropagator();
    
    /// \brief Run type propagation on a function
    /// \param fd The function to analyze
    /// \return Number of types propagated
    int propagate(ghidra::Funcdata* fd);
    
    /// \brief Get inferred type for a varnode
    ghidra::Datatype* get_type(ghidra::Varnode* vn);
    
    /// \brief Clear all inferred types
    void clear();
};

} // namespace analysis
} // namespace fission

#endif // __TYPE_PROPAGATOR_H__
