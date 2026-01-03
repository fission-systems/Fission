/**
 * Fission Decompiler FFI Implementation
 * 
 * C++ implementation of the FFI interface defined in libdecomp_ffi.h.
 * Wraps DecompilerContext for use from Rust via extern "C".
 */

#include "fission/ffi/libdecomp_ffi.h"
#include "fission/core/DecompilerContext.h"
#include "fission/core/CliArchitecture.h"
#include "fission/core/ArchPolicy.h"
#include "fission/loader/MemoryImage.h"
#include "fission/loader/SectionAwareLoadImage.h"
#include "fission/loader/BinaryDetector.h"
#include "fission/types/TypeManager.h"
#include "fission/types/GdtBinaryParser.h"
#include "fission/types/PrototypeEnforcer.h"
#include "fission/types/StructureAnalyzer.h"
#include "fission/types/GuidParser.h"
#include "fission/analysis/FunctionMatcher.h"
#include "fission/analysis/FidDatabase.h"
#include "fission/processing/PostProcessors.h"
#include "fission/processing/Constants.h"
#include "fission/processing/StringScanner.h"
#include "fission/utils/file_utils.h"
#include "libdecomp.hh"
#include "sleigh_arch.hh"

#include <cstring>
#include <string>
#include <map>
#include <set>
#include <regex>
#include <memory>
#include <mutex>
#include <fstream>
#include <iomanip>

using namespace ghidra;
using namespace fission::core;
using namespace fission::loader;
using namespace fission::types;
using namespace fission::analysis;
using namespace fission::processing;
using namespace fission::utils;

// ============================================================================
// Internal Context Structure
// ============================================================================

struct MemoryBlockInfo {
    std::string name;
    uint64_t va_addr;          // Virtual address
    uint64_t va_size;          // Size in virtual memory
    uint64_t file_offset;      // Offset in PE file
    uint64_t file_size;        // Size in PE file
    bool is_executable;
    bool is_writable;
};

struct DecompContext {
    std::string sla_dir;
    std::string last_error;
    std::string gdt_path;
    
    // Memory image (using fission::loader::SectionAwareLoadImage)
    std::unique_ptr<SectionAwareLoadImage> memory_image;
    std::vector<uint8_t> binary_data;  // Keep data alive
    uint64_t base_addr = 0;
    bool is_64bit = true;
    
    // Symbol table
    std::map<uint64_t, std::string> symbols;
    
    // Memory blocks (sections)
    std::vector<MemoryBlockInfo> memory_blocks;
    
    // Architecture (lazy-initialized)
    std::unique_ptr<CliArchitecture> arch;
    
    // Error stream for architecture
    std::ostringstream err_stream;
    
    // Feature flags
    bool infer_pointers = true;
    bool analyze_loops = true;
    bool readonly_propagate = true;
    
    // FID Support - Multiple databases for better matching
    std::vector<std::unique_ptr<FidDatabase>> fid_databases;
    std::unique_ptr<FunctionMatcher> matcher;
    
    // Thread safety
    std::mutex mutex;
    
    DecompContext(const char* sla) : sla_dir(sla ? sla : "") {
        matcher = std::make_unique<FunctionMatcher>();
    }
};

// ============================================================================
// Lifecycle Management
// ============================================================================

// Static flag to ensure library is only initialized once
static bool ghidra_library_initialized = false;
static std::mutex init_mutex;

static bool initialize_ghidra_library(const std::string& sla_dir) {
    std::lock_guard<std::mutex> lock(init_mutex);
    
    if (ghidra_library_initialized) {
        return true;
    }
    
    try {
        // Initialize the Ghidra decompiler library
        ghidra::startDecompilerLibrary(sla_dir.c_str());
        
        // Set up Sleigh spec paths
        std::string langDir = sla_dir;
        if (langDir.length() < 9 || langDir.substr(langDir.length() - 9) != "languages") {
            langDir += "/languages";
        }
        
        ghidra::SleighArchitecture::specpaths.addDir2Path(langDir);
        ghidra::SleighArchitecture::getDescriptions();
        
        ghidra_library_initialized = true;
        std::cerr << "[libdecomp FFI] Ghidra library initialized with specpath: " << langDir << std::endl;
        return true;
    } catch (const LowlevelError& e) {
        std::cerr << "[libdecomp FFI] Failed to init Ghidra: " << e.explain << std::endl;
        return false;
    } catch (...) {
        std::cerr << "[libdecomp FFI] Unknown error during Ghidra init" << std::endl;
        return false;
    }
}

