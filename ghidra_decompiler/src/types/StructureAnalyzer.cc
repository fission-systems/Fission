#include "fission/types/StructureAnalyzer.h"
#include "fission/types/TypeResolver.h"
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
            bool is_float = false;
            bool is_pointer = false;
            
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
                        ghidra::Varnode* load_out = use_op->getOut();
                        if (load_out) {
                            access_size = std::max(access_size, (int)load_out->getSize());
                            // Check if loaded value is used as float
                            if (TypeResolver::is_used_as_float(load_out)) {
                                is_float = true;
                            }
                            // Check if loaded value is used as pointer
                            int ptr_size = fd->getArch()->types->getSizeOfPointer();
                            if (TypeResolver::is_pointer_access(load_out, ptr_size)) {
                                is_pointer = true;
                            }
                        }
                    } else if (use_code == ghidra::CPUI_STORE) {
                        // STORE(space, ptr, value) -> size of value (input 2)
                        ghidra::Varnode* val = use_op->getIn(2);
                        if (val) {
                            access_size = std::max(access_size, (int)val->getSize());
                            // Check source of stored value for float ops
                            ghidra::PcodeOp* def_op = val->getDef();
                            if (def_op && TypeResolver::is_float_operation(def_op)) {
                                is_float = true;
                            }
                        }
                    }
                }
            }

            // Update map: track field info for this offset
            FieldInfo& info = access_map[base_storage][(int)offset];
            if (access_size > info.size) {
                info.size = access_size;
            }
            if (is_float) info.is_float = true;
            if (is_pointer) info.is_pointer = true;
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
        std::map<int, FieldInfo>& offsets = pair.second; // Offset -> FieldInfo

        if (offsets.empty()) continue;
        if (offsets.size() == 1 && offsets.begin()->first == 0) continue; // Heuristic: Skip if only accessing offset 0

        // Calculate total struct size
        int max_offset = offsets.rbegin()->first;
        int last_field_size = offsets.rbegin()->second.size;
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
        
        // Fill fields with precise type detection
        for (auto const& [off, info] : offsets) {
            std::stringstream fss;
            
            // Generate descriptive field name based on detected type
            if (info.is_float) {
                fss << ((info.size == 4) ? "flt_" : "dbl_") << std::hex << off;
            } else if (info.is_pointer) {
                fss << "ptr_" << std::hex << off;
            } else {
                fss << "field_" << std::hex << off;
            }
            
            // Use TypeResolver for precise type selection
            ghidra::Datatype* field_type = TypeResolver::get_field_type(
                factory,
                info.size,
                info.is_float,
                info.is_pointer,
                ptr_size
            );
            
            if (!field_type) field_type = factory->getBase(1, ghidra::TYPE_UNKNOWN);

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
                if (!st) continue;
                
                ghidra::TypePointer* ptr_type = ArchPolicy::getPointerType(factory, st, fd->getArch());
                if (!ptr_type) {
                    std::cerr << "[StructureAnalyzer] ERROR: Failed to create pointer type for " 
                              << st->getName() << std::endl;
                    continue;
                }

                // Aggressively update type AND lock it
                vn->updateType(ptr_type, true, true);
                
                std::cerr << "[StructureAnalyzer] Applied " << st->getName() << "* "
                          << "to Input @" << std::hex << storage << std::dec << std::endl;
            }
        }
    }
}


    std::string StructureAnalyzer::generate_struct_definitions() const {
        std::stringstream ss;
        if (inferred_structs.empty()) return "";

        ss << "// Inferred Structure Definitions\n";
        
        for (auto const& [addr, type] : inferred_structs) {
            if (!type) continue;
            
            std::string name = type->getName();
            ss << "typedef struct " << name << " {\n";
            
            auto iter = type->beginField();
            auto end = type->endField();
            
            // Sort fields by offset if not already
            // TypeStruct stores them in a vector, usually sorted by offset
            
            for (; iter != end; ++iter) {
                // TypeField members are public: offset, name, type
                std::string field_type = "undefined";
                if (iter->type) {
                    field_type = iter->type->getName();
                }
                
                // Indent
                ss << "    " << field_type << " " << iter->name << "; // Offset " << std::hex << iter->offset << std::dec << "\n";
            }
            
            ss << "} " << name << ";\n\n";
        }
        
        return ss.str();
    }

    std::map<std::string, std::string> StructureAnalyzer::get_type_replacements() const {
        // Future implementation: Return map for precise type replacement
        // e.g. "DWORD *param_1" -> "f_... *param_1"
        return std::map<std::string, std::string>();
    }

} // namespace types
} // namespace fission

