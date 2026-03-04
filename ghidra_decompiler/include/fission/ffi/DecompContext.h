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
    class TypeStruct;
}

// Include actual headers for std::unique_ptr members
#include "fission/core/CliArchitecture.h"
#include "fission/core/SymbolProvider.h"
#include "fission/loader/SectionAwareLoadImage.h"
#include "fission/analysis/FidDatabase.h"
#include "fission/analysis/FunctionMatcher.h"
#include "fission/ffi/SymbolProviderFfi.h"
#include "fission/types/GlobalTypeRegistry.h"
#include "fission/decompiler/PostProcessPipeline.h"

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
    bool record_jumploads = true;
    bool disable_toomanyinstructions_error = true;
    bool allow_inline = false;

    // Output quality options (Phase 1 — Ghidra option activation)
    bool null_printing    = true;   ///< Print pointer 0 as NULL
    bool inplace_ops      = true;   ///< x = x+1 → x += 1
    bool no_cast_printing = false;  ///< Suppress safe casts (aggressive; off by default)
    bool convention_printing = false; ///< Show calling convention names

    // Post-processing options (configurable via set_feature with "pp_" prefix)
    fission::decompiler::PostProcessOptions post_process_options;
    
    // FID Support - Multiple databases for better matching
    std::vector<std::unique_ptr<fission::analysis::FidDatabase>> fid_databases;
    std::unique_ptr<fission::analysis::FunctionMatcher> matcher;

    // VTable virtual call display names
    // key: vtable address, value: map(slot_offset -> display_name)
    std::map<uint64_t, std::map<int, std::string>> vtable_virtual_names;

    // Slot-only fallback hints derived from scanned vtables
    // key: slot_offset, value: display_name
    std::map<int, std::string> vcall_slot_name_hints;

    // Slot-only fallback call targets derived from scanned vtables
    // key: slot_offset, value: function address
    std::map<int, uint64_t> vcall_slot_target_hints;

    // Struct type propagation across call sites
    std::map<uint64_t, std::map<int, std::string>> struct_registry;
    
    // Registered struct types for type recovery
    std::map<std::string, ghidra::TypeStruct*> registered_types;
    
    // Parameter type hints (func_addr -> param_index -> struct_name)
    std::map<uint64_t, std::map<int, std::string>> param_type_hints;

    // Cross-function type registry for call-graph propagation
    fission::types::GlobalTypeRegistry type_registry;

    // Cached per-section string table for string inlining.
    // Built once on first call to run_post_processing(); reused for every
    // subsequent function in the same binary (avoids O(n_funcs × section_size)).
    std::map<uint64_t, std::string> cached_string_table;
    bool string_table_built = false;
    
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
