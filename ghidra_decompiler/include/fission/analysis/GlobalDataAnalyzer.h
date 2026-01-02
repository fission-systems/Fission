#ifndef __GLOBAL_DATA_ANALYZER_H__
#define __GLOBAL_DATA_ANALYZER_H__

#include <cstdint>
#include <map>
#include <vector>
#include <string>

namespace ghidra {
    class Funcdata;
    class TypeFactory;
    class TypeStruct;
}

namespace fission {
namespace analysis {

/**
 * Information about a global variable access
 */
struct GlobalAccess {
    uint64_t address;       // Base address of global
    int offset;             // Offset within structure
    int size;               // Size of access
    bool is_read;           // Read or write
    bool is_float;          // FPU operation
    bool is_pointer;        // Used as pointer
    uint64_t from_function; // Accessing function
};

/**
 * Inferred global structure
 */
struct GlobalStructure {
    uint64_t address;
    int total_size;
    std::string name;
    std::map<int, int> fields; // offset -> size
    std::map<int, bool> float_fields;
    std::map<int, bool> pointer_fields;
};

/**
 * GlobalDataAnalyzer - Analyze global variable access patterns
 * 
 * Scans all decompiled functions to find accesses to global memory
 * (.data, .bss sections) and infers structure layouts.
 */
class GlobalDataAnalyzer {
public:
    GlobalDataAnalyzer();
    ~GlobalDataAnalyzer();

    /**
     * Set the data section range (from binary loader)
     */
    void set_data_section(uint64_t start, uint64_t end);
    
    /**
     * Analyze a decompiled function for global accesses
     */
    void analyze_function(ghidra::Funcdata* fd);
    
    /**
     * After analyzing all functions, cluster accesses into structures
     */
    void infer_structures();
    
    /**
     * Create types in the type factory
     * @return Number of structures created
     */
    int create_types(ghidra::TypeFactory* factory, int ptr_size);
    
    /**
     * Get inferred structures for debugging/reporting
     */
    const std::vector<GlobalStructure>& get_structures() const { return inferred_globals; }
    
    /**
     * Clear all collected data
     */
    void clear();

private:
    uint64_t data_section_start = 0;
    uint64_t data_section_end = 0;
    
    // All collected global accesses
    std::vector<GlobalAccess> accesses;
    
    // Inferred global structures
    std::vector<GlobalStructure> inferred_globals;
    
    // Check if address is in data section
    bool is_in_data_section(uint64_t addr) const;
    
    // Cluster accesses by base address
    std::map<uint64_t, std::vector<GlobalAccess>> cluster_by_base();
};

} // namespace analysis
} // namespace fission

#endif
