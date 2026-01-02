#include "fission/analysis/VTableAnalyzer.h"

#include <iostream>
#include <sstream>
#include <cstring>

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
    // 1. Be within the binary's code section (roughly image_base to image_base + size)
    // 2. Not be zero or obviously invalid
    if (addr == 0) return false;
    if (addr < image_base) return false;
    if (addr > image_base + binary_size + 0x100000) return false; // Some leeway
    return true;
}

bool VTableAnalyzer::scan_vtable_at(const uint8_t* data, size_t offset, size_t max_size,
                                     uint64_t image_base, size_t binary_size, int ptr_size, VTable& out) {
    out.entries.clear();
    out.address = image_base + offset;
    
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
    
    // Require at least 2 entries to be considered a vtable
    return out.entries.size() >= 2;
}

void VTableAnalyzer::scan_vtables(const uint8_t* data, size_t size, uint64_t image_base, bool is_64bit) {
    if (!data || size == 0) return;
    
    int ptr_size = is_64bit ? 8 : 4;
    
    std::cerr << "[VTableAnalyzer] Scanning for vtables (ptr_size=" << ptr_size << ")..." << std::endl;
    
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
            // Skip if overlaps with existing vtable
            bool overlaps = false;
            for (const auto& existing : vtables) {
                if (addr >= existing.address && 
                    addr < existing.address + existing.entries.size() * ptr_size) {
                    overlaps = true;
                    break;
                }
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
    
    std::cerr << "[VTableAnalyzer] Found " << found << " potential vtables" << std::endl;
}

void VTableAnalyzer::link_with_rtti(const std::map<uint64_t, std::string>& rtti_classes) {
    if (rtti_classes.empty()) return;
    
    int linked = 0;
    
    for (auto& vt : vtables) {
        // RTTI type info is typically just before the vtable
        // Check a few offsets before the vtable address
        for (int off = 0; off <= 16; off += 8) {
            uint64_t check_addr = vt.address - off;
            auto it = rtti_classes.find(check_addr);
            if (it != rtti_classes.end()) {
                vt.class_name = it->second;
                vt.has_rtti = true;
                vt.rtti_pointer = check_addr;
                linked++;
                break;
            }
        }
        
        // Also check if any vtable entry matches a recovered class
        for (auto& entry : vt.entries) {
            // For inherited vtables, the entry might already have a class association
            // This is a simplification - real RTTI linking is more complex
        }
    }
    
    std::cerr << "[VTableAnalyzer] Linked " << linked << " vtables with RTTI class names" << std::endl;
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
    
    std::stringstream ss;
    ss << vt->class_name << "::vfunc_" << slot_index;
    return ss.str();
}

} // namespace analysis
} // namespace fission
