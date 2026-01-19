#include "fission/types/StructureAnalyzer.h"
#include "fission/types/TypeResolver.h"
#include "fission/core/ArchPolicy.h"

// Ghidra headers
#include "funcdata.hh"
#include "varnode.hh"
#include "type.hh"
#include "op.hh"
#include "address.hh"

#include <iostream>
#include "fission/utils/logger.h"
#include <sstream>
#include <algorithm>
#include <limits>

namespace fission {
namespace types {

using namespace fission::core;

static uint64_t make_base_key(const ghidra::Varnode* vn) {
    if (!vn) return 0;
    if (vn->isInput()) {
        return vn->getOffset();
    }
    uint64_t space = static_cast<uint64_t>(vn->getSpace()->getIndex()) & 0x7f;
    uint64_t offset = vn->getOffset() & 0x00FFFFFFFFFFFFFFULL;
    return 0x8000000000000000ULL | (space << 56) | offset;
}

static ghidra::Varnode* resolve_base_pointer(ghidra::Varnode* vn, int max_depth = 6) {
    if (!vn || max_depth <= 0) {
        return vn;
    }
    if (!vn->isWritten()) {
        return vn;
    }
    ghidra::PcodeOp* def = vn->getDef();
    if (!def) {
        return vn;
    }
    switch (def->code()) {
        case ghidra::CPUI_COPY:
        case ghidra::CPUI_CAST:
        case ghidra::CPUI_INT_ZEXT:
        case ghidra::CPUI_INT_SEXT:
            return resolve_base_pointer(def->getIn(0), max_depth - 1);
        case ghidra::CPUI_PTRSUB:
        case ghidra::CPUI_PTRADD:
        case ghidra::CPUI_INT_ADD:
            return resolve_base_pointer(def->getIn(0), max_depth - 1);
        case ghidra::CPUI_MULTIEQUAL:
        case ghidra::CPUI_INDIRECT: {
            ghidra::Varnode* candidate = nullptr;
            for (int slot = 0; slot < def->numInput(); ++slot) {
                ghidra::Varnode* in = def->getIn(slot);
                if (!in) continue;
                ghidra::Varnode* resolved = resolve_base_pointer(in, max_depth - 1);
                if (!resolved) continue;
                if (!candidate) {
                    candidate = resolved;
                } else if (candidate != resolved) {
                    return vn;
                }
            }
            return candidate ? candidate : vn;
        }
        default:
            return vn;
    }
}

static bool get_signed_offset(ghidra::Varnode* vn, int64_t& out) {
    if (!vn || !vn->isConstant()) {
        return false;
    }
    int size = vn->getSize();
    if (size <= 0) {
        return false;
    }
    int bits = (size * 8) - 1;
    ghidra::intb raw = static_cast<ghidra::intb>(vn->getOffset());
    out = static_cast<int64_t>(ghidra::sign_extend(raw, bits));
    return true;
}

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
    int ptr_size = fd->getArch()->types->getSizeOfPointer();

    for (; iter != end; ++iter) {
        ghidra::PcodeOp* op = iter->second;
        if (!op || op->isDead()) continue;

        ghidra::OpCode opcode = op->code();
        ghidra::Varnode* base = nullptr;
        int64_t offset = 0;
        bool found = false;

        if (opcode == ghidra::CPUI_LOAD) {
            // LOAD(space, ptr) -> direct dereference
            base = op->getIn(1);
            offset = 0;
            found = true;
        }
        else if (opcode == ghidra::CPUI_STORE) {
            // STORE(space, ptr, value) -> direct dereference
            base = op->getIn(1);
            offset = 0;
            found = true;
        }
        else if (opcode == ghidra::CPUI_PTRSUB) {
            // PTRSUB(base, offset)
            base = op->getIn(0);
            ghidra::Varnode* off_vn = op->getIn(1);
            if (get_signed_offset(off_vn, offset)) {
                found = true;
            }
        } 
        else if (opcode == ghidra::CPUI_INT_ADD) {
            // INT_ADD(base, const) or INT_ADD(const, base)
            ghidra::Varnode* vn0 = op->getIn(0);
            ghidra::Varnode* vn1 = op->getIn(1);
            if (get_signed_offset(vn0, offset)) {
                base = vn1;
                found = true;
            } else if (get_signed_offset(vn1, offset)) {
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

            int64_t idx = 0;
            int64_t elem_size = 0;
            if (get_signed_offset(idx_vn, idx) && get_signed_offset(size_vn, elem_size)) {
                offset = idx * elem_size;
                found = true;
            }
        }

        if (found && base) {
            if (offset < 0 || offset > std::numeric_limits<int>::max()) {
                continue;
            }
            base = resolve_base_pointer(base);
            if (!base) continue;
            if (base->isConstant()) continue;
            if (base->getSize() != ptr_size) continue;

            unsigned long long base_storage = make_base_key(base);
            
            // Determine size of access by checking descendants (LOAD/STORE)
            int access_size = 1; // Default
            bool is_float = false;
            bool is_pointer = false;

            if (opcode == ghidra::CPUI_LOAD) {
                ghidra::Varnode* load_out = op->getOut();
                if (load_out) {
                    access_size = std::max(access_size, (int)load_out->getSize());
                    if (TypeResolver::is_used_as_float(load_out)) {
                        is_float = true;
                    }
                    if (TypeResolver::is_pointer_access(load_out, ptr_size)) {
                        is_pointer = true;
                    }
                }
            } else if (opcode == ghidra::CPUI_STORE) {
                ghidra::Varnode* val = op->getIn(2);
                if (val) {
                    access_size = std::max(access_size, (int)val->getSize());
                    ghidra::PcodeOp* def_op = val->getDef();
                    if (def_op && TypeResolver::is_float_operation(def_op)) {
                        is_float = true;
                    }
                    if (TypeResolver::is_pointer_access(val, ptr_size)) {
                        is_pointer = true;
                    }
                }
            }
            
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
        if (base_addr & 0x8000000000000000ULL) {
            uint64_t space = (base_addr >> 56) & 0x7f;
            uint64_t offset = base_addr & 0x00FFFFFFFFFFFFFFULL;
            ss << "f_" << std::hex << func_entry << "_local_" << space << "_" << offset;
        } else {
            ss << "f_" << std::hex << func_entry << "_arg_" << base_addr;
        }
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
        
        fission::utils::log_stream() << "[StructureAnalyzer] Created " << struct_name 
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
        if (!vn || vn->isAnnotation() || vn->isConstant()) continue;
        if (vn->getSize() != ptr_size) continue;

        unsigned long long storage = make_base_key(vn);
        if (inferred_structs.count(storage)) {
            ghidra::TypeStruct* st = inferred_structs[storage];
            if (!st) continue;
            
            ghidra::TypePointer* ptr_type = ArchPolicy::getPointerType(factory, st, fd->getArch());
            if (!ptr_type) {
                fission::utils::log_stream() << "[StructureAnalyzer] ERROR: Failed to create pointer type for " 
                          << st->getName() << std::endl;
                continue;
            }

            // Aggressively update type AND lock it
            vn->updateType(ptr_type, true, true);
            
            fission::utils::log_stream() << "[StructureAnalyzer] Applied " << st->getName() << "* "
                      << "to Varnode @" << std::hex << storage << std::dec << std::endl;
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
