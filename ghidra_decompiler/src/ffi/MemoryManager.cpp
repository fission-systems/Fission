/**
 * Fission Memory Manager Implementation
 */

#include "fission/ffi/MemoryManager.h"
#include "fission/loader/SectionAwareLoadImage.h"

#include <iostream>
#include <stdexcept>

using namespace fission::ffi;
using namespace fission::loader;

DecompError fission::ffi::load_binary(
    DecompContext* ctx,
    const uint8_t* data,
    size_t len,
    uint64_t base_addr,
    bool is_64bit
) {
    if (!ctx) return DECOMP_ERR_INVALID_CONTEXT;
    
    std::lock_guard<std::mutex> lock(ctx->mutex);
    
    try {
        // Store binary data (PE raw file data)
        ctx->binary_data.assign(data, data + len);
        
        // Create section-aware memory image (sections will be added via add_memory_block)
        ctx->memory_image = std::make_unique<SectionAwareLoadImage>(ctx->binary_data);
        ctx->base_addr = base_addr;
        ctx->is_64bit = is_64bit;
        
        // Reset architecture (will be created on first decompile)
        ctx->arch.reset();
        
        return DECOMP_OK;
    } catch (const std::exception& e) {
        ctx->last_error = e.what();
        return DECOMP_ERR_LOAD;
    } catch (...) {
        ctx->last_error = "Unknown error during binary load";
        return DECOMP_ERR_LOAD;
    }
}

DecompError fission::ffi::add_memory_block(
    DecompContext* ctx,
    const char* name,
    uint64_t va_addr,
    uint64_t va_size,
    uint64_t file_offset,
    uint64_t file_size,
    bool is_executable,
    bool is_writable
) {
    if (!ctx || !name) return DECOMP_ERR_INVALID_CONTEXT;
    
    std::lock_guard<std::mutex> lock(ctx->mutex);
    
    try {
        MemoryBlockInfo block;
        block.name = name;
        block.va_addr = va_addr;
        block.va_size = va_size;
        block.file_offset = file_offset;
        block.file_size = file_size;
        block.is_executable = is_executable;
        block.is_writable = is_writable;
        
        ctx->memory_blocks.push_back(block);
        
        // Add section mapping to the memory image
        if (ctx->memory_image) {
            ctx->memory_image->addSection(
                va_addr,
                va_size,
                file_offset,
                file_size,
                is_executable,
                is_writable,
                block.name
            );
        }
        
        std::cerr << "[MemoryManager] Registered memory block: " << name 
                  << " at VA 0x" << std::hex << va_addr << std::dec
                  << " (vsize: " << va_size << ", file_off: 0x" << std::hex << file_offset 
                  << std::dec << ", fsize: " << file_size << ", "
                  << (block.is_executable ? "executable" : "data")
                  << (block.is_writable ? ", writable" : ", readonly")
                  << ")" << std::endl;
        
        return DECOMP_OK;
    } catch (const std::exception& e) {
        ctx->last_error = std::string("Failed to add memory block: ") + e.what();
        return DECOMP_ERR_LOAD;
    } catch (...) {
        ctx->last_error = "Unknown error in add_memory_block";
        return DECOMP_ERR_LOAD;
    }
}
