#include "fission/analysis/GlobalDataAnalyzer.h"
#include "fission/types/TypeResolver.h"

// Ghidra headers
#include "funcdata.hh"
#include "varnode.hh"
#include "type.hh"
#include "op.hh"

#include <iostream>
#include "fission/utils/logger.h"
#include <sstream>
#include <algorithm>

namespace fission {
namespace analysis {

using namespace fission::types;

GlobalDataAnalyzer::GlobalDataAnalyzer() {}
GlobalDataAnalyzer::~GlobalDataAnalyzer() {}

void GlobalDataAnalyzer::set_data_section(uint64_t start, uint64_t end) {
    data_section_start = start;
    data_section_end = end;
    fission::utils::log_stream() << "[GlobalDataAnalyzer] Data section: 0x" << std::hex 
              << start << " - 0x" << end << std::dec << std::endl;
}

bool GlobalDataAnalyzer::is_in_data_section(uint64_t addr) const {
    // If no section defined, accept any non-zero address
    if (data_section_start == 0 && data_section_end == 0) {
        return addr != 0 && addr < 0x7FFFFFFFFFFFFFFF;
    }
    return addr >= data_section_start && addr < data_section_end;
}

void GlobalDataAnalyzer::clear() {
    accesses.clear();
    inferred_globals.clear();
    scalar_floats_.clear();
}

void GlobalDataAnalyzer::analyze_function(ghidra::Funcdata* fd) {
    if (!fd) return;
    
    uint64_t func_addr = fd->getAddress().getOffset();
    int ptr_size = fd->getArch()->types->getSizeOfPointer();
    
    auto iter = fd->beginOpAll();
    auto end_iter = fd->endOpAll();
    
    for (; iter != end_iter; ++iter) {
        ghidra::PcodeOp* op = iter->second;
        if (!op || op->isDead()) continue;
        
        ghidra::OpCode code = op->code();
        
        // Look for LOAD/STORE operations on constant addresses
        if (code == ghidra::CPUI_LOAD) {
            // LOAD(space, ptr) -> output
            ghidra::Varnode* ptr_vn = op->getIn(1);
            ghidra::Varnode* out_vn = op->getOut();
            
            if (ptr_vn && ptr_vn->isConstant()) {
                uint64_t addr = ptr_vn->getOffset();
                
                if (is_in_data_section(addr)) {
                    GlobalAccess access;
                    access.address = addr;
                    access.offset = 0;
                    access.size = out_vn ? out_vn->getSize() : ptr_size;
                    access.is_read = true;
                    access.is_float = out_vn && TypeResolver::is_used_as_float(out_vn);
                    access.is_pointer = out_vn && TypeResolver::is_pointer_access(out_vn, ptr_size);
                    access.from_function = func_addr;
                    accesses.push_back(access);
                }
            }
            // Also check PTRSUB/PTRADD to constant base
            if (ptr_vn && ptr_vn->getDef()) {
                ghidra::PcodeOp* def = ptr_vn->getDef();
                ghidra::OpCode def_code = def->code();
                
                if (def_code == ghidra::CPUI_PTRSUB || def_code == ghidra::CPUI_INT_ADD) {
                    ghidra::Varnode* base = def->getIn(0);
                    ghidra::Varnode* off = def->getIn(1);
                    
                    if (base && base->isConstant() && off && off->isConstant()) {
                        uint64_t base_addr = base->getOffset();
                        uint64_t offset = off->getOffset();
                        
                        if (is_in_data_section(base_addr)) {
                            GlobalAccess access;
                            access.address = base_addr;
                            access.offset = (int)offset;
                            access.size = out_vn ? out_vn->getSize() : ptr_size;
                            access.is_read = true;
                            access.is_float = out_vn && TypeResolver::is_used_as_float(out_vn);
                            access.is_pointer = out_vn && TypeResolver::is_pointer_access(out_vn, ptr_size);
                            access.from_function = func_addr;
                            accesses.push_back(access);
                        }
                    }
                }
            }
        }
        else if (code == ghidra::CPUI_STORE) {
            // STORE(space, ptr, value)
            ghidra::Varnode* ptr_vn = op->getIn(1);
            ghidra::Varnode* val_vn = op->getIn(2);
            
            if (ptr_vn && ptr_vn->isConstant()) {
                uint64_t addr = ptr_vn->getOffset();
                
                if (is_in_data_section(addr)) {
                    GlobalAccess access;
                    access.address = addr;
                    access.offset = 0;
                    access.size = val_vn ? val_vn->getSize() : ptr_size;
                    access.is_read = false;
                    access.is_float = false;
                    access.is_pointer = false;
                    access.from_function = func_addr;
                    accesses.push_back(access);
                }
            }
        }
    }
}

std::map<uint64_t, std::vector<GlobalAccess>> GlobalDataAnalyzer::cluster_by_base() {
    std::map<uint64_t, std::vector<GlobalAccess>> clusters;
    
    for (const auto& access : accesses) {
        // For direct accesses (offset = 0), use address as key
        // For structure accesses, group by base address
        uint64_t base = access.address;
        clusters[base].push_back(access);
    }
    
    return clusters;
}

void GlobalDataAnalyzer::infer_structures() {
    scalar_floats_.clear();
    auto clusters = cluster_by_base();
    
    for (auto& [base, cluster_accesses] : clusters) {
        // Skip single accesses (likely not structures)
        bool has_offsets = false;
        for (const auto& a : cluster_accesses) {
            if (a.offset != 0) {
                has_offsets = true;
                break;
            }
        }
        
        if (!has_offsets && cluster_accesses.size() == 1) {
            // GAP-1 FIX: Instead of discarding scalar float/double accesses,
            // record them so the pipeline can register typed DAT_ symbols.
            // This mirrors Ghidra's ConstantPropagationAnalyzer which creates
            // float/double data symbols that then propagate via LOAD type
            // inference, replacing raw hex constants in decompiler output.
            const GlobalAccess& a = cluster_accesses[0];
            if (a.is_float && (a.size == 4 || a.size == 8)) {
                ScalarFloatEntry entry;
                entry.address = a.address;
                entry.size    = a.size;
                scalar_floats_.push_back(entry);
                fission::utils::log_stream()
                    << "[GlobalDataAnalyzer] Scalar "
                    << (a.size == 4 ? "float" : "double")
                    << " at 0x" << std::hex << a.address << std::dec << "\n";
            }
            continue;
        }
        
        GlobalStructure gs;
        gs.address = base;
        
        std::stringstream ss;
        ss << "g_" << std::hex << base;
        gs.name = ss.str();
        
        // Merge accesses into fields
        for (const auto& access : cluster_accesses) {
            int off = access.offset;
            int size = access.size;
            
            // Track max size at each offset
            if (gs.fields.find(off) == gs.fields.end() || gs.fields[off] < size) {
                gs.fields[off] = size;
            }
            if (access.is_float) gs.float_fields[off] = true;
            if (access.is_pointer) gs.pointer_fields[off] = true;
        }
        
        // Calculate total size
        if (!gs.fields.empty()) {
            auto last = gs.fields.rbegin();
            gs.total_size = last->first + last->second;
        }
        
        inferred_globals.push_back(gs);
    }
    
    fission::utils::log_stream() << "[GlobalDataAnalyzer] Inferred " << inferred_globals.size() 
              << " global structures from " << accesses.size() << " accesses" << std::endl;
}

int GlobalDataAnalyzer::create_types(ghidra::TypeFactory* factory, int ptr_size) {
    if (!factory) return 0;
    
    int created = 0;
    
    for (const auto& gs : inferred_globals) {
        if (gs.fields.empty()) continue;
        
        // Check if type already exists
        ghidra::Datatype* existing = factory->findByName(gs.name);
        if (existing) continue;
        
        // Create struct type
        ghidra::TypeStruct* new_struct = factory->getTypeStruct(gs.name);
        std::vector<ghidra::TypeField> fields;
        int field_id = 0;
        
        for (const auto& [off, size] : gs.fields) {
            std::stringstream fss;
            
            bool is_float = gs.float_fields.count(off) && gs.float_fields.at(off);
            bool is_pointer = gs.pointer_fields.count(off) && gs.pointer_fields.at(off);
            
            if (is_float) {
                fss << ((size == 4) ? "flt_" : "dbl_") << std::hex << off;
            } else if (is_pointer) {
                fss << "ptr_" << std::hex << off;
            } else {
                fss << "field_" << std::hex << off;
            }
            
            ghidra::Datatype* field_type = TypeResolver::get_field_type(
                factory, size, is_float, is_pointer, ptr_size
            );
            
            if (!field_type) {
                field_type = factory->getBase(size, ghidra::TYPE_UNKNOWN);
            }
            
            fields.push_back(ghidra::TypeField(field_id++, off, fss.str(), field_type));
        }
        
        // Align size
        int aligned_size = gs.total_size;
        if (aligned_size % ptr_size != 0) {
            aligned_size += ptr_size - (aligned_size % ptr_size);
        }
        
        factory->setFields(fields, new_struct, aligned_size, ptr_size, 0);
        created++;
        
        fission::utils::log_stream() << "[GlobalDataAnalyzer] Created " << gs.name 
                  << " (" << fields.size() << " fields, " << aligned_size << " bytes)" << std::endl;
    }
    
    return created;
}

} // namespace analysis
} // namespace fission
