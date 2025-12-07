/**
 * Fission C Wrapper for Ghidra Decompiler
 * 
 * This file implements the C ABI interface for Rust FFI.
 * It wraps the C++ Ghidra decompiler with extern "C" functions.
 * 
 * Copyright 2024 Fission Dev Team
 * Licensed under Apache 2.0
 */

#include "wrapper.h"
#include "libdecomp.hh"
#include "sleigh_arch.hh"
#include "loadimage.hh"
#include "emulate.hh"
#include "context.hh"
#include "printc.hh"

#include <cstring>
#include <sstream>
#include <mutex>
#include <memory>

using namespace ghidra;

// Thread-safe error message storage
static thread_local std::string g_last_error;

/**
 * Custom LoadImage that reads from a memory buffer provided by Rust
 */
class BufferLoadImage : public LoadImage {
private:
    const uint8_t* m_buffer;
    size_t m_buffer_size;
    uint64_t m_base_addr;
    
public:
    BufferLoadImage(const uint8_t* buffer, size_t size, uint64_t base_addr)
        : LoadImage("buffer"), m_buffer(buffer), m_buffer_size(size), m_base_addr(base_addr) {}
    
    virtual void loadFill(uint1* ptr, int4 size, const Address& addr) override {
        uint64_t offset = addr.getOffset();
        
        if (offset < m_base_addr) {
            memset(ptr, 0, size);
            return;
        }
        
        uint64_t rel_offset = offset - m_base_addr;
        
        for (int4 i = 0; i < size; ++i) {
            if (rel_offset + i < m_buffer_size) {
                ptr[i] = m_buffer[rel_offset + i];
            } else {
                ptr[i] = 0;
            }
        }
    }
    
    virtual string getArchType(void) const override {
        return "buffer";
    }
    
    virtual void adjustVma(long adjust) override {}
};

/**
 * Internal decompiler state wrapper
 */
struct FissionDecompiler {
    std::unique_ptr<SleighArchitecture> arch;
    std::string sla_dir;
    bool initialized;
    std::mutex mutex;
    
    FissionDecompiler() : initialized(false) {}
};

// Global initialization flag
static bool g_library_initialized = false;
static std::mutex g_init_mutex;

/**
 * Initialize Ghidra library (thread-safe, called once)
 */
static bool ensureLibraryInitialized(const char* sla_dir) {
    std::lock_guard<std::mutex> lock(g_init_mutex);
    
    if (g_library_initialized) {
        return true;
    }
    
    try {
        std::vector<std::string> paths;
        paths.push_back(sla_dir);
        startDecompilerLibrary(paths);
        g_library_initialized = true;
        return true;
    } catch (const std::exception& e) {
        g_last_error = std::string("Failed to initialize library: ") + e.what();
        return false;
    }
}

extern "C" {

FissionDecompiler* fission_decompiler_init(const char* sla_dir) {
    if (!sla_dir) {
        g_last_error = "sla_dir is null";
        return nullptr;
    }
    
    if (!ensureLibraryInitialized(sla_dir)) {
        return nullptr;
    }
    
    try {
        FissionDecompiler* decomp = new FissionDecompiler();
        decomp->sla_dir = sla_dir;
        decomp->initialized = true;
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
    
    try {
        // Create a buffer-based load image
        BufferLoadImage loader(bytes, bytes_len, base_addr);
        ContextInternal context;
        
        // Find appropriate .sla file for x86-64 (default)
        std::string sla_file = decomp->sla_dir + "/x86-64.sla";
        
        // Initialize Sleigh translator
        Sleigh sleigh(&loader, &context);
        
        DocumentStorage docstorage;
        Element* root = docstorage.openDocument(sla_file)->getRoot();
        docstorage.registerTag(root);
        sleigh.initialize(docstorage);
        
        // Set x86-64 context defaults
        context.setVariableDefault("addrsize", 2);  // 64-bit addresses
        context.setVariableDefault("opsize", 1);    // 32-bit operands by default
        
        // Collect P-code and generate C output
        // Note: Full decompilation requires more infrastructure (Funcdata, Actions, etc.)
        // For now, we output a placeholder with lifted P-code
        
        std::ostringstream output;
        output << "// Decompiled by Fission (Ghidra Sleigh Engine)\n";
        output << "// Address: 0x" << std::hex << base_addr << std::dec << "\n\n";
        output << "void func_" << std::hex << base_addr << "() {\n";
        output << "    // P-code lifting placeholder\n";
        output << "    // Full decompilation requires Funcdata analysis\n";
        output << "}\n";
        
        std::string result = output.str();
        size_t copy_len = std::min(result.size(), out_len - 1);
        memcpy(out_buffer, result.c_str(), copy_len);
        out_buffer[copy_len] = '\0';
        
        return static_cast<int>(copy_len);
        
    } catch (const std::exception& e) {
        g_last_error = std::string("Decompilation failed: ") + e.what();
        return -1;
    }
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
    
    try {
        BufferLoadImage loader(bytes, bytes_len, base_addr);
        ContextInternal context;
        
        std::string sla_file = decomp->sla_dir + "/x86-64.sla";
        
        Sleigh sleigh(&loader, &context);
        
        DocumentStorage docstorage;
        Element* root = docstorage.openDocument(sla_file)->getRoot();
        docstorage.registerTag(root);
        sleigh.initialize(docstorage);
        
        context.setVariableDefault("addrsize", 2);
        context.setVariableDefault("opsize", 1);
        
        // Disassemble instructions
        std::ostringstream output;
        Address addr(sleigh.getDefaultCodeSpace(), base_addr);
        
        size_t offset = 0;
        int max_instructions = 100;
        int count = 0;
        
        // Custom AssemblyEmit that writes to our ostringstream
        class StringAssemblyEmit : public AssemblyEmit {
        public:
            std::ostringstream& out;
            StringAssemblyEmit(std::ostringstream& o) : out(o) {}
            
            virtual void dump(const Address& addr, const string& mnem, const string& body) override {
                out << std::hex << addr.getOffset() << ":  " << mnem;
                if (!body.empty()) {
                    out << " " << body;
                }
                out << "\n";
            }
        };
        
        StringAssemblyEmit asm_emit(output);
        
        while (offset < bytes_len && count < max_instructions) {
            Address current_addr(sleigh.getDefaultCodeSpace(), base_addr + offset);
            int4 instr_len = sleigh.printAssembly(asm_emit, current_addr);
            
            if (instr_len <= 0) {
                break;
            }
            
            offset += instr_len;
            count++;
        }
        
        std::string result = output.str();
        size_t copy_len = std::min(result.size(), out_len - 1);
        memcpy(out_buffer, result.c_str(), copy_len);
        out_buffer[copy_len] = '\0';
        
        return static_cast<int>(copy_len);
        
    } catch (const std::exception& e) {
        g_last_error = std::string("Disassembly failed: ") + e.what();
        return -1;
    }
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
