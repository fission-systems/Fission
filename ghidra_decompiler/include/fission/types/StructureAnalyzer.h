#ifndef __STRUCTURE_ANALYZER_HL__
#define __STRUCTURE_ANALYZER_HL__

#include <vector>
#include <map>
#include <set>
#include <string>
#include <cstdint>

// Forward declarations from Ghidra
namespace ghidra {
    class Funcdata;
    class Varnode;
    class TypeFactory;
    class TypeStruct;
    class TypeUnion;
}

namespace fission {
namespace types {

struct StructureMember {
    int offset;
    int size;
    std::string name;
};

// Type hint for each field access
struct FieldInfo {
    int size = 1;
    bool is_float = false;
    bool is_pointer = false;
};

class StructureAnalyzer {
public:
    StructureAnalyzer();
    ~StructureAnalyzer();

    // Analyze a function to find potential structures
    // Returns true if any structures were inferred/updated
    bool analyze_function_structures(ghidra::Funcdata* fd);
    
    // Generate C-style struct definitions for all inferred structures
    // Returns multi-line string with typedef struct definitions
    std::string generate_struct_definitions() const;
    
    // Get type replacement map for post-processing output
    // Key: original type pattern (e.g., "DWORD *param_1")
    // Value: replacement pattern (e.g., "f_140001450_arg_8 *param_1")
    std::map<std::string, std::string> get_type_replacements() const;
    
    // Get the inferred struct types (for external access)
    const std::map<unsigned long long, ghidra::TypeStruct*>& get_inferred_structs() const {
        return inferred_structs;
    }

    // Get the inferred union types (for external access)
    const std::map<unsigned long long, ghidra::TypeUnion*>& get_inferred_unions() const {
        return inferred_unions;
    }

    // Generate C-style union definitions for all inferred unions
    std::string generate_union_definitions() const;

private:
    // Tracks offsets accessed for a given base varnode
    // Key: Base Varnode storage offset
    // Value: Map of field offset -> FieldInfo
    std::map<unsigned long long, std::map<int, FieldInfo>> access_map;

    // Tracks all distinct sizes observed per (base, offset) — for union detection
    // Key: base key  Value: offset -> set of observed sizes
    std::map<unsigned long long, std::map<int, std::set<int>>> size_variants;

    // Map of inferred structures (Base Address -> New Type)
    std::map<unsigned long long, ghidra::TypeStruct*> inferred_structs;

    // Map of inferred unions (Base Address -> New Type)
    std::map<unsigned long long, ghidra::TypeUnion*> inferred_unions;

    void collect_accesses(ghidra::Funcdata* fd);
    bool infer_structures(ghidra::TypeFactory* factory, uint64_t func_entry, int ptr_size);
    bool infer_unions(ghidra::TypeFactory* factory, uint64_t func_entry, int ptr_size);
    void apply_structures(ghidra::Funcdata* fd, int ptr_size);
};

} // namespace types
} // namespace fission

#endif
