/**
 * Fission FID Manager Implementation
 */

#include "fission/ffi/FidManager.h"
#include "fission/analysis/FidDatabase.h"
#include "fission/analysis/FunctionMatcher.h"

#include <iostream>
#include "fission/utils/logger.h"
#include <cstring>

using namespace fission::ffi;
using namespace fission::analysis;

DecompError fission::ffi::load_fid_database(DecompContext* ctx, const char* db_path) {
    if (!ctx || !db_path) return DECOMP_ERR_INVALID_CONTEXT;
    
    std::lock_guard<std::mutex> lock(ctx->mutex);
    
    try {
        auto new_db = std::make_unique<FidDatabase>();
        if (!new_db->load(db_path)) {
            ctx->last_error = "Failed to load FID database: ";
            ctx->last_error += db_path;
            return DECOMP_ERR_FID_LOAD;
        }
        
        if (ctx->fid_databases.empty()) {
            fission::utils::log_stream() << "[FidManager] Loaded FID database: " << db_path 
                      << " (" << new_db->get_function_count() << " functions)" << std::endl;
        }
        
        ctx->fid_databases.push_back(std::move(new_db));
        
        // Register the newly-loaded database with the matcher for multi-DB search
        ctx->matcher->add_fid_database(ctx->fid_databases.back().get());
        
        return DECOMP_OK;
    } catch (const std::exception& e) {
        ctx->last_error = e.what();
        return DECOMP_ERR_FID_LOAD;
    }
}

char* fission::ffi::get_fid_match(DecompContext* ctx, uint64_t addr, size_t len) {
    if (!ctx || !ctx->memory_image) return nullptr;
    
    std::lock_guard<std::mutex> lock(ctx->mutex);
    
    try {
        // Read bytes from memory image
        std::vector<uint8_t> code_bytes(len);
        try {
            // Check if address falls in binary range
            uint64_t offset = addr - ctx->base_addr;
            if (offset < ctx->binary_data.size()) {
                size_t avail = ctx->binary_data.size() - offset;
                size_t read_len = std::min(len, avail);
                memcpy(code_bytes.data(), ctx->binary_data.data() + offset, read_len);
                if (read_len < len) {
                    // Zero pad
                    memset(code_bytes.data() + read_len, 0, len - read_len);
                }
            } else {
                return nullptr; // Invalid address
            }
        } catch (...) {
            return nullptr;
        }
        
        // Perform match
        // Heuristic: if 64-bit, likely x86_64, which is x86 family. 
        // If 32-bit, likely x86 32-bit. 
        // Ghidra FID usually treats 'is_x86' as true for Intel architecture family.
        std::string match_name = ctx->matcher->match_by_fid(
            addr, 
            code_bytes.data(), 
            len, 
            true // Assuming x86/x64 family for now as per current limitation
        );
        
        if (!match_name.empty()) {
            return strdup(match_name.c_str());
        }
        
        return nullptr;
    } catch (...) {
        return nullptr;
    }
}
