#ifndef __STACK_FRAME_ANALYZER_H__
#define __STACK_FRAME_ANALYZER_H__

#include <map>
#include <vector>
#include <string>

namespace ghidra {
    class Funcdata;
    class Architecture;
    class TypeFactory;
    class TypeStruct;
    class Datatype;
}

namespace fission {
namespace analysis {

/// \brief Stack variable cluster for structure inference
struct StackCluster {
    int64_t base_offset;            ///< Base stack offset
    int64_t size;                   ///< Total size of cluster
    std::string inferred_name;      ///< Auto-generated name
    
    struct Member {
        int64_t offset;             ///< Offset within cluster
        int size;                   ///< Size of access
        std::string name;           ///< Variable name if known
        ghidra::Datatype* type;     ///< Inferred type
    };
    std::vector<Member> members;
};

/// \brief Stack Frame Analyzer
///
/// Groups stack variables into logical structures based on access patterns.
class StackFrameAnalyzer {
private:
    ghidra::Architecture* arch;
    
    // Stack access tracking: offset -> (size, count)
    std::map<int64_t, std::pair<int, int>> stack_accesses;
    
    // Detected clusters
    std::vector<StackCluster> clusters;
    
    /// Collect all stack memory accesses
    void collect_stack_accesses(ghidra::Funcdata* fd);
    
    /// Group accesses into clusters (contiguous offsets)
    void cluster_accesses();
    
    /// Create structure type for a cluster
    ghidra::TypeStruct* create_struct_for_cluster(
        ghidra::TypeFactory* tf, 
        const StackCluster& cluster
    );

public:
    StackFrameAnalyzer(ghidra::Architecture* arch);
    ~StackFrameAnalyzer();
    
    /// \brief Analyze stack frame of a function
    /// \param fd Function to analyze
    /// \return Number of stack structures detected
    int analyze(ghidra::Funcdata* fd);
    
    /// \brief Get detected clusters
    const std::vector<StackCluster>& get_clusters() const { return clusters; }

    /// \brief Build a map of stack base offsets to struct types
    std::map<int64_t, ghidra::TypeStruct*> build_struct_map(ghidra::TypeFactory* tf);

    /// \brief Apply detected structures to type factory
    void apply_structures(ghidra::TypeFactory* tf);
    
    /// \brief Clear analysis state
    void clear();
};

} // namespace analysis
} // namespace fission

#endif // __STACK_FRAME_ANALYZER_H__
