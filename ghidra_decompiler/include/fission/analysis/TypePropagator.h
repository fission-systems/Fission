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
    class TypeStruct;
}

namespace fission {
namespace analysis {

/// \brief Type Propagation Engine
///
/// Propagates types from known API calls back to function parameters
/// and local variables. Uses iterative dataflow analysis.
/// Also handles struct-based type inference from global registry.
class TypePropagator {
private:
    ghidra::Architecture* arch;
    
    // Track type assignments: varnode unique ID -> inferred type
    std::map<uint64_t, ghidra::Datatype*> inferred_types;
    
    // Track which varnodes have been processed
    std::set<uint64_t> processed;
    
    // Struct registry: function address -> (param index -> struct name)
    std::map<uint64_t, std::map<int, std::string>>* struct_registry;
    
    /// Get varnode unique ID for tracking
    uint64_t get_varnode_id(ghidra::Varnode* vn);
    
    /// Propagate type from a CALL operation's parameters
    void propagate_from_call(ghidra::Funcdata* fd, ghidra::PcodeOp* call_op);
    
    /// Infer types from known Windows API patterns
    void infer_windows_api_types(ghidra::PcodeOp* call_op, const std::string& func_name);
    
    /// Propagate type backwards through assignment chain
    void propagate_backwards(ghidra::Varnode* vn, ghidra::Datatype* type);
    
    /// Propagate type across operation edge (Ghidra style)
    bool propagate_type_edge(ghidra::PcodeOp* op, int inslot, int outslot);

    /// Seed temporary types using local type inference (Ghidra style)
    void build_local_types(ghidra::Funcdata* fd);

    /// Apply inferred types to high-level representation
    void apply_inferred_types(ghidra::Funcdata* fd);
    
    /// Propagate one varnode's type across the function (Ghidra style)
    void propagate_one_type(ghidra::Varnode* vn);

public:
    TypePropagator(ghidra::Architecture* arch);
    TypePropagator(ghidra::Architecture* arch, 
                   std::map<uint64_t, std::map<int, std::string>>* registry);
    ~TypePropagator();
    
    /// \brief Run type propagation on a function
    /// \param fd The function to analyze
    /// \return Number of types propagated
    int propagate(ghidra::Funcdata* fd);
    
    /// \brief Get inferred type for a varnode
    ghidra::Datatype* get_type(ghidra::Varnode* vn);
    
    
    /// \brief Apply struct types from global registry
    /// Returns true if any types were changed
    bool propagate_struct_types(ghidra::Funcdata* fd);
    
    // Static utility functions (formerly in TypeEnhancer)
    
    /// \brief Apply inferred struct types to C code output
    static std::string apply_struct_types(
        std::string c_code,
        ghidra::Funcdata* fd,
        const std::map<unsigned long long, ghidra::TypeStruct*>& structs
    );
    
    /// \brief Select appropriate FID database filename
    static std::string get_fid_filename(bool is_64bit, const std::string& compiler_id);
    /// \brief Clear all inferred types
    void clear();
};

// Helper struct for propagation state tracking
struct PropagationState {
    ghidra::Varnode* vn;
    
    PropagationState(ghidra::Varnode* v) : vn(v) {}
};

} // namespace analysis
} // namespace fission

#endif // __TYPE_PROPAGATOR_H__
