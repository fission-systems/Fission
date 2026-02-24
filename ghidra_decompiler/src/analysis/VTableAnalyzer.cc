#include "fission/analysis/VTableAnalyzer.h"

#include <iostream>
#include "fission/utils/logger.h"
#include <sstream>
#include <cstring>

// Ghidra type-system headers (needed by register_vtable_types)
#include "architecture.hh"
#include "type.hh"
#include "database.hh"

namespace fission {
namespace analysis {

VTableAnalyzer::VTableAnalyzer() {}
VTableAnalyzer::~VTableAnalyzer() {}

void VTableAnalyzer::clear() {
    vtables.clear();
    vtable_index.clear();
}

bool VTableAnalyzer::looks_like_function_ptr(uint64_t addr, uint64_t image_base, size_t binary_size) const {
    // Function pointers should:
    // 1. Be within the binary's code section (image_base to image_base + size)
    // 2. Not be zero or obviously invalid
    // 3. Be at least 4-byte aligned (all function addresses are)
    if (addr == 0) return false;
    if (addr < image_base) return false;
    if (addr > image_base + binary_size) return false;
    if (addr % 4 != 0) return false; // B-3: functions are always aligned
    return true;
}

bool VTableAnalyzer::scan_vtable_at(const uint8_t* data, size_t offset, size_t max_size,
                                     uint64_t image_base, size_t binary_size, int ptr_size, VTable& out) {
    out.entries.clear();
    out.address = image_base + offset;

    // B-3: Check the meta-pointer slot before the vtable.
    // MSVC: vtable[-ptr_size] = CompleteObjectLocator pointer (a data address, NOT a code ptr)
    // Itanium: vtable[-2*ptr_size] = 0 (offset-to-top), vtable[-ptr_size] = type_info ptr
    // If vtable[-ptr_size] itself looks like a function pointer, this is probably NOT a vtable.
    if (offset >= (size_t)ptr_size) {
        uint64_t meta_ptr = 0;
        if (ptr_size == 8) meta_ptr = *(const uint64_t*)(data + offset - 8);
        else               meta_ptr = *(const uint32_t*)(data + offset - 4);
        // Allow: 0 (Itanium primary, or null), or a data-section address
        // Reject: if meta_ptr itself look like a function (we'd double-count)
        // Heuristic: real COL/type_info pointers are > image_base but different from
        // the range we'd expect for code.  Just require ptr != 0 XOR looks_like_fn.
        // Simpler: if out.address == meta_ptr, reject (self-referential garbage).
        if (meta_ptr == out.address) return false;
    }
    
    size_t pos = offset;
    int slot = 0;
    
    // Scan for consecutive valid function pointers
    while (pos + ptr_size <= max_size && slot < 100) {  // Max 100 virtual functions
        uint64_t ptr_value = 0;
        
        if (ptr_size == 8) {
            ptr_value = *(const uint64_t*)(data + pos);
        } else {
            ptr_value = *(const uint32_t*)(data + pos);
        }
        
        if (!looks_like_function_ptr(ptr_value, image_base, binary_size)) {
            break;  // End of vtable
        }
        
        VirtualFunction vf;
        vf.slot_index = slot;
        vf.function_addr = ptr_value;
        vf.is_pure_virtual = false;
        
        // Generate placeholder name
        std::stringstream ss;
        ss << "vfunc_" << slot;
        vf.name = ss.str();
        
        out.entries.push_back(vf);
        
        pos += ptr_size;
        slot++;
    }
    
    // B-3: Require at least 3 entries to reduce false positives from random
    // pointer pairs in .rdata / string data.  Real vtables have >= 3 slots
    // in practice (destructor + at least two methods).
    return out.entries.size() >= 3;
}

void VTableAnalyzer::scan_vtables(const uint8_t* data, size_t size, uint64_t image_base, bool is_64bit) {
    if (!data || size == 0) return;
    
    int ptr_size = is_64bit ? 8 : 4;
    
    fission::utils::log_stream() << "[VTableAnalyzer] Scanning for vtables (ptr_size=" << ptr_size << ")..." << std::endl;
    
    // In real implementation, we'd scan .rdata section specifically
    // For now, scan entire binary for patterns of consecutive function pointers
    
    // Simple heuristic: look for aligned sequences that look like vtables
    // A real vtable often starts after RTTI pointer (which can be NULL or point to TypeInfo)
    
    size_t step = ptr_size;  // Check every pointer-sized offset
    int found = 0;
    
    for (size_t offset = 0; offset + ptr_size * 3 < size; offset += step) {
        // Quick check: does this look like start of vtable?
        uint64_t first_ptr = 0;
        if (is_64bit) {
            first_ptr = *(const uint64_t*)(data + offset);
        } else {
            first_ptr = *(const uint32_t*)(data + offset);
        }
        
        if (!looks_like_function_ptr(first_ptr, image_base, size)) continue;
        
        // Check if we already have a vtable at this address
        uint64_t addr = image_base + offset;
        if (vtable_index.count(addr)) continue;
        
        VTable vt;
        vt.has_rtti = false;
        vt.rtti_pointer = 0;
        
        if (scan_vtable_at(data, offset, size, image_base, size, ptr_size, vt)) {
            // D-1: O(log n) overlap check via vtable_index instead of O(n) linear scan.
            // The previous code iterated all 'vtables' for each candidate — O(n²)
            // overall. vtable_index is a sorted map, so lower_bound gives us the
            // nearest predecessor in O(log n) and we check one range overlap.
            bool overlaps = false;
            auto it = vtable_index.upper_bound(addr);
            if (it != vtable_index.begin()) {
                --it;  // largest known vtable address <= addr
                const VTable& prev = vtables[it->second];
                uint64_t prev_end = prev.address +
                                    prev.entries.size() * static_cast<size_t>(ptr_size);
                overlaps = (addr < prev_end);
            }
            
            if (!overlaps) {
                std::stringstream ss;
                ss << "vtable_" << std::hex << addr;
                vt.class_name = ss.str();
                
                vtable_index[addr] = vtables.size();
                vtables.push_back(vt);
                found++;
                
                // Skip past this vtable to avoid duplicates
                offset += vt.entries.size() * ptr_size - step;
            }
        }
    }
    
    fission::utils::log_stream() << "[VTableAnalyzer] Found " << found << " potential vtables" << std::endl;
}

void VTableAnalyzer::link_with_rtti(const std::map<uint64_t, std::string>& rtti_classes) {
    if (rtti_classes.empty()) return;

    int linked = 0;

    for (auto& vt : vtables) {
        // B-2: RttiAnalyzer now returns vtable_va -> class_name directly
        // (via CompleteObjectLocator chain for MSVC, or type_info chain for Itanium)
        // so we can do a simple direct lookup.
        auto it = rtti_classes.find(vt.address);
        if (it != rtti_classes.end()) {
            vt.class_name = it->second;
            vt.has_rtti = true;
            vt.rtti_pointer = vt.address; // vtable itself is the key
            ++linked;

            // Rename placeholder slot names using class name
            for (auto& entry : vt.entries) {
                std::stringstream ss;
                ss << vt.class_name << "::vfunc_" << entry.slot_index;
                entry.name = ss.str();
            }
        }
    }

    fission::utils::log_stream() << "[VTableAnalyzer] Linked " << linked
                                  << " vtables with RTTI class names" << std::endl;
}

const VTable* VTableAnalyzer::get_vtable(uint64_t addr) const {
    auto it = vtable_index.find(addr);
    if (it != vtable_index.end()) {
        return &vtables[it->second];
    }
    return nullptr;
}

uint64_t VTableAnalyzer::resolve_virtual_call(uint64_t vtable_addr, int slot_offset, int ptr_size) const {
    const VTable* vt = get_vtable(vtable_addr);
    if (!vt) return 0;
    
    int slot_index = slot_offset / ptr_size;
    if (slot_index < 0 || slot_index >= (int)vt->entries.size()) return 0;
    
    return vt->entries[slot_index].function_addr;
}

std::string VTableAnalyzer::get_virtual_call_name(uint64_t vtable_addr, int slot_offset, int ptr_size) const {
    const VTable* vt = get_vtable(vtable_addr);
    if (!vt) return "";

    int slot_index = slot_offset / ptr_size;
    if (slot_index < 0 || slot_index >= (int)vt->entries.size()) return "";

    // B-2: If the entry already has a resolved name (set by link_with_rtti), use it.
    const VirtualFunction& entry = vt->entries[slot_index];
    if (!entry.name.empty() && entry.name.find("vfunc_") == std::string::npos) {
        return entry.name; // already a resolved class::method name
    }

    // Fallback: class_name::vfunc_N  (or vtable_XXXX::vfunc_N if no RTTI)
    std::stringstream ss;
    ss << vt->class_name << "::vfunc_" << slot_index;
    return ss.str();
}

// ============================================================================
// P2-A: register_vtable_types
// For each detected vtable, create a TypeStruct in Ghidra's TypeFactory whose
// fields are code-pointer (TypePointer-to-TypeCode) entries, then register a
// global symbol at the vtable address so ActionInferTypes can propagate the
// derived type through indirect-call varnodes.
// ============================================================================
void VTableAnalyzer::register_vtable_types(ghidra::Architecture* arch) {
    if (!arch || vtables.empty()) return;

    ghidra::TypeFactory* tf = arch->types;
    if (!tf) return;

    ghidra::Scope* global_scope = arch->symboltab ? arch->symboltab->getGlobalScope() : nullptr;
    ghidra::AddrSpace* data_space = arch->getDefaultDataSpace();
    int ptr_size = data_space ? data_space->getAddrSize() : 8;

    // Canonical code pointer type: void (*)(void)
    ghidra::TypeCode* code_type = tf->getTypeCode();
    ghidra::Datatype* fn_ptr_type = tf->getTypePointer(ptr_size, code_type, ptr_size);

    int registered = 0;

    for (const VTable& vt : vtables) {
        if (vt.entries.empty()) continue;

        // Build struct name: "vtbl_ClassName" or "vtbl_addr_XXXX"
        std::string struct_name;
        if (!vt.class_name.empty() && vt.class_name.find("vtable_") == std::string::npos) {
            struct_name = "vtbl_" + vt.class_name;
        } else {
            std::ostringstream ns;
            ns << "vtbl_" << std::hex << vt.address;
            struct_name = ns.str();
        }

        // Reuse if already registered
        ghidra::Datatype* existing = tf->findByName(struct_name);
        ghidra::TypeStruct* vtbl_struct = nullptr;

        if (existing && existing->getMetatype() == ghidra::TYPE_STRUCT) {
            vtbl_struct = dynamic_cast<ghidra::TypeStruct*>(existing);
        } else if (!existing) {
            vtbl_struct = tf->getTypeStruct(struct_name);

            // Build fields: one fn_ptr per vtable slot in order
            std::vector<ghidra::TypeField> fields;
            int field_id = 0;
            for (const VirtualFunction& vf : vt.entries) {
                int offset = vf.slot_index * ptr_size;
                std::string fname = vf.name.empty() ? ("vfunc_" + std::to_string(vf.slot_index)) : vf.name;
                fields.push_back(ghidra::TypeField(field_id++, offset, fname, fn_ptr_type));
            }

            int struct_size = static_cast<int>(vt.entries.size()) * ptr_size;
            try {
                tf->setFields(fields, vtbl_struct, struct_size, ptr_size, 0);
            } catch (...) {
                fission::utils::log_stream() << "[VTableAnalyzer] setFields failed for " << struct_name << std::endl;
                continue;
            }
        }

        if (!vtbl_struct) continue;

        // Create TypePointer-to-struct for the vtable pointer lodged in objects
        ghidra::Datatype* struct_ptr = tf->getTypePointer(ptr_size, vtbl_struct, ptr_size);
        (void)struct_ptr; // Available for future use in parameter inference

        // Register a global symbol at the vtable address so Ghidra's
        // ActionConstantPtr / ActionMapGlobals can resolve references to it.
        if (global_scope && data_space) {
            ghidra::Address vtaddr(data_space, vt.address);
            if (!global_scope->findAddr(vtaddr, ghidra::Address())) {
                try {
                    global_scope->addSymbol(struct_name, vtbl_struct, vtaddr, ghidra::Address());
                    registered++;
                } catch (...) {
                    // Symbol may already exist under a different name — ignore
                }
            }
        }

        fission::utils::log_stream() << "[VTableAnalyzer] Registered vtable type '"
                  << struct_name << "' with " << vt.entries.size() << " slots at 0x"
                  << std::hex << vt.address << std::dec << std::endl;
    }

    if (registered > 0) {
        fission::utils::log_stream() << "[VTableAnalyzer] register_vtable_types: "
                  << registered << " vtable symbols added to global scope." << std::endl;
    }
}

} // namespace analysis
} // namespace fission
