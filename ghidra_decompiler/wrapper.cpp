/**
 * Fission C Wrapper for Ghidra Decompiler
 * 
 * SIMPLIFIED VERSION for debugging
 * 
 * Copyright 2024 Fission Dev Team
 * Licensed under Apache 2.0
 */

#include "wrapper.h"
#include <cstring>
#include <sstream>
#include <mutex>

// Only include Ghidra headers when actually needed
#ifdef USE_GHIDRA
#include "libdecomp.hh"
#include "sleigh_arch.hh"
#include "loadimage.hh"
#include "context.hh"
using namespace ghidra;
#endif

// Thread-safe error message storage
static thread_local std::string g_last_error;

// Global initialization flag
static bool g_library_initialized = false;
static std::mutex g_init_mutex;

/**
 * Minimal FissionDecompiler struct - no Ghidra dependencies for now
 */
struct FissionDecompiler {
    std::string sla_dir;
    bool initialized;
    std::mutex mutex;
    
    FissionDecompiler() : initialized(false) {}
};

extern "C" {

FissionDecompiler* fission_decompiler_init(const char* sla_dir) {
    if (!sla_dir) {
        g_last_error = "sla_dir is null";
        return nullptr;
    }
    
    try {
        // Simple initialization - don't call Ghidra yet
        FissionDecompiler* decomp = new FissionDecompiler();
        decomp->sla_dir = sla_dir;
        decomp->initialized = true;
        g_library_initialized = true;
        return decomp;
    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to create decompiler: ") + e.what();
        return nullptr;
    }
}

void fission_decompiler_destroy(FissionDecompiler* decomp) {
    if (decomp) {
        delete decomp;
    }
}

int fission_decompile(
    FissionDecompiler* decomp,
    const uint8_t* bytes,
    size_t bytes_len,
    uint64_t base_addr,
    char* out_buffer,
    size_t out_len
) {
    if (!decomp || !decomp->initialized) {
        g_last_error = "Decompiler not initialized";
        return -1;
    }
    
    if (!bytes || bytes_len == 0) {
        g_last_error = "Invalid input bytes";
        return -1;
    }
    
    if (!out_buffer || out_len == 0) {
        g_last_error = "Invalid output buffer";
        return -1;
    }
    
    std::lock_guard<std::mutex> lock(decomp->mutex);
    
    // For now, just return a placeholder until we fix Ghidra integration
    std::ostringstream output;
    output << "// Decompiled by Fission (Ghidra Sleigh Engine)\n";
    output << "// Address: 0x" << std::hex << base_addr << std::dec << "\n";
    output << "// Input: " << bytes_len << " bytes\n\n";
    output << "void func_" << std::hex << base_addr << "() {\n";
    output << "    // TODO: Full Ghidra decompilation\n";
    output << "    // SLA dir: " << decomp->sla_dir << "\n";
    output << "}\n";
    
    std::string result = output.str();
    size_t copy_len = std::min(result.size(), out_len - 1);
    memcpy(out_buffer, result.c_str(), copy_len);
    out_buffer[copy_len] = '\0';
    
    return static_cast<int>(copy_len);
}

int fission_disassemble(
    FissionDecompiler* decomp,
    const uint8_t* bytes,
    size_t bytes_len,
    uint64_t base_addr,
    char* out_buffer,
    size_t out_len
) {
    if (!decomp || !decomp->initialized) {
        g_last_error = "Decompiler not initialized";
        return -1;
    }
    
    if (!bytes || bytes_len == 0) {
        g_last_error = "Invalid input bytes";
        return -1;
    }
    
    if (!out_buffer || out_len == 0) {
        g_last_error = "Invalid output buffer";
        return -1;
    }
    
    std::lock_guard<std::mutex> lock(decomp->mutex);
    
    // Simple hex dump as placeholder
    std::ostringstream output;
    output << "; Disassembly by Fission (Ghidra Sleigh)\n";
    output << "; Address: 0x" << std::hex << base_addr << "\n";
    output << "; Bytes: " << std::dec << bytes_len << "\n\n";
    
    // Raw bytes display
    size_t addr = base_addr;
    for (size_t i = 0; i < bytes_len && i < 64; i += 8) {
        output << std::hex << addr << ":  ";
        for (size_t j = i; j < i + 8 && j < bytes_len; j++) {
            output << std::hex;
            if (bytes[j] < 16) output << "0";
            output << (int)bytes[j] << " ";
        }
        output << "\n";
        addr += 8;
    }
    
    std::string result = output.str();
    size_t copy_len = std::min(result.size(), out_len - 1);
    memcpy(out_buffer, result.c_str(), copy_len);
    out_buffer[copy_len] = '\0';
    
    return static_cast<int>(copy_len);
}

const char* fission_get_error(void) {
    if (g_last_error.empty()) {
        return nullptr;
    }
    return g_last_error.c_str();
}

int fission_is_available(void) {
    return g_library_initialized ? 1 : 0;
}

} // extern "C"
