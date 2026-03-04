#include "fission/types/StructureAnalyzer.h"
#include "fission/types/TypeResolver.h"
#include "fission/core/ArchPolicy.h"

// Ghidra headers
#include "funcdata.hh"
#include "varnode.hh"
#include "type.hh"
#include "op.hh"
#include "address.hh"
#include "unionresolve.hh"

#include <iostream>
#include "fission/utils/logger.h"
#include <sstream>
#include <algorithm>
#include <limits>
#include <chrono>

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
    size_variants.clear();
    inferred_unions.clear();

    ghidra::Architecture* arch = fd->getArch();
    int ptr_size = ArchPolicy::getPointerSize(arch);
    uint64_t func_entry = fd->getAddress().getOffset();

    // 1. Collect Access Patterns (PTRSUB, PTRADD, INT_ADD)
    collect_accesses(fd);

    if (access_map.empty()) return false;

    // 2. Infer Structures
    ghidra::TypeFactory* factory = fd->getArch()->types;
    bool new_types_created = infer_structures(factory, func_entry, ptr_size);

    // 2b. Infer Unions (overlapping field accesses)
    bool unions_created = infer_unions(factory, func_entry, ptr_size);
    new_types_created = new_types_created || unions_created;

    if (inferred_structs.empty()) return false;

    // 3. Apply to Function Inputs
    apply_structures(fd, ptr_size);

    return new_types_created;
}

void StructureAnalyzer::collect_accesses(ghidra::Funcdata* fd) {
    auto iter = fd->beginOpAll();
    auto end = fd->endOpAll();
    int ptr_size = fd->getArch()->types->getSizeOfPointer();

    // Wall-clock safety net: stop collecting after 50 ms to prevent hangs on
    // huge or heavily CFG-optimised functions.  Check the deadline every 256
    // operations so the overhead is negligible.
    const auto deadline =
        std::chrono::steady_clock::now() + std::chrono::milliseconds(50);
    int op_count = 0;

    for (; iter != end; ++iter) {
        if ((++op_count & 0xFF) == 0) {
            if (std::chrono::steady_clock::now() > deadline) {
                fission::utils::log_stream()
                    << "[StructureAnalyzer] collect_accesses: 50 ms deadline hit after "
                    << op_count << " pcode ops — using partial results" << std::endl;
                break;
            }
        }

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

            // Track all distinct sizes per (base, offset) for union detection
            size_variants[base_storage][(int)offset].insert(access_size);
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

                    // Handle pointer types — getName() may return empty or
                    // an internal Ghidra name; produce readable C type instead
                    if (field_type.empty()) {
                        // Detect pointer by field name prefix or type metatype
                        if (iter->name.substr(0, 4) == "ptr_") {
                            field_type = "void *";
                        } else {
                            field_type = "undefined";
                        }
                    } else if (iter->type->getMetatype() == ghidra::TYPE_PTR) {
                        // Ghidra pointer type — ensure proper C-formatted name
                        // e.g., "void *" instead of internal representation
                        const ghidra::TypePointer* ptr_type =
                            dynamic_cast<const ghidra::TypePointer*>(iter->type);
                        if (ptr_type) {
                            ghidra::Datatype* pointed = ptr_type->getPtrTo();
                            if (pointed) {
                                std::string pointed_name = pointed->getName();
                                if (pointed_name.empty() || pointed_name == "undefined" ||
                                    pointed->getMetatype() == ghidra::TYPE_VOID) {
                                    field_type = "void *";
                                } else {
                                    field_type = pointed_name + " *";
                                }
                            } else {
                                field_type = "void *";
                            }
                        }
                    }
                }
                
                // Indent — handle "TYPE *" vs "TYPE" spacing
                if (field_type.back() == '*') {
                    // Already has pointer suffix like "void *", no extra space needed
                    ss << "    " << field_type << iter->name << "; // Offset " << iter->offset << "\n";
                } else {
                    ss << "    " << field_type << " " << iter->name << "; // Offset " << iter->offset << "\n";
                }
            }
            
            ss << "} " << name << ";\n\n";
        }
        
        return ss.str();
    }

    std::map<std::string, std::string> StructureAnalyzer::get_type_replacements() const {
        std::map<std::string, std::string> replacements;

        for (auto const& [base_key, st] : inferred_structs) {
            if (!st) continue;

            std::string struct_name = st->getName();

            // Build field offset → field name map for this struct
            auto iter = st->beginField();
            auto end  = st->endField();

            for (; iter != end; ++iter) {
                int off = iter->offset;
                std::string fname = iter->name;

                // Text-level patterns emitted by Ghidra's PrintC for pointer arithmetic:
                //   *(type *)(param + 0xNN)   →   param->field_name
                //   param[0xNN]               →   param->field_name
                //   *(param + NN)             →   param->field_name

                // Build hex offset key (decimal and hex variants):
                std::stringstream hex_ss;
                hex_ss << "0x" << std::hex << off;
                std::string hex_off = hex_ss.str();
                std::string dec_off = std::to_string(off);

                // Map: offset value → struct_name.field_name
                // PostProcessPipeline will use this to annotate offsets
                std::string field_key = struct_name + "." + fname;
                replacements["@off:" + hex_off] = field_key;
                replacements["@off:" + dec_off] = field_key;
            }

            // Map: struct typedef replacement (DWORD * → struct_name *)
            // Use base_key to identify which parameter gets this struct type
            std::stringstream key_ss;
            if (base_key & 0x8000000000000000ULL) {
                // Stack local — type replacement for local variables
                uint64_t offset = base_key & 0x00FFFFFFFFFFFFFFULL;
                key_ss << "@local:" << std::hex << offset;
            } else {
                // Input parameter — type replacement for function params
                key_ss << "@param:" << std::hex << base_key;
            }
            replacements[key_ss.str()] = struct_name + " *";
        }

        return replacements;
    }

