/**
 * Fission C Wrapper for Ghidra Decompiler
 * * Implements the C ABI interface for Rust FFI.
 * Optimized with Architecture Caching to prevent memory explosion.
 */

#include "wrapper.h"
#include <cstring>
#include <sstream>
#include <mutex>
#include <memory>
#include <vector>
#include <iostream>
#include <fstream> // For null stream

#include "libdecomp.hh"
#include "sleigh_arch.hh"
#include "loadimage.hh"
#include "flow.hh"

using namespace ghidra;

// Thread-safe error message storage
static thread_local std::string g_last_error;

// Null buffer to silence logs
class NullBuffer : public std::streambuf {
public:
    int overflow(int c) { return c; }
};
static NullBuffer null_buffer;
static std::ostream null_stream(&null_buffer);

// Custom LoadImage - feeds bytes to Sleigh
class MemoryLoadImage : public LoadImage {
    std::vector<uint8_t> data_;
    uint64_t base_addr_;
public:
    MemoryLoadImage(const uint8_t* d, size_t len, uint64_t base) 
        : LoadImage("memory"), base_addr_(base) {
        data_.assign(d, d + len);
    }
    
    // [최적화] 데이터 갱신 메서드 추가 (객체 재생성 방지)
    void update(const uint8_t* d, size_t len, uint64_t base) {
        data_.assign(d, d + len);
        base_addr_ = base;
    }
    
    virtual void loadFill(uint1 *ptr, int4 size, const Address &addr) override {
        uint64_t offset = addr.getOffset();
        // Check for integer overflow
        if (base_addr_ > UINT64_MAX - data_.size()) return;
        uint64_t max = base_addr_ + data_.size();
        
        for(int4 i = 0; i < size; ++i) {
            uint64_t cur = offset + i;
            if (cur >= base_addr_ && cur < max) {
                ptr[i] = static_cast<uint1>(data_[cur - base_addr_]);
            } else {
                ptr[i] = 0;
            }
        }
    }
    
    virtual std::string getArchType(void) const override { return "memory"; }
    virtual void adjustVma(long adjust) override {}
};

// Custom Architecture that uses our MemoryLoadImage
// Custom Architecture that uses our MemoryLoadImage
class ServerArchitecture : public SleighArchitecture {
    MemoryLoadImage* custom_loader;
public:
    ServerArchitecture(const std::string& sleigh_id, MemoryLoadImage* ldr, std::ostream* err)
        : SleighArchitecture("", sleigh_id, err), custom_loader(ldr) {}
    
    virtual void buildLoader(DocumentStorage& store) override {
        loader = custom_loader;
    }
};

struct FissionDecompiler {
    std::unique_ptr<MemoryLoadImage> loader;
    std::unique_ptr<ServerArchitecture> arch;
    std::mutex mutex;
    bool initialized;
    std::string sla_dir;
    
    // [Optimization] Track last architecture state
    int last_is_64bit = -1; 
    
    // [Safety] Instance-specific error storage
    std::string last_error;

    FissionDecompiler() : initialized(false) {}
};

extern "C" {

FissionDecompiler* fission_decompiler_init(const char* sla_dir) {
    if (!sla_dir) {
        std::cerr << "[fission_wrapper] sla_dir is null" << std::endl;
        return nullptr;
    }
    
    try {
        startDecompilerLibrary(sla_dir);
        
        FissionDecompiler* decomp = new FissionDecompiler();
        decomp->sla_dir = sla_dir;
        
        // Manually add the languages directory to specpaths
        std::string langDir = std::string(sla_dir) + "/languages";
        SleighArchitecture::specpaths.addDir2Path(langDir);
        SleighArchitecture::getDescriptions();
        
        decomp->initialized = true;
        return decomp;
    } catch (const LowlevelError& e) {
        std::cerr << "[fission_wrapper] Init Error: " << e.explain << std::endl;
        return nullptr;
    } catch (const std::exception& e) {
        std::cerr << "[fission_wrapper] Init Error: " << e.what() << std::endl;
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
    int is_64bit,
    char* out_buffer,
    size_t out_len
) {
    if (!decomp || !decomp->initialized) {
        return -1;
    }
    
    std::lock_guard<std::mutex> lock(decomp->mutex);
    decomp->last_error.clear();
    
    try {
        // [Optimization] Initialize architecture transactional-y only if needed
        if (!decomp->arch || decomp->last_is_64bit != is_64bit) {
            
            const char* arch_id = is_64bit ? "x86:LE:64:default" : "x86:LE:32:default";
            
            // [Safety] Transactional initialization: Create new objects first
            auto new_loader = std::make_unique<MemoryLoadImage>(bytes, bytes_len, base_addr);
            // Use null_stream for logging to prevent memory explosion
            auto new_arch = std::make_unique<ServerArchitecture>(arch_id, new_loader.get(), &null_stream);
            
            DocumentStorage store;
            new_arch->init(store); // This might throw
            
            // If we get here, initialization succeeded. Commit changes.
            decomp->loader = std::move(new_loader);
            decomp->arch = std::move(new_arch);
            decomp->last_is_64bit = is_64bit;
            
        } else {
            // [Optimization] Reuse existing object: update data
            decomp->loader->update(bytes, bytes_len, base_addr);
            
            // Clear previous analysis
            decomp->arch->symboltab->getGlobalScope()->clear();
        }

        // Common config
        decomp->arch->max_instructions = 200000;
        decomp->arch->flowoptions &= ~FlowInfo::error_toomanyinstructions;
        
        // Decompile logic
        Address func_addr(decomp->arch->getDefaultCodeSpace(), base_addr);
        Scope* global_scope = decomp->arch->symboltab->getGlobalScope();
        
        // Find or create function
        Funcdata* fd = global_scope->findFunction(func_addr);
        if (fd == nullptr) {
            fd = global_scope->addFunction(func_addr, "func")->getFunction();
        }
        
        // Perform actions
        decomp->arch->allacts.getCurrent()->reset(*fd);
        decomp->arch->allacts.getCurrent()->perform(*fd);
        
        std::ostringstream c_stream;
        decomp->arch->print->setOutputStream(&c_stream);
        decomp->arch->print->docFunction(fd);
        
        std::string result = c_stream.str();
        
        // [Safety] Buffer check and Return required size
        int required = static_cast<int>(result.size());
        
        if (out_len == 0) {
            decomp->last_error = "Output buffer size is zero";
            return required + 1; // Indicate required size
        }
        
        if (result.size() >= out_len) {
            decomp->last_error = "Buffer too small";
            return required + 1; // Indicate required size
        }
        
        // Safe copy
        size_t copy_len = result.size();
        memcpy(out_buffer, result.c_str(), copy_len);
        out_buffer[copy_len] = '\0'; // Null terminator
        
        return static_cast<int>(copy_len);
        
    } catch (const LowlevelError& e) {
        decomp->last_error = e.explain;
        return -1;
    } catch (const std::exception& e) {
        decomp->last_error = e.what();
        return -1;
    } catch (...) {
        decomp->last_error = "Unknown error";
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
    if (decomp) {
        decomp->last_error = "Not implemented";
    }
    return -1; 
}

const char* fission_get_error(FissionDecompiler* decomp) {
    if (!decomp) return "null decompiler";
    return decomp->last_error.empty() ? nullptr : decomp->last_error.c_str();
}

int fission_is_available(void) {
    return 1;
}

} // extern "C"