extern "C" DECOMP_API DecompContext* decomp_create(const char* sla_dir) {
    try {
        // Initialize Ghidra library first (only once)
        if (sla_dir && !initialize_ghidra_library(sla_dir)) {
            return nullptr;
        }
        
        return new DecompContext(sla_dir);
    } catch (...) {
        return nullptr;
    }
}

extern "C" DECOMP_API void decomp_destroy(DecompContext* ctx) {
    if (ctx) {
        // WORKAROUND: Release the architecture pointer instead of destroying it
        // Ghidra's Architecture destructor can crash after decompilation due to
        // internal state corruption. This is a minor memory leak but prevents crash.
        // The architecture lives for the process lifetime anyway.
        if (ctx->arch) {
            ctx->arch.release(); // Leak instead of crash
        }
        delete ctx;
    }
}

// ============================================================================
// Binary Loading
// ============================================================================

extern "C" DECOMP_API DecompError decomp_load_binary(
    DecompContext* ctx,
    const uint8_t* data,
    size_t len,
    uint64_t base_addr,
    int is_64bit
) {
    if (!ctx) return DECOMP_ERR_INVALID_CONTEXT;
    
    std::lock_guard<std::mutex> lock(ctx->mutex);
    
    try {
        // Store binary data (PE raw file data)
        ctx->binary_data.assign(data, data + len);
        
        // Create section-aware memory image (sections will be added via decomp_add_memory_block)
        ctx->memory_image = std::make_unique<SectionAwareLoadImage>(ctx->binary_data);
        ctx->base_addr = base_addr;
        ctx->is_64bit = (is_64bit != 0);
        
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

// ============================================================================
// Symbol Management
// ============================================================================

extern "C" DECOMP_API void decomp_add_symbol(
    DecompContext* ctx,
    uint64_t addr,
    const char* name
) {
    if (!ctx || !name) return;
    
    std::lock_guard<std::mutex> lock(ctx->mutex);
    ctx->symbols[addr] = name;
}

extern "C" DECOMP_API void decomp_clear_symbols(DecompContext* ctx) {
    if (!ctx) return;
    
    std::lock_guard<std::mutex> lock(ctx->mutex);
    ctx->symbols.clear();
}

extern "C" DECOMP_API DecompError decomp_add_function(
    DecompContext* ctx,
    uint64_t addr,
    const char* name
) {
    if (!ctx) return DECOMP_ERR_INVALID_CONTEXT;
    
    std::lock_guard<std::mutex> lock(ctx->mutex);
    
    try {
        // Store the function declaration (will be applied when arch is created)
        std::string func_name = name ? name : ("FUN_" + std::to_string(addr));
        ctx->symbols[addr] = func_name;
        
        // If architecture already exists, add function immediately
        if (ctx->arch && ctx->memory_image) {
            Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
            if (global_scope) {
                Address func_addr(ctx->arch->getDefaultCodeSpace(), addr);
                
                // Check if function already exists
                Funcdata* existing = global_scope->findFunction(func_addr);
                if (!existing) {
                    // Create new function
                    global_scope->addFunction(func_addr, func_name);
                    std::cerr << "[libdecomp FFI] Declared function at 0x" << std::hex << addr 
                              << std::dec << ": " << func_name << std::endl;
                }
            }
        }
        
        return DECOMP_OK;
    } catch (const std::exception& e) {
        ctx->last_error = std::string("Failed to add function: ") + e.what();
        return DECOMP_ERR_DECOMPILE;
    } catch (...) {
        ctx->last_error = "Unknown error in decomp_add_function";
        return DECOMP_ERR_DECOMPILE;
    }
}

extern "C" DECOMP_API DecompError decomp_add_memory_block(
    DecompContext* ctx,
    const char* name,
    uint64_t va_addr,
    uint64_t va_size,
    uint64_t file_offset,
    uint64_t file_size,
    int is_executable,
    int is_writable
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
        block.is_executable = (is_executable != 0);
        block.is_writable = (is_writable != 0);
        
        ctx->memory_blocks.push_back(block);
        
        // Add section mapping to the memory image
        if (ctx->memory_image) {
            ctx->memory_image->addSection(va_addr, va_size, file_offset, file_size);
        }
        
        std::cerr << "[libdecomp FFI] Registered memory block: " << name 
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
        ctx->last_error = "Unknown error in decomp_add_memory_block";
        return DECOMP_ERR_LOAD;
    }
}

// ============================================================================
// Decompilation
// ============================================================================

// Forward declarations for helper functions
static std::string run_decompilation(DecompContext* ctx, uint64_t addr);
static void ensure_architecture(DecompContext* ctx);

extern "C" DECOMP_API char* decomp_function(DecompContext* ctx, uint64_t addr) {
    if (!ctx) return nullptr;
    
    std::lock_guard<std::mutex> lock(ctx->mutex);
    
    try {
        std::string result = run_decompilation(ctx, addr);
        
        // Allocate and copy result (caller must free)
        char* output = static_cast<char*>(malloc(result.size() + 1));
        if (output) {
            std::memcpy(output, result.c_str(), result.size() + 1);
        }
        return output;
    } catch (const LowlevelError& e) {
        ctx->last_error = std::string("Ghidra error: ") + e.explain;
        return nullptr;
    } catch (const std::exception& e) {
        ctx->last_error = std::string("Error: ") + e.what();
        return nullptr;
    } catch (...) {
        ctx->last_error = "Unknown decompilation error";
        return nullptr;
    }
}

extern "C" DECOMP_API const char* decomp_get_last_error(DecompContext* ctx) {
    if (!ctx) return "Invalid context";
    return ctx->last_error.c_str();
}

extern "C" DECOMP_API void decomp_free_string(char* str) {
    if (str) {
        free(str);
    }
}

// ============================================================================
// Configuration
// ============================================================================

extern "C" DECOMP_API DecompError decomp_set_gdt(DecompContext* ctx, const char* gdt_path) {
    if (!ctx) return DECOMP_ERR_INVALID_CONTEXT;
    
    std::lock_guard<std::mutex> lock(ctx->mutex);
    ctx->gdt_path = gdt_path ? gdt_path : "";
    return DECOMP_OK;
}

extern "C" DECOMP_API void decomp_set_feature(
    DecompContext* ctx,
    const char* feature,
    int enabled
) {
    if (!ctx || !feature) return;
    
    std::lock_guard<std::mutex> lock(ctx->mutex);
    
    std::string feat(feature);
    bool on = (enabled != 0);
    
    if (feat == "infer_pointers") {
        ctx->infer_pointers = on;
    } else if (feat == "analyze_loops") {
        ctx->analyze_loops = on;
    } else if (feat == "readonly_propagate") {
        ctx->readonly_propagate = on;
    }
}

// ============================================================================
// FID Support
// ============================================================================

extern "C" DECOMP_API DecompError decomp_load_fid_db(DecompContext* ctx, const char* db_path) {
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
            std::cerr << "[libdecomp FFI] Loaded FID database: " << db_path 
                      << " (" << new_db->get_function_count() << " functions)" << std::endl;
        }
        
        ctx->fid_databases.push_back(std::move(new_db));
        
        // Update matcher with all databases (matcher will search through all of them)
        // For now, set the first one. We'll improve lookup logic later.
        if (!ctx->fid_databases.empty()) {
            ctx->matcher->set_fid_database(ctx->fid_databases[0].get());
        }
        
        return DECOMP_OK;
    } catch (const std::exception& e) {
        ctx->last_error = e.what();
        return DECOMP_ERR_FID_LOAD;
    }
}

