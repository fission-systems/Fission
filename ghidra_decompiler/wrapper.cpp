/**
 * Fission C Wrapper for Ghidra Decompiler
 * 
 * Implements the C ABI interface for Rust FFI.
 * Directly uses SleighArchitecture and PrintC for high performance.
 */

#include "wrapper.h"
#include <cstring>
#include <sstream>
#include <mutex>
#include <memory>
#include <vector>
#include <iostream>

#include "libdecomp.hh"
#include "sleigh_arch.hh"
#include "loadimage.hh"
#include "flow.hh"

using namespace ghidra;

// Thread-safe error message storage
static thread_local std::string g_last_error;

// Custom LoadImage - feeds bytes to Sleigh
class MemoryLoadImage : public LoadImage {
    std::vector<uint8_t> data_;
    uint64_t base_addr_;
public:
    MemoryLoadImage(const uint8_t* d, size_t len, uint64_t base) 
        : LoadImage("memory"), base_addr_(base) {
        data_.assign(d, d + len);
    }
    
    virtual void loadFill(uint1 *ptr, int4 size, const Address &addr) override {
        uint64_t offset = addr.getOffset();
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
    
    FissionDecompiler() : initialized(false) {}
};

extern "C" {

FissionDecompiler* fission_decompiler_init(const char* sla_dir) {
    if (!sla_dir) {
        g_last_error = "sla_dir is null";
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
        g_last_error = e.explain;
        return nullptr;
    } catch (const std::exception& e) {
        g_last_error = e.what();
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
        g_last_error = "Decompiler not initialized";
        return -1;
    }
    
    std::lock_guard<std::mutex> lock(decomp->mutex);
    
    try {
        // Reset previous state to avoid memory issues
        decomp->arch.reset();
        decomp->loader.reset();
        
        // Select architecture based on binary type
        const char* arch_id = is_64bit ? "x86:LE:64:default" : "x86:LE:32:default";
        
        // Prepare architecture for this binary
        decomp->loader = std::make_unique<MemoryLoadImage>(bytes, bytes_len, base_addr);
        decomp->arch = std::make_unique<ServerArchitecture>(arch_id, decomp->loader.get(), &std::cerr);
        
        DocumentStorage store;
        decomp->arch->init(store);
        decomp->arch->max_instructions = 200000;
        decomp->arch->flowoptions &= ~FlowInfo::error_toomanyinstructions;
        
        // Decompile
        Address func_addr(decomp->arch->getDefaultCodeSpace(), base_addr);
        Scope* global_scope = decomp->arch->symboltab->getGlobalScope();
        Funcdata* fd = global_scope->findFunction(func_addr);
        if (fd == nullptr) {
            fd = global_scope->addFunction(func_addr, "func")->getFunction();
        }
        
        decomp->arch->allacts.getCurrent()->reset(*fd);
        decomp->arch->allacts.getCurrent()->perform(*fd);
        
        std::ostringstream c_stream;
        decomp->arch->print->setOutputStream(&c_stream);
        decomp->arch->print->docFunction(fd);
        
        std::string result = c_stream.str();
        size_t copy_len = std::min(result.size(), out_len - 1);
        memcpy(out_buffer, result.c_str(), copy_len);
        out_buffer[copy_len] = '\0';
        
        return static_cast<int>(copy_len);
    } catch (const LowlevelError& e) {
        g_last_error = e.explain;
        return -1;
    } catch (const std::exception& e) {
        g_last_error = e.what();
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
    return 0; 
}

const char* fission_get_error(void) {
    return g_last_error.empty() ? nullptr : g_last_error.c_str();
}

int fission_is_available(void) {
    return 1;
}

} // extern "C"
