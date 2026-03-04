#ifndef __TYPE_PROPAGATOR_H__
#define __TYPE_PROPAGATOR_H__

#include <cstdint>
#include <map>
#include <set>
#include <unordered_map>
#include <unordered_set>
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
    std::unordered_map<uint64_t, ghidra::Datatype*> inferred_types;
    
    // Track which varnodes have been processed
    std::unordered_set<uint64_t> processed;
    
    // Struct registry: function address -> (param index -> struct name)
    std::map<uint64_t, std::map<int, std::string>>* struct_registry;
    
    // Iteration counter for type propagation (Ghidra uses max 7)
    int local_count;

    // A-2: compiler/platform identifier ("windows", "gcc", "clang", ...).
    // Determines which platform-specific API type inference rules apply.
    std::string compiler_id_;
    
    /// Get varnode unique ID for tracking
    uint64_t get_varnode_id(ghidra::Varnode* vn);
    
    /// Propagate type from a CALL operation's parameters
    void propagate_from_call(ghidra::Funcdata* fd, ghidra::PcodeOp* call_op);
    
    /// Infer types from known Windows API patterns
    void infer_windows_api_types(ghidra::PcodeOp* call_op, const std::string& func_name);

    /// A-2: Infer types from POSIX / standard C API patterns (ELF / Mach-O)
    void infer_posix_api_types(ghidra::PcodeOp* call_op, const std::string& func_name);
    
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
    
    /// Detect pointer usage in stack variables and apply pointer types
    void infer_stack_pointer_types(ghidra::Funcdata* fd);
    
    /// Detect pointer usage in function parameters and apply pointer types
    void infer_parameter_pointer_types(ghidra::Funcdata* fd);
    
    /// Write back temporary types to permanent fields (Ghidra style)
    bool write_back(ghidra::Funcdata* fd);

    /// Select the CPUI_RETURN op whose input varnode carries the most-specific
    /// TempType (mirrors Ghidra ActionInferTypes::canonicalReturnOp).
    ghidra::PcodeOp* canonical_return_op(ghidra::Funcdata* fd);

    /// Synchronise TempTypes across all CPUI_RETURN ops so that the
    /// most-specific inferred return type is propagated to every exit path
    /// (mirrors Ghidra ActionInferTypes::propagateAcrossReturns).
    void propagate_across_returns(ghidra::Funcdata* fd);
    
public:
    /// Propagate call return types using FuncCallSpecs
    void propagate_call_return_types(ghidra::Funcdata* fd);

    /// x86 32-bit cdecl: merge pairs of 4-byte constant call inputs that
    /// correspond to a single double (8-byte float) callee parameter.
    /// Replaces the two inputs with one 8-byte constant and removes the
    /// extra slot so the C printer emits one argument instead of two.
    /// Must be called AFTER the final rerun_action() since rerun clears Pcode.
    void merge_split_double_args(ghidra::Funcdata* fd);
    
    /// Propagate inferred types to update structure definitions
    bool propagate_struct_types(ghidra::Funcdata* fd);

    /// Maximum iterations for type propagation (from Ghidra)
    static const int MAX_TYPE_ITERATIONS = 7;
    TypePropagator(ghidra::Architecture* arch);
    TypePropagator(ghidra::Architecture* arch, 
                   std::map<uint64_t, std::map<int, std::string>>* registry);
    ~TypePropagator();

    /// A-2: Set compiler/platform identifier for platform-specific API inference.
    void set_compiler_id(const std::string& id) { compiler_id_ = id; }

    /// \brief Seed Ghidra's type system BEFORE action->perform().
    ///
    /// Calls ScopeLocal::addTypeRecommendation() for each CALL target whose
    /// prototype is known from Windows/POSIX API tables.  Because this runs
    /// before ActionInferTypes, the recommendations participate in Ghidra's
    /// own 7-iteration type propagation loop instead of being applied only
    /// after it has already converged.
    ///
    /// \param fd Function to process (must have had followFlow called on it)
    void seed_before_action(ghidra::Funcdata* fd);
    
    /// \brief Run type propagation on a function
    /// \param fd The function to analyze
    /// \return Number of types propagated
    int propagate(ghidra::Funcdata* fd);
    
    /// \brief Get inferred type for a varnode
    ghidra::Datatype* get_type(ghidra::Varnode* vn);
    
    
    /// \brief Apply struct types from global registry
    /// Returns true if any types were changed
    /// (duplicate declaration removed)
    
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
