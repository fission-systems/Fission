/**
 * Fission Decompiler - Memory Load Image
 * Custom LoadImage implementation for in-memory binary data
 */

#ifndef FISSION_LOADER_MEMORY_IMAGE_H
#define FISSION_LOADER_MEMORY_IMAGE_H

#include <vector>
#include <cstdint>
#include <cstring>
#include "loadimage.hh"

namespace fission {
namespace loader {

using namespace ghidra;

/**
 * In-memory binary loader for Ghidra decompiler
 */
class MemoryLoadImage : public LoadImage {
    std::vector<uint8_t> data_;
    uint64_t base_addr_;
    mutable std::vector<LoadImageFunc> loader_symbols_;
    mutable size_t loader_symbol_index_ = 0;
    
public:
    MemoryLoadImage(const std::vector<uint8_t>& d, uint64_t base)
        : LoadImage("memory"), data_(d), base_addr_(base) {}
    
    /**
     * Update binary data (for reusing loader instance)
     */
    void updateData(const std::vector<uint8_t>& d, uint64_t base) {
        data_ = d;
        base_addr_ = base;
    }

    void addLoaderSymbol(uint64_t addr, const std::string& name) {
        if (name.empty()) {
            return;
        }
        LoadImageFunc rec;
        (void)addr;
        rec.address = Address();
        rec.name = name;
        loader_symbols_.push_back(std::move(rec));
    }

    void clearLoaderSymbols() {
        loader_symbols_.clear();
        loader_symbol_index_ = 0;
    }
    
    virtual void loadFill(uint1* ptr, int4 size, const Address& addr) override {
        uint64_t offset = addr.getOffset();
        const uint64_t max_offset = base_addr_ + data_.size();
        
        // Optimized bulk copy
        if (offset >= base_addr_ && offset + size <= max_offset) {
            std::memcpy(ptr, data_.data() + (offset - base_addr_), size);
        } else {
            // Fallback for boundary crossing
            for(int4 i = 0; i < size; ++i) {
                uint64_t cur = offset + i;
                if (cur >= base_addr_ && cur < max_offset) {
                    ptr[i] = static_cast<uint1>(data_[cur - base_addr_]);
                } else {
                    ptr[i] = 0;
                }
            }
        }
    }
    
    virtual std::string getArchType(void) const override { return "memory"; }
    virtual void adjustVma(long adjust) override {}

    virtual void openSymbols(void) const override {
        loader_symbol_index_ = 0;
    }

    virtual void closeSymbols(void) const override {
        loader_symbol_index_ = 0;
    }

    virtual bool getNextSymbol(LoadImageFunc& record) const override {
        if (loader_symbol_index_ >= loader_symbols_.size()) {
            return false;
        }
        record = loader_symbols_[loader_symbol_index_++];
        return true;
    }
};

} // namespace loader
} // namespace fission

#endif // FISSION_LOADER_MEMORY_IMAGE_H
