#include "fission/types/StructureAnalyzer.h"

// Ghidra headers
#include "funcdata.hh"
#include "varnode.hh"
#include "type.hh"
#include "op.hh"

#include <iostream>
#include <sstream>

namespace fission {
namespace types {

StructureAnalyzer::StructureAnalyzer() {}
StructureAnalyzer::~StructureAnalyzer() {}

bool StructureAnalyzer::analyze_function_structures(ghidra::Funcdata* fd) {
    if (!fd) return false;

    // Clear previous usage data
    access_map.clear();
    inferred_structs.clear();

    // Get architecture-specific pointer size (4 for 32-bit, 8 for 64-bit)
    ghidra::Architecture* arch = fd->getArch();
    int ptr_size = arch->getDefaultSize();  // FIX #2: Use architecture size
    
    // Get function entry address for unique naming (FIX #1)
    uint64_t func_entry = fd->getAddress().getOffset();

    // 1. Collect Access Patterns (PTRSUB, etc.)
    collect_accesses(fd);

    if (access_map.empty()) return false;

    // 2. Infer Structures (Create TypeStruct objects)
    ghidra::TypeFactory* factory = fd->getArch()->types;
    bool new_types_created = infer_structures(factory, func_entry, ptr_size);

    if (inferred_structs.empty()) return false;

    // 3. Apply to Function Inputs
    apply_structures(fd, ptr_size);

    // Only return true if NEW types were created (not reused)
    // This prevents unnecessary re-decompilation (FIX #3)
    return new_types_created;
}

void StructureAnalyzer::collect_accesses(ghidra::Funcdata* fd) {
    // Iterate over all alive PcodeOps
    auto iter = fd->beginOpAll();
    auto end = fd->endOpAll();

    for (; iter != end; ++iter) {
        ghidra::PcodeOp* op = iter->second;
        if (!op) continue;

        // Ensure it's alive
        if (op->isDead()) continue;

        ghidra::OpCode opcode = op->code();

        // We are looking for CPUI_PTRSUB: output = PTRSUB(base, offset)
        // This is the canonical way Ghidra represents "add offset to pointer to struct"
        if (opcode == ghidra::CPUI_PTRSUB) {
            ghidra::Varnode* base = op->getIn(0);
            ghidra::Varnode* offset_vn = op->getIn(1);

            // Check if base is an Input to the function (Parameter)
            if (base && base->isInput() && offset_vn && offset_vn->isConstant()) {
                unsigned long long offset = offset_vn->getOffset();
                // Store using the storage offset of the base varnode as key
                unsigned long long storage_addr = base->getOffset();
                
                // Track this access
                access_map[storage_addr].insert((int)offset);
            }
        }
    }
}

bool StructureAnalyzer::infer_structures(ghidra::TypeFactory* factory, 
                                          uint64_t func_entry, 
                                          int ptr_size) {
    if (!factory) return false;

    bool new_types_created = false;

    for (const auto& pair : access_map) {
        unsigned long long base_addr = pair.first;
        const std::set<int>& offsets = pair.second;

        if (offsets.empty()) continue;
        
        // Heuristic: ignore if only 0 is accessed (could be just int*)
        if (offsets.size() == 1 && *offsets.begin() == 0) continue;

        int max_offset = *offsets.rbegin();
        // FIX #2: Use architecture-specific pointer size
        int struct_size = max_offset + ptr_size;

        // FIX #1: Include function address in struct name for uniqueness
        // Format: f_<func_addr>_arg_<storage_offset>
        std::stringstream ss;
        ss << "f_" << std::hex << func_entry << "_arg_" << base_addr;
        std::string struct_name = ss.str();

        // Check if type already exists
        ghidra::Datatype* existing = factory->findByName(struct_name);
        if (existing != nullptr) {
            // Type already exists for THIS function - reuse it
            if (existing->getMetatype() == ghidra::TYPE_STRUCT) {
                inferred_structs[base_addr] = (ghidra::TypeStruct*)existing;
                // Note: This is a reuse, not a new creation
            }
            continue;  // Skip creation
        }

        // Create new struct with proper TypeFactory API
        ghidra::TypeStruct* new_struct = factory->getTypeStruct(struct_name);
        
        // Create Fields with architecture-appropriate sizes
        std::vector<ghidra::TypeField> fields;
        int field_id = 0;
        for (int off : offsets) {
            std::stringstream fss;
            fss << "field_" << std::hex << off;
            // FIX #2: Use architecture-specific integer type
            ghidra::Datatype* field_type = factory->getBase(ptr_size, ghidra::TYPE_INT);
            
            fields.push_back(ghidra::TypeField(field_id++, off, fss.str(), field_type));
        }

        // Set fields with architecture-specific alignment
        factory->setFields(fields, new_struct, struct_size, ptr_size, 0);
        
        // Store result
        inferred_structs[base_addr] = new_struct;
        new_types_created = true;  // FIX #3: Track that we created new types
        
        std::cerr << "[StructureAnalyzer] Created " << struct_name 
                  << " (" << (ptr_size * 8) << "-bit) with " 
                  << fields.size() << " fields" << std::endl;
    }
    
    return new_types_created;
}

void StructureAnalyzer::apply_structures(ghidra::Funcdata* fd, int ptr_size) {
    // Iterate input varnodes (Parameters)
    auto iter = fd->beginLoc(); // Location order
    auto end = fd->endLoc();

    ghidra::TypeFactory* factory = fd->getArch()->types;

    for (; iter != end; ++iter) {
        ghidra::Varnode* vn = *iter;
        if (vn->isInput()) {
            unsigned long long storage = vn->getOffset();
            if (inferred_structs.count(storage)) {
                ghidra::TypeStruct* st = inferred_structs[storage];
                
                // FIX #2: Use architecture-specific pointer size
                ghidra::TypePointer* ptr_type = factory->getTypePointer(ptr_size, st, ptr_size);

                // Update the Varnode's type
                vn->updateType(ptr_type, true, true); // Lock it!
                
                std::cerr << "[StructureAnalyzer] Applied " << ptr_type->getName() 
                          << " to Input @" << std::hex << storage << std::dec << std::endl;
            }
        }
    }
}

} // namespace types
} // namespace fission

