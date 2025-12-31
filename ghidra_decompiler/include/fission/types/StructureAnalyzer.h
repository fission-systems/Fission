#ifndef __STRUCTURE_ANALYZER_HL__
#define __STRUCTURE_ANALYZER_HL__

#include <vector>
#include <map>
#include <set>
#include <string>

// Forward declarations from Ghidra
namespace ghidra {
    class Funcdata;
    class Varnode;
    class TypeFactory;
    class TypeStruct;
}

namespace fission {
namespace types {

struct StructureMember {
    int offset;
    int size;
    std::string name;
};

class StructureAnalyzer {
public:
    StructureAnalyzer();
    ~StructureAnalyzer();

    // Analyze a function to find potential structures
    // Returns true if any structures were inferred/updated
    bool analyze_function_structures(ghidra::Funcdata* fd);

private:
    // Tracks offsets accessed for a given base varnode
    // Key: Base Varnode UID (or just pointer if suitable, but Varnodes die)
    // Actually we want to map Input Storage (Address) to Offsets
    // map<StorageAddress, set<Offset>>
    std::map<unsigned long long, std::set<int>> access_map; 
    
    // Map of inferred structures (Base Address -> New Type)
    std::map<unsigned long long, ghidra::TypeStruct*> inferred_structs;
    
    void collect_accesses(ghidra::Funcdata* fd);
    void infer_structures(ghidra::TypeFactory* factory);
    void apply_structures(ghidra::Funcdata* fd);
};

} // namespace types
} // namespace fission

#endif
