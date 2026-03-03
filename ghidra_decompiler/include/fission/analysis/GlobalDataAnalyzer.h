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
 * GlobalDataAnalyzer - Analyze global variable access patterns.
 *
 * Scans all decompiled functions to find accesses to global memory
 * (.data, .bss, .rdata sections) and infers:
 *   - Struct layouts from multi-field access clusters
 *   - Scalar float/double symbols (GAP-1 fix: prevents raw hex emission)
 */
class GlobalDataAnalyzer {
public:
    GlobalDataAnalyzer();
    ~GlobalDataAnalyzer();

    /// Set the data section range (from binary loader)
    void set_data_section(uint64_t start, uint64_t end);

    /// Analyze a decompiled function for global accesses
    void analyze_function(ghidra::Funcdata* fd);

    /// After analyzing all functions, cluster accesses into structures and
    /// discover scalar float/double globals.
    void infer_structures();

    /// Create typed struct entries in the Ghidra type factory.
    /// @return Number of new struct types created.
    int create_types(ghidra::TypeFactory* factory, int ptr_size);

    /// Get inferred global structures (for debugging/reporting).
    const std::vector<GlobalStructure>& get_structures() const {
        return inferred_globals;
    }

    // -------------------------------------------------------------------------
    // GAP-1: Scalar float/double global symbol support
    // -------------------------------------------------------------------------

    /// A scalar (non-struct) float or double global.
    /// address: VA of the value in .rdata/.data
    /// size:    4 = float, 8 = double
    struct ScalarFloatEntry {
        uint64_t address;
        int      size;
    };

    /// All scalar float/double globals found during the most recent
    /// infer_structures() call.  These should be registered as typed
    /// DAT_<addr> symbols in the global scope so that Ghidra's LOAD
    /// type-propagation can replace raw hex constants with named values.
    const std::vector<ScalarFloatEntry>& get_scalar_floats() const {
        return scalar_floats_;
    }

    /// Clear all collected data (call before reusing the analyzer).
    void clear();

private:
    uint64_t data_section_start = 0;
    uint64_t data_section_end   = 0;

    std::vector<GlobalAccess>    accesses;
    std::vector<GlobalStructure> inferred_globals;
    std::vector<ScalarFloatEntry> scalar_floats_;

    bool is_in_data_section(uint64_t addr) const;
    std::map<uint64_t, std::vector<GlobalAccess>> cluster_by_base();
};

} // namespace analysis
} // namespace fission

#endif