bool StructureAnalyzer::infer_unions(ghidra::TypeFactory* factory,
                                      uint64_t func_entry,
                                      int ptr_size) {
    if (!factory) return false;
    bool created = false;

    // For each base varnode, check if any two fields have overlapping byte ranges.
    // Overlapping fields that all start at the same base offset are strong union candidates.
    for (auto& [base_key, offset_size_map] : size_variants) {
        // Skip bases that are already fully described by a struct (no overlap needed)
        // Collect (offset, size) pairs sorted by offset
        std::vector<std::pair<int, int>> ranges; // (offset, size)
        for (auto& [off, sizes] : offset_size_map) {
            for (int sz : sizes) {
                if (sz > 0) ranges.push_back({off, sz});
            }
        }
        if (ranges.size() < 2) continue;

        // Detect any overlap: field A covers [oa, oa+sa), field B covers [ob, ob+sb)
        // => overlap if oa < ob+sb && ob < oa+sa
        bool has_overlap = false;
        for (size_t i = 0; i < ranges.size() && !has_overlap; ++i) {
            for (size_t j = i + 1; j < ranges.size() && !has_overlap; ++j) {
                int oa = ranges[i].first,  sa = ranges[i].second;
                int ob = ranges[j].first,  sb = ranges[j].second;
                if (oa < ob + sb && ob < oa + sa) {
                    has_overlap = true;
                }
            }
        }
        if (!has_overlap) continue;

        // Build a union name
        std::stringstream ss;
        if (base_key & 0x8000000000000000ULL) {
            uint64_t space  = (base_key >> 56) & 0x7f;
            uint64_t offset = base_key & 0x00FFFFFFFFFFFFFFULL;
            ss << "u_" << std::hex << func_entry << "_local_" << space << "_" << offset;
        } else {
            ss << "u_" << std::hex << func_entry << "_arg_" << base_key;
        }
        std::string union_name = ss.str();

        // Reuse if already exists
        ghidra::Datatype* existing = factory->findByName(union_name);
        if (existing != nullptr) {
            if (existing->getMetatype() == ghidra::TYPE_UNION) {
                inferred_unions[base_key] = dynamic_cast<ghidra::TypeUnion*>(existing);
            }
            continue;
        }

        // Compute union size = max end byte across all ranges
        int union_size = 0;
        for (auto& [off, sz] : ranges) {
            union_size = std::max(union_size, off + sz);
        }
        // Align to pointer size
        if (union_size % ptr_size != 0) {
            union_size += ptr_size - (union_size % ptr_size);
        }

        // Create union fields (one per unique (offset, size) pair to avoid duplicates)
        std::set<std::pair<int,int>> seen_range;
        std::vector<ghidra::TypeField> fields;
        int fid = 0;
        for (auto& [off, sz] : ranges) {
            if (!seen_range.insert({off, sz}).second) continue;
            std::stringstream fss;
            fss << "var_" << std::hex << off << "_" << sz;
            ghidra::Datatype* ft = factory->getBase(sz, ghidra::TYPE_UNKNOWN);
            if (!ft) ft = factory->getBase(1, ghidra::TYPE_UNKNOWN);
            // For union fields all start at offset 0 (union semantics)
            fields.push_back(ghidra::TypeField(fid++, 0, fss.str(), ft));
        }

        ghidra::TypeUnion* new_union = factory->getTypeUnion(union_name);
        factory->setFields(fields, new_union, union_size, ptr_size, 0);
        inferred_unions[base_key] = new_union;
        created = true;

        fission::utils::log_stream() << "[StructureAnalyzer] Created union " << union_name
                  << " (" << union_size << " bytes, " << fields.size() << " variants)\n";
    }
    return created;
}

std::string StructureAnalyzer::generate_union_definitions() const {
    std::stringstream ss;
    if (inferred_unions.empty()) return "";
    ss << "// Inferred Union Definitions\n";
    for (auto const& [addr, type] : inferred_unions) {
        if (!type) continue;
        ss << "typedef union " << type->getName() << " {\n";
        int nf = type->numDepend();
        for (int i = 0; i < nf; ++i) {
            const ghidra::TypeField* f = type->getField(i);
            if (!f) continue;
            std::string ft = f->type ? f->type->getName() : "undefined";
            if (ft.empty()) ft = "undefined";
            ss << "    " << ft << " " << f->name << ";\n";
        }
        ss << "} " << type->getName() << ";\n\n";
    }
    return ss.str();
}

} // namespace types
} // namespace fission