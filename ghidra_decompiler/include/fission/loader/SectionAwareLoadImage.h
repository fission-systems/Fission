#ifndef FISSION_LOADER_SECTION_AWARE_LOAD_IMAGE_H
#define FISSION_LOADER_SECTION_AWARE_LOAD_IMAGE_H

#include "loadimage.hh"

#include <vector>
#include <cstdint>
#include <cstring>
#include <algorithm>

namespace fission {
namespace loader {

struct SectionMapping {
    uint64_t virtual_addr;     // Virtual address start
    uint64_t virtual_size;     // Size in virtual memory
    uint64_t file_offset;      // Offset in file
    uint64_t file_size;        // Size in file
    
    SectionMapping(uint64_t vaddr, uint64_t vsize, uint64_t foff, uint64_t fsize)
        : virtual_addr(vaddr), virtual_size(vsize), file_offset(foff), file_size(fsize) {}
};

/**
 * Section-aware load image that maps virtual addresses to file offsets
 * using PE section information
 */
class SectionAwareLoadImage : public ghidra::LoadImage {
    std::vector<uint8_t> file_data_;
    std::vector<SectionMapping> sections_;
    
public:
    SectionAwareLoadImage(const std::vector<uint8_t>& file_data)
        : ghidra::LoadImage("section-aware"), file_data_(file_data) {}
    
    void addSection(uint64_t va, uint64_t vsize, uint64_t file_offset, uint64_t file_size) {
        sections_.emplace_back(va, vsize, file_offset, file_size);
    }
    
    virtual void loadFill(ghidra::uint1* ptr, ghidra::int4 size, const ghidra::Address& addr) override {
        uint64_t va = addr.getOffset();
        
        // Zero-initialize output buffer
        std::memset(ptr, 0, size);
        
        // Find section containing this VA
        for (const auto& section : sections_) {
            uint64_t section_end = section.virtual_addr + section.virtual_size;
            
            // Check if requested range overlaps with this section
            if (va < section_end && va + size > section.virtual_addr) {
                // Calculate overlap
                uint64_t overlap_start = std::max(va, section.virtual_addr);
                uint64_t overlap_end = std::min(va + size, section_end);
                
                // Calculate file offset for this overlap
                uint64_t section_offset = overlap_start - section.virtual_addr;
                uint64_t file_off = section.file_offset + section_offset;
                uint64_t overlap_size = overlap_end - overlap_start;
                
                // Only copy if within file bounds
                if (section_offset < section.file_size) {
                    uint64_t available = std::min(overlap_size, section.file_size - section_offset);
                    
                    // Copy from file
                    uint64_t dest_offset = overlap_start - va;
                    if (file_off + available <= file_data_.size()) {
                        std::memcpy(ptr + dest_offset, file_data_.data() + file_off, available);
                    }
                }
                // Note: BSS sections (file_size < va_size) will remain zero-filled
            }
        }
    }
    
    virtual std::string getArchType(void) const override { return "section-aware"; }
    virtual void adjustVma(long adjust) override {}
};

} // namespace loader
} // namespace fission

#endif // FISSION_LOADER_SECTION_AWARE_LOAD_IMAGE_H