extern "C" DECOMP_API char* decomp_get_fid_match(DecompContext* ctx, uint64_t addr, size_t len) {
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

// ============================================================================
// Internal Helpers
// ============================================================================

// Helper: Load GDT types for architecture
static void load_gdt_for_arch(ghidra::Architecture* arch, bool is_64bit) {
    std::string suffix = is_64bit ? "_64.gdt" : "_32.gdt";
    std::vector<std::string> candidates = {
        "../../ghidra/typeinfo/win32/windows_vs12" + suffix,
        "../ghidra/typeinfo/win32/windows_vs12" + suffix,
        "./ghidra/typeinfo/win32/windows_vs12" + suffix,
        "ghidra/typeinfo/win32/windows_vs12" + suffix
    };
    
    for (const auto& path : candidates) {
        if (file_exists(path)) {
            std::cerr << "[libdecomp FFI] Loading GDT from: " << path << std::endl;
            GdtBinaryParser gdt;
            if (gdt.load(path)) {
                TypeManager::load_types_from_gdt(arch->types, &gdt, ArchPolicy::getPointerSize(arch));
            }
            break;
        }
    }
}

// Helper: Load GUID maps for substitution
static std::map<std::string, std::string> load_guid_maps() {
    std::map<std::string, std::string> guid_map;
    
    std::vector<std::string> guid_files = {
        "../../ghidra/typeinfo/win32/msvcrt/guids.txt",
        "../ghidra/typeinfo/win32/msvcrt/guids.txt",
        "./ghidra/typeinfo/win32/msvcrt/guids.txt",
        "ghidra/typeinfo/win32/msvcrt/guids.txt",
        "../../ghidra/typeinfo/win32/msvcrt/iids.txt",
        "../ghidra/typeinfo/win32/msvcrt/iids.txt",
        "./ghidra/typeinfo/win32/msvcrt/iids.txt",
        "ghidra/typeinfo/win32/msvcrt/iids.txt"
    };
    
    for (const auto& path : guid_files) {
        if (file_exists(path)) {
            std::string content = read_file_content(path);
            if (!content.empty()) {
                std::map<std::string, std::string> loaded = load_guids_to_map(content);
                guid_map.insert(loaded.begin(), loaded.end());
            }
        }
    }
    
    if (!guid_map.empty()) {
        std::cerr << "[libdecomp FFI] Loaded " << guid_map.size() << " GUIDs/IIDs" << std::endl;
    }
    
    return guid_map;
}

// Global GUID map (loaded once)
static std::map<std::string, std::string> g_guid_map;
static bool g_guid_map_loaded = false;

static void ensure_architecture(DecompContext* ctx) {
    if (ctx->arch) return;
    
    // Determine sleigh language ID from binary type
    std::string sleigh_id = ctx->is_64bit ? "x86:LE:64:default" : "x86:LE:32:default";
    
    // Create architecture with correct constructor: (sleigh_id, MemoryLoadImage*, ostream*)
    ctx->arch = std::make_unique<CliArchitecture>(
        sleigh_id,
        ctx->memory_image.get(),
        &ctx->err_stream
    );
    
    // CRITICAL: Initialize Sleigh engine and register print languages
    ghidra::DocumentStorage store;
    ctx->arch->init(store);
    
    // Configure advanced options (infer_pointers, analyze_loops, etc.)
    configure_arch(ctx->arch.get());
    
    // Register Windows types (DWORD, HANDLE, etc.)
    TypeManager::register_windows_types(ctx->arch->types, ArchPolicy::getPointerSize(ctx->arch.get()));
    
    // Load GDT type information
    load_gdt_for_arch(ctx->arch.get(), ctx->is_64bit);
    
    // Inject IAT symbols and register functions
    if (!ctx->symbols.empty()) {
        std::cerr << "[libdecomp FFI] Injecting " << ctx->symbols.size() << " symbols" << std::endl;
        ctx->arch->injectIatSymbols(ctx->symbols);
        
        // Register all symbols as functions in global scope
        Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
        if (global_scope) {
            AddrSpace* code_space = ctx->arch->getDefaultCodeSpace();
            std::cerr << "[libdecomp FFI] Using code space for registration: " << code_space->getName() << std::endl;
            
            int func_count = 0;
            int existing_count = 0;
            int failed_count = 0;
            for (const auto& [addr, name] : ctx->symbols) {
                try {
                    Address func_addr(ctx->arch->getDefaultCodeSpace(), addr);
                    
                    // Check if function already exists
                    Funcdata* existing = global_scope->findFunction(func_addr);
                    if (!existing) {
                        // Create new function
                        FunctionSymbol* sym = global_scope->addFunction(func_addr, name);
                        if (sym) {
                            func_count++;
                        } else {
                            failed_count++;
                            std::cerr << "[libdecomp FFI] Failed to add function at 0x" << std::hex << addr << std::dec << ": " << name << std::endl;
                        }
                    } else {
                        existing_count++;
                    }
                } catch (const std::exception& e) {
                    failed_count++;
                    std::cerr << "[libdecomp FFI] Exception adding function at 0x" << std::hex << addr << std::dec << ": " << e.what() << std::endl;
                } catch (...) {
                    failed_count++;
                }
            }
            std::cerr << "[libdecomp FFI] Function registration: " << func_count << " added, " 
                      << existing_count << " already exist, " << failed_count << " failed" << std::endl;
            std::cerr << "[libdecomp FFI] Global scope: " << (void*)global_scope << std::endl;
        }
    }
    
    // Register memory blocks (sections) if any
    if (!ctx->memory_blocks.empty()) {
        std::cerr << "[libdecomp FFI] Registering " << ctx->memory_blocks.size() << " memory blocks" << std::endl;
        
        // SectionAwareLoadImage handles the VA-to-file-offset mapping
        // The sections have already been registered via decomp_add_memory_block
        for (const auto& block : ctx->memory_blocks) {
            std::cerr << "  - " << block.name 
                      << ": VA 0x" << std::hex << block.va_addr << "-0x" << (block.va_addr + block.va_size)
                      << std::dec << " (vsize: " << block.va_size << " bytes, "
                      << "file_off: 0x" << std::hex << block.file_offset << std::dec << ", "
                      << (block.is_executable ? "CODE" : "DATA") << ")" << std::endl;
        }
    }
    
    // Load GUID map (once)
    if (!g_guid_map_loaded) {
        g_guid_map = load_guid_maps();
        g_guid_map_loaded = true;
    }
    
    std::cerr << "[libdecomp FFI] Architecture initialized: " << sleigh_id << std::endl;
}

static std::string run_decompilation(DecompContext* ctx, uint64_t addr) {
    if (!ctx->memory_image) {
        throw std::runtime_error("No binary loaded");
    }
    
    ensure_architecture(ctx);
    
    std::cerr << "[libdecomp FFI] Starting decompilation at 0x" << std::hex << addr << std::dec << std::endl;
    
    // Validate architecture components
    if (!ctx->arch) {
        throw std::runtime_error("Architecture not initialized");
    }
    if (!ctx->arch->symboltab) {
        throw std::runtime_error("Symbol table not initialized");
    }
    
    // Get global scope
    Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
    if (!global_scope) {
        throw std::runtime_error("Global scope not initialized");
    }
    
    std::cerr << "[libdecomp FFI] Global scope in decompilation: " << (void*)global_scope << std::endl;
    
    // NOTE: We don't clear global scope here anymore because we want to keep
    // registered functions and symbols. Instead, we clear only the specific
    // function we're about to decompile.
    
    // Create function address
    AddrSpace* code_space = ctx->arch->getDefaultCodeSpace();
    if (!code_space) {
        throw std::runtime_error("Code space not initialized");
    }
    Address start_addr(code_space, addr);
    
    std::cerr << "[libdecomp FFI] Looking up function at code space=" 
              << code_space->getName() << ", addr=0x" << std::hex << addr << std::dec << std::endl;
    
    // Check if function exists at address
    Funcdata* fd = global_scope->findFunction(start_addr);
    if (!fd) {
        // Check if we have a registered name for this address
        std::string func_name;
        auto it = ctx->symbols.find(addr);
        if (it != ctx->symbols.end()) {
            func_name = it->second;
            std::cerr << "[libdecomp FFI] Found registered name for 0x" << std::hex << addr << std::dec << ": " << func_name << std::endl;
        } else {
            // Generate name
            std::ostringstream name_ss;
            name_ss << "sub_" << std::hex << addr;
            func_name = name_ss.str();
            std::cerr << "[libdecomp FFI] No registered name, using: " << func_name << std::endl;
        }
        
        FunctionSymbol* sym = global_scope->addFunction(start_addr, func_name);
        if (!sym) {
            throw std::runtime_error("Failed to add function");
        }
        fd = sym->getFunction();
        std::cerr << "[libdecomp FFI] Created new function at 0x" << std::hex << addr << std::dec << " with name: " << func_name << std::endl;
    } else {
        std::cerr << "[libdecomp FFI] Found existing function at 0x" << std::hex << addr << std::dec << ": " << fd->getName() << std::endl;
    }
    
    if (!fd) {
        throw std::runtime_error("Failed to get function data");
    }
    
    // Clear only this function's data for fresh analysis
    fd->clear();
    
    std::cerr << "[libdecomp FFI] Following control flow..." << std::endl;
    
    // Debug: Check if we can read memory at this address
    uint8_t test_byte;
    try {
        ctx->memory_image->loadFill(&test_byte, 1, start_addr);
        std::cerr << "[libdecomp FFI] Successfully read first byte at 0x" << std::hex << addr << ": 0x" << (int)test_byte << std::dec << std::endl;
    } catch (const std::exception& e) {
        std::cerr << "[libdecomp FFI] ERROR: Cannot read memory at 0x" << std::hex << addr << std::dec << ": " << e.what() << std::endl;
    }
    
    // CRITICAL: Follow control flow to discover instructions
    // This disassembles the function and builds the control flow graph
    Address end_addr = start_addr + 0x1000; // Reasonable function size limit
    try {
        fd->followFlow(start_addr, end_addr);
        std::cerr << "[libdecomp FFI] Control flow analysis complete" << std::endl;
    } catch (const std::exception& e) {
        std::cerr << "[libdecomp FFI] ERROR in followFlow: " << e.what() << std::endl;
    } catch (...) {
        std::cerr << "[libdecomp FFI] ERROR: Unknown exception in followFlow" << std::endl;
    }
    
    // Check action group
    Action* current_action = ctx->arch->allacts.getCurrent();
    if (!current_action) {
        throw std::runtime_error("No current action group");
    }
    
    // CRITICAL: Reset action state for this function AFTER clear and followFlow
    std::cerr << "[libdecomp FFI] Resetting action state..." << std::endl;
    current_action->reset(*fd);
    
    std::cerr << "[libdecomp FFI] Performing decompilation..." << std::endl;
    
    // Perform decompilation
    current_action->perform(*fd);
    
    std::cerr << "[libdecomp FFI] Generating output..." << std::endl;
    
    // Check print language
    if (!ctx->arch->print) {
        throw std::runtime_error("Print language not initialized");
    }
    
    // Print decompiled output to string
    std::ostringstream ss;
    ctx->arch->print->setOutputStream(&ss);
    ctx->arch->print->docFunction(fd);
    
    std::string result = ss.str();
    
    std::cerr << "[libdecomp FFI] Raw output: " << result.size() << " bytes, post-processing..." << std::endl;
    
    // ========================================================================
    // Full Post-Processing Chain (matching Pool mode fission_decomp.cpp)
    // ========================================================================
    
    // Step 1: IAT symbol replacement (pcRamXXX -> function_name)
    result = post_process_iat_calls(result, ctx->symbols);
    
    // Step 2: Smart constant replacement (context-aware API parameter naming)
    result = smart_constant_replace(result);
    
    // Step 2.5: String inlining - Add inline comments for string addresses
    // Only scan .rdata section (typical string location in PE binaries)
    std::map<uint64_t, std::string> string_table;
    
    // Find .rdata section and scan only that
    for (const auto& block : ctx->memory_blocks) {
        if (block.name == ".rdata" && block.file_size > 0) {
            // Calculate offset in binary_data
            size_t start_idx = block.file_offset;
            size_t end_idx = start_idx + block.file_size;
            
            if (end_idx <= ctx->binary_data.size()) {
                std::vector<uint8_t> rdata_section(
                    ctx->binary_data.begin() + start_idx,
                    ctx->binary_data.begin() + end_idx
                );
                string_table = StringScanner::scan_ascii_strings(rdata_section, block.va_addr);
            }
            break;
        }
    }
    
    if (!string_table.empty()) {
        result = inline_strings(result, string_table);
    }
    
    // Step 3: Fallback constant replacement for enum values
    // Using empty map for now - could be expanded to load from config
    std::map<uint64_t, std::string> enum_values;
    result = post_process_constants(result, enum_values);
    
    // Step 4: GUID substitution (IID_IUnknown, CLSID_*, etc.)
    if (!g_guid_map.empty()) {
        result = substitute_guids(result, g_guid_map);
    }
    
    // Step 5: Unicode string recovery
    result = recover_unicode_strings(result);
    
    // Step 6: Interlocked pattern replacement (LOCK prefix -> Interlocked*)
    result = replace_interlocked_patterns(result);
    
    // Step 7: xunknown/undefined type replacement
    result = replace_xunknown_types(result);
    
    // Step 8: SEH boilerplate cleanup
    result = cleanup_seh_boilerplate(result);
    
    // Step 9: Internal function naming improvement (func_0x -> sub_)
    result = improve_internal_function_names(result);
    
    // Step 9.5: Structure offset annotation (param_1 + 10 -> param_1 + 10 /* &value */)
    result = annotate_structure_offsets(result);
    
    // Step 10: Apply FID-resolved function names from loaded databases
    if (!ctx->fid_databases.empty() && ctx->matcher) {
        std::map<uint64_t, std::string> fid_names;
        
        // Use multiple FID databases for function matching
        
        // Extract all function addresses from decompiled output (sub_XXXXXXXX pattern)
        std::regex func_pattern(R"(sub_([0-9a-fA-F]{8,16}))");
        std::smatch match;
        std::string::const_iterator search_start(result.cbegin());
        std::set<uint64_t> found_addrs;
        
        while (std::regex_search(search_start, result.cend(), match, func_pattern)) {
            try {
                uint64_t func_addr = std::stoull(match[1].str(), nullptr, 16);
                found_addrs.insert(func_addr);
            } catch (...) {
                // Ignore parse errors
            }
            search_start = match.suffix().first;
        }
        
        // Match each found address with FID databases
        int fid_matches = 0;
        for (uint64_t func_addr : found_addrs) {
            // Try to read function bytes from memory image
            try {
                std::vector<uint8_t> code_bytes(64); // Read up to 64 bytes
                Address read_addr(ctx->arch->getDefaultCodeSpace(), func_addr);
                ctx->memory_image->loadFill(code_bytes.data(), 64, read_addr);
                
                // Calculate FID hash using FidHasher
                uint64_t hash = FidHasher::calculate_full_hash(code_bytes.data(), code_bytes.size());
                
                // Search through all loaded FID databases
                bool found_match = false;
                for (size_t db_idx = 0; db_idx < ctx->fid_databases.size() && !found_match; ++db_idx) {
                    std::vector<std::string> names = ctx->fid_databases[db_idx]->lookup_by_hash(hash);
                    if (!names.empty()) {
                        // Use first match (could be improved with ranking)
                        fid_names[func_addr] = names[0];
                        fid_matches++;
                        found_match = true;
                    }
                }
            } catch (const std::exception& e) {
                std::cerr << "[libdecomp FFI] Error matching 0x" << std::hex << func_addr 
                          << std::dec << ": " << e.what() << std::endl;
            } catch (...) {
                std::cerr << "[libdecomp FFI] Unknown error matching 0x" << std::hex << func_addr << std::dec << std::endl;
            }
        }
        
        if (fid_matches > 0) {
            result = apply_fid_names(result, fid_names);
        }
    }
    
    std::cerr << "[libdecomp FFI] Decompilation complete, " << result.size() << " bytes after post-processing" << std::endl;
    
    return result;
}

