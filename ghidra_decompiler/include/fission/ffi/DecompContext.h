/**
 * Fission Decompiler Context
 * 
 * Core context structure that holds all decompilation state.
 * Separated from libdecomp_ffi.cpp for better modularity.
 */

#ifndef FISSION_FFI_DECOMP_CONTEXT_H
#define FISSION_FFI_DECOMP_CONTEXT_H

#include <string>
#include <vector>
#include <map>
#include <memory>
#include <mutex>
#include <sstream>
#include <cstdint>

// Forward declarations
namespace ghidra {
    class Architecture;
}

// Include actual headers for std::unique_ptr members
#include "fission/core/CliArchitecture.h"
#include "fission/core/SymbolProvider.h"
#include "fission/loader/SectionAwareLoadImage.h"
#include "fission/analysis/FidDatabase.h"
#include "fission/analysis/FunctionMatcher.h"
#include "fission/ffi/SymbolProviderFfi.h"

namespace fission {
namespace ffi {

/**
 * Information about a memory block (section)
 */
struct MemoryBlockInfo {
    std::string name;
    uint64_t va_addr;          // Virtual address
    uint64_t va_size;          // Size in virtual memory
    uint64_t file_offset;      // Offset in PE file
    uint64_t file_size;        // Size in PE file
    bool is_executable;
    bool is_writable;
};

/**
 * Main decompiler context structure.
 * Contains all state needed for decompilation including memory image,
 * symbol table, architecture instance, and FID databases.
 */
struct DecompContext {
    // Configuration
    std::string sla_dir;
    std::string last_error;
    std::string gdt_path;
    
    // Memory image (using fission::loader::SectionAwareLoadImage)
    std::unique_ptr<fission::loader::SectionAwareLoadImage> memory_image;
    std::vector<uint8_t> binary_data;  // Keep raw PE data alive
    uint64_t base_addr = 0;
    bool is_64bit = true;
    std::string sleigh_id;
    std::string compiler_id;
    
    // Symbol table
    std::map<uint64_t, std::string> symbols;
    std::map<uint64_t, std::string> global_symbols;
    
    // Memory blocks (sections)
    std::vector<MemoryBlockInfo> memory_blocks;
    
    // Architecture (lazy-initialized)
    std::unique_ptr<fission::core::CliArchitecture> arch;
    std::unique_ptr<fission::core::SymbolProvider> symbol_provider;
    DecompSymbolProvider symbol_provider_callbacks;
    bool symbol_provider_enabled = false;
    
    // Error stream for architecture
    std::ostringstream err_stream;
    
    // Feature flags
    bool infer_pointers = true;
    bool analyze_loops = true;
    bool readonly_propagate = true;
    
    // FID Support - Multiple databases for better matching
    std::vector<std::unique_ptr<fission::analysis::FidDatabase>> fid_databases;
    std::unique_ptr<fission::analysis::FunctionMatcher> matcher;

    // Struct type propagation across call sites
    std::map<uint64_t, std::map<int, std::string>> struct_registry;
    
    // Thread safety
    std::mutex mutex;
    
    /**
     * Constructor
     * @param sla Sleigh language directory path
     */
    explicit DecompContext(const char* sla);
    
    /**
     * Destructor - handles cleanup with Ghidra workaround
     */
    ~DecompContext();
};

// ============================================================================
// Lifecycle Functions
// ============================================================================

/**
 * Initialize the Ghidra decompiler library (once per process)
 * @param sla_dir Path to Sleigh language specifications
 * @return true on success, false on failure
 */
bool initialize_ghidra_library(const std::string& sla_dir);

/**
 * Create a new decompiler context
 * @param sla_dir Path to Sleigh language specifications
 * @return New context or nullptr on failure
 */
DecompContext* create_context(const char* sla_dir);

/**
 * Destroy a decompiler context
 * @param ctx Context to destroy
 */
void destroy_context(DecompContext* ctx);

} // namespace ffi
} // namespace fission

#endif // FISSION_FFI_DECOMP_CONTEXT_H
