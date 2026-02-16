#ifndef FISSION_LOADER_SECTION_AWARE_LOAD_IMAGE_H
#define FISSION_LOADER_SECTION_AWARE_LOAD_IMAGE_H

#include "loadimage.hh"

#include <vector>
#include <cstdint>
#include <cstring>
#include <algorithm>
#include <string>

namespace fission {
namespace loader {

struct SectionMapping {
    uint64_t virtual_addr;     // Virtual address start
    uint64_t virtual_size;     // Size in virtual memory
    uint64_t file_offset;      // Offset in file
    uint64_t file_size;        // Size in file
    bool is_executable;
    bool is_writable;
    std::string name;
    
    SectionMapping(
        uint64_t vaddr,
        uint64_t vsize,
        uint64_t foff,
        uint64_t fsize,
        bool executable,
        bool writable,
        std::string section_name
    ) : virtual_addr(vaddr),
        virtual_size(vsize),
        file_offset(foff),
        file_size(fsize),
        is_executable(executable),
        is_writable(writable),
        name(std::move(section_name)) {}
};

/**
 * Section-aware load image that maps virtual addresses to file offsets
 * using PE section information
 */
class SectionAwareLoadImage : public ghidra::LoadImage {
    struct LoaderSymbolRecord {
        uint64_t addr;
        std::string name;
    };

    std::vector<uint8_t> file_data_;
    std::vector<SectionMapping> sections_;
    ghidra::AddrSpace* default_space_ = nullptr;
    mutable std::vector<LoaderSymbolRecord> loader_symbols_;
    mutable size_t loader_symbol_index_ = 0;
    
public:
    explicit SectionAwareLoadImage(const std::vector<uint8_t>& file_data)
        : ghidra::LoadImage("section-aware"), file_data_(file_data) {}
    
    void addSection(
        uint64_t va,
        uint64_t vsize,
        uint64_t file_offset,
        uint64_t file_size,
        bool is_executable,
        bool is_writable,
        std::string name
    ) {
        sections_.emplace_back(
            va,
            vsize,
            file_offset,
            file_size,
            is_executable,
            is_writable,
            std::move(name)
        );
    }

    void setDefaultSpace(ghidra::AddrSpace* space) {
        default_space_ = space;
    }

    void addLoaderSymbol(uint64_t addr, const std::string& name) {
        if (name.empty()) {
            return;
        }
        for (auto& rec : loader_symbols_) {
            if (rec.addr == addr) {
                rec.name = name;
                return;
            }
        }
        loader_symbols_.push_back(LoaderSymbolRecord{addr, name});
    }

    void clearLoaderSymbols() {
        loader_symbols_.clear();
        loader_symbol_index_ = 0;
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
                // Only copy if within file bounds
                if (section_offset < section.file_size) {
                    uint64_t overlap_size = overlap_end - overlap_start;
                    uint64_t available = std::min(overlap_size, section.file_size - section_offset);
                    
                    // Copy from file
                    if (file_off + available <= file_data_.size()) {
                        uint64_t dest_offset = overlap_start - va;
                        std::memcpy(ptr + dest_offset, file_data_.data() + file_off, available);
                    }
                }
                // Note: BSS sections (file_size < va_size) will remain zero-filled
            }
        }
    }
    
    virtual void getReadonly(ghidra::RangeList &list) const override {
        if (!default_space_) {
            return;
        }

        for (const auto& section : sections_) {
            if (section.is_writable) {
                continue;
            }
            uint64_t size = section.virtual_size > 0 ? section.virtual_size : section.file_size;
            if (size == 0) {
                continue;
            }
            uint64_t start = section.virtual_addr;
            uint64_t stop = start + size - 1;
            if (stop < start) {
                stop = start;
            }
            list.insertRange(default_space_, start, stop);
        }
    }

    virtual std::string getArchType(void) const override { return "section-aware"; }
    virtual void adjustVma(long adjust) override {}

    virtual void openSymbols(void) const override {
        loader_symbol_index_ = 0;
    }

    virtual void closeSymbols(void) const override {
        loader_symbol_index_ = 0;
    }

    virtual bool getNextSymbol(ghidra::LoadImageFunc& record) const override {
        if (default_space_ == nullptr || loader_symbol_index_ >= loader_symbols_.size()) {
            return false;
        }
        const LoaderSymbolRecord& rec = loader_symbols_[loader_symbol_index_++];
        record.address = ghidra::Address(default_space_, rec.addr);
        record.name = rec.name;
        return true;
    }
};

} // namespace loader
} // namespace fission

#endif // FISSION_LOADER_SECTION_AWARE_LOAD_IMAGE_H
