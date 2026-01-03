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
#include "fission/loader/BinaryDetector.h"
#include "fission/types/TypeManager.h"
#include "fission/types/GdtBinaryParser.h"
#include "fission/types/PrototypeEnforcer.h"
#include "fission/types/StructureAnalyzer.h"
#include "fission/types/GuidParser.h"
#include "fission/analysis/FunctionMatcher.h"
#include "fission/processing/PostProcessors.h"
#include "fission/processing/Constants.h"
#include "fission/processing/StringScanner.h"
#include "fission/utils/file_utils.h"
#include "libdecomp.hh"
#include "sleigh_arch.hh"

#include <cstring>
#include <string>
#include <map>
#include <memory>
#include <mutex>
#include <fstream>

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

struct DecompContext {
    std::string sla_dir;
    std::string last_error;
    std::string gdt_path;
    
    // Memory image (using fission::loader::MemoryLoadImage)
    std::unique_ptr<MemoryLoadImage> memory_image;
    std::vector<uint8_t> binary_data;  // Keep data alive
    uint64_t base_addr = 0;
    bool is_64bit = true;
    
    // Symbol table
    std::map<uint64_t, std::string> symbols;
    
    // Architecture (lazy-initialized)
    std::unique_ptr<CliArchitecture> arch;
    
    // Error stream for architecture
    std::ostringstream err_stream;
    
    // Feature flags
    bool infer_pointers = true;
    bool analyze_loops = true;
    bool readonly_propagate = true;
    
    // Thread safety
    std::mutex mutex;
    
    DecompContext(const char* sla) : sla_dir(sla ? sla : "") {}
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
        // Store binary data (must keep alive for MemoryLoadImage)
        ctx->binary_data.assign(data, data + len);
        
        // Create memory image using fission::loader::MemoryLoadImage
        ctx->memory_image = std::make_unique<MemoryLoadImage>(ctx->binary_data, base_addr);
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
    
    // Inject IAT symbols
    if (!ctx->symbols.empty()) {
        ctx->arch->injectIatSymbols(ctx->symbols);
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
    
    // CRITICAL: Clear global scope before each decompilation to prevent zombie Function objects
    // Without this, repeated decompilations corrupt internal state and cause crashes
    global_scope->clear();
    
    // Create function address
    AddrSpace* code_space = ctx->arch->getDefaultCodeSpace();
    if (!code_space) {
        throw std::runtime_error("Code space not initialized");
    }
    Address start_addr(code_space, addr);
    
    std::cerr << "[libdecomp FFI] Looking up function..." << std::endl;
    
    // Check if function exists at address
    Funcdata* fd = global_scope->findFunction(start_addr);
    if (!fd) {
        // Create new function with generated name
        std::ostringstream name_ss;
        name_ss << "sub_" << std::hex << addr;
        FunctionSymbol* sym = global_scope->addFunction(start_addr, name_ss.str());
        if (!sym) {
            throw std::runtime_error("Failed to add function");
        }
        fd = sym->getFunction();
    }
    
    if (!fd) {
        throw std::runtime_error("Failed to get function data");
    }
    
    std::cerr << "[libdecomp FFI] Performing decompilation..." << std::endl;
    
    // Check action group
    Action* current_action = ctx->arch->allacts.getCurrent();
    if (!current_action) {
        throw std::runtime_error("No current action group");
    }
    
    // Reset action state for this function
    current_action->reset(*fd);
    
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
    
    // Step 10: Apply FID-resolved function names (if any loaded)
    // Using empty map for now - could integrate with FID database
    std::map<uint64_t, std::string> fid_names;
    result = apply_fid_names(result, fid_names);
    
    std::cerr << "[libdecomp FFI] Decompilation complete, " << result.size() << " bytes after post-processing" << std::endl;
    
    return result;
}

