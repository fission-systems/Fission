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

    // 1. Collect Access Patterns (PTRSUB, etc.)
    collect_accesses(fd);

    if (access_map.empty()) return false;

    // 2. Infer Structures (Create TypeStruct objects)
    ghidra::TypeFactory* factory = fd->getArch()->types;
    infer_structures(factory);

    if (inferred_structs.empty()) return false;

    // 3. Apply to Function Inputs
    apply_structures(fd);

    return true;
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

void StructureAnalyzer::infer_structures(ghidra::TypeFactory* factory) {
    if (!factory) return;

    for (const auto& pair : access_map) {
        unsigned long long base_addr = pair.first;
        const std::set<int>& offsets = pair.second;

        if (offsets.empty()) continue;
        
        // Heuristic: ignore if only 0 is accessed (could be just int*)
        if (offsets.size() == 1 && *offsets.begin() == 0) continue;

        int max_offset = *offsets.rbegin();
        int struct_size = max_offset + 8; // Assuming 64-bit pointer size

        // Generate struct name
        std::stringstream ss;
        ss << "auto_struct_" << std::hex << base_addr;
        std::string struct_name = ss.str();

        // === USE PROPER TypeFactory API ===
        // getTypeStruct creates an empty struct with a valid ID
        ghidra::TypeStruct* new_struct = factory->getTypeStruct(struct_name);
        
        // Create Fields
        std::vector<ghidra::TypeField> fields;
        int field_id = 0;
        for (int off : offsets) {
            std::stringstream fss;
            fss << "field_" << std::hex << off;
            // Default to 8-byte int type
            ghidra::Datatype* field_type = factory->getBase(8, ghidra::TYPE_INT);
            
            fields.push_back(ghidra::TypeField(field_id++, off, fss.str(), field_type));
        }

        // === USE TypeFactory::setFields to properly configure the struct ===
        // setFields(fields, struct_ptr, size, alignment, flags)
        factory->setFields(fields, new_struct, struct_size, 8, 0);
        
        // Store result
        inferred_structs[base_addr] = new_struct;
        
        std::cerr << "[StructureAnalyzer] Created " << struct_name 
                  << " with " << fields.size() << " fields" << std::endl;
    }
}

void StructureAnalyzer::apply_structures(ghidra::Funcdata* fd) {
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
                
                // Construct a Pointer to this struct
                // Using TypeFactory::getTypePointer for proper ID handling
                ghidra::TypePointer* ptr_type = factory->getTypePointer(8, st, 8);

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
