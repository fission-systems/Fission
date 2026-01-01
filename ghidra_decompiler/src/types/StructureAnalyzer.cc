#include "fission/types/StructureAnalyzer.h"
#include "fission/core/ArchPolicy.h"

// Ghidra headers
#include "funcdata.hh"
#include "varnode.hh"
#include "type.hh"
#include "op.hh"

#include <iostream>
#include <sstream>
#include <algorithm>

namespace fission {
namespace types {

using namespace fission::core;

StructureAnalyzer::StructureAnalyzer() {}
StructureAnalyzer::~StructureAnalyzer() {}

bool StructureAnalyzer::analyze_function_structures(ghidra::Funcdata* fd) {
    if (!fd) return false;

    access_map.clear();
    inferred_structs.clear();

    ghidra::Architecture* arch = fd->getArch();
    int ptr_size = ArchPolicy::getPointerSize(arch);
    uint64_t func_entry = fd->getAddress().getOffset();

    // 1. Collect Access Patterns (PTRSUB, PTRADD, INT_ADD)
    collect_accesses(fd);

    if (access_map.empty()) return false;

    // 2. Infer Structures
    ghidra::TypeFactory* factory = fd->getArch()->types;
    bool new_types_created = infer_structures(factory, func_entry, ptr_size);

    if (inferred_structs.empty()) return false;

    // 3. Apply to Function Inputs
    apply_structures(fd, ptr_size);

    return new_types_created;
}

void StructureAnalyzer::collect_accesses(ghidra::Funcdata* fd) {
    auto iter = fd->beginOpAll();
    auto end = fd->endOpAll();

    for (; iter != end; ++iter) {
        ghidra::PcodeOp* op = iter->second;
        if (!op || op->isDead()) continue;

        ghidra::OpCode opcode = op->code();
        ghidra::Varnode* base = nullptr;
        uint64_t offset = 0;
        bool found = false;

        if (opcode == ghidra::CPUI_PTRSUB) {
            // PTRSUB(base, offset)
            base = op->getIn(0);
            ghidra::Varnode* off_vn = op->getIn(1);
            if (off_vn && off_vn->isConstant()) {
                offset = off_vn->getOffset();
                found = true;
            }
        } 
        else if (opcode == ghidra::CPUI_INT_ADD) {
            // INT_ADD(base, const) or INT_ADD(const, base)
            ghidra::Varnode* vn0 = op->getIn(0);
            ghidra::Varnode* vn1 = op->getIn(1);
            if (vn0->isConstant()) {
                offset = vn0->getOffset();
                base = vn1;
                found = true;
            } else if (vn1->isConstant()) {
                offset = vn1->getOffset();
                base = vn0;
                found = true;
            }
        }
        else if (opcode == ghidra::CPUI_PTRADD) {
            // PTRADD(base, index, elem_size)
            // Handle only simple case: index is constant
            base = op->getIn(0);
            ghidra::Varnode* idx_vn = op->getIn(1);
            ghidra::Varnode* size_vn = op->getIn(2);
            
            if (idx_vn && idx_vn->isConstant() && size_vn && size_vn->isConstant()) {
                uint64_t idx = idx_vn->getOffset();
                uint64_t elem_size = size_vn->getOffset();
                offset = idx * elem_size;
                found = true;
            }
        }

        if (found && base && base->isInput()) {
            unsigned long long base_storage = base->getOffset();
            
            // Determine size of access by checking descendants (LOAD/STORE)
            int access_size = 1; // Default
            ghidra::Varnode* out_vn = op->getOut();
            if (out_vn) {
                auto desc_iter = out_vn->beginDescend();
                auto desc_end = out_vn->endDescend();
                for(; desc_iter != desc_end; ++desc_iter) {
                    ghidra::PcodeOp* use_op = *desc_iter;
                    if (!use_op) continue;
                    ghidra::OpCode use_code = use_op->code();
                    
                    if (use_code == ghidra::CPUI_LOAD) {
                        // output = LOAD(space, ptr) -> size of output
                        if (use_op->getOut()) {
                            access_size = std::max(access_size, (int)use_op->getOut()->getSize());
                        }
                    } else if (use_code == ghidra::CPUI_STORE) {
                        // STORE(space, ptr, value) -> size of value (input 2)
                        ghidra::Varnode* val = use_op->getIn(2);
                        if (val) {
                            access_size = std::max(access_size, (int)val->getSize());
                        }
                    }
                }
            }

            // Update map: track max size for this offset
            int& stored_size = access_map[base_storage][(int)offset];
            if (access_size > stored_size) {
                stored_size = access_size;
            }
        }
    }
}

bool StructureAnalyzer::infer_structures(ghidra::TypeFactory* factory, 
                                          uint64_t func_entry, 
                                          int ptr_size) {
    if (!factory) return false;

    bool new_types_created = false;

    // Iterate over inferred accesses
    for (auto& pair : access_map) {
        unsigned long long base_addr = pair.first;
        std::map<int, int>& offsets = pair.second; // Offset -> Size

        if (offsets.empty()) continue;
        if (offsets.size() == 1 && offsets.begin()->first == 0) continue; // Heuristic: Skip if only accessing offset 0

        // Calculate total struct size
        int max_offset = offsets.rbegin()->first;
        int last_field_size = offsets.rbegin()->second;
        int struct_size = max_offset + last_field_size;
        
        // Align struct size to pointer size
        if (struct_size % ptr_size != 0) {
            struct_size += (ptr_size - (struct_size % ptr_size));
        }

        std::stringstream ss;
        ss << "f_" << std::hex << func_entry << "_arg_" << base_addr;
        std::string struct_name = ss.str();

        // Reuse if exists
        ghidra::Datatype* existing = factory->findByName(struct_name);
        if (existing != nullptr) {
            if (existing->getMetatype() == ghidra::TYPE_STRUCT) {
                inferred_structs[base_addr] = (ghidra::TypeStruct*)existing;
            }
            continue;
        }

        // Create new struct
        ghidra::TypeStruct* new_struct = factory->getTypeStruct(struct_name);
        std::vector<ghidra::TypeField> fields;
        int field_id = 0;
        
        // Fill fields
        for (auto const& [off, size] : offsets) {
            std::stringstream fss;
            fss << "field_" << std::hex << off;
            
            // Try to find a suitable primitive type for the detected size
            ghidra::Datatype* field_type = nullptr;
            
            // Basic primitives prefer signed int/long
            if (size == 1) field_type = factory->getBase(1, ghidra::TYPE_INT); // char/byte
            else if (size == 2) field_type = factory->getBase(2, ghidra::TYPE_INT); // short
            else if (size == 4) field_type = factory->getBase(4, ghidra::TYPE_INT); // int/float
            else if (size == 8) field_type = factory->getBase(8, ghidra::TYPE_INT); // long/double
            else {
                // For other sizes, fallback to unknown
                field_type = factory->getBase(size, ghidra::TYPE_UNKNOWN);
            }
            
            if(!field_type) field_type = factory->getBase(1, ghidra::TYPE_UNKNOWN); // Ultimate fallback

            fields.push_back(ghidra::TypeField(field_id++, off, fss.str(), field_type));
        }

        // Apply/Finalize struct
        // Passing 0 for flags handles padding automatically
        factory->setFields(fields, new_struct, struct_size, ptr_size, 0);
        
        inferred_structs[base_addr] = new_struct;
        new_types_created = true;
        
        std::cerr << "[StructureAnalyzer] Created " << struct_name 
                  << " (" << (struct_size) << " bytes) with " 
                  << fields.size() << " detected fields" << std::endl;
    }
    
    return new_types_created;
}

void StructureAnalyzer::apply_structures(ghidra::Funcdata* fd, int ptr_size) {
    ghidra::TypeFactory* factory = fd->getArch()->types;

    // Use beginLoc for parameter order iteration
    auto iter = fd->beginLoc();
    auto end = fd->endLoc();

    for (; iter != end; ++iter) {
        ghidra::Varnode* vn = *iter;
        if (vn->isInput()) {
            unsigned long long storage = vn->getOffset();
            if (inferred_structs.count(storage)) {
                ghidra::TypeStruct* st = inferred_structs[storage];
                ghidra::TypePointer* ptr_type = ArchPolicy::getPointerType(factory, st, fd->getArch());

                // Aggressively update type AND lock it
                vn->updateType(ptr_type, true, true);
                
                std::cerr << "[StructureAnalyzer] Applied " << ptr_type->getName() 
                          << " to Input @" << std::hex << storage << std::dec << std::endl;
            }
        }
    }
}

} // namespace types
} // namespace fission
