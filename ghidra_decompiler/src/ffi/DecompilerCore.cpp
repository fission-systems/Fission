/**
 * Fission Decompiler Core Implementation
 */

#include "fission/ffi/DecompilerCore.h"
#include "fission/core/CliArchitecture.h"
#include "fission/core/ArchPolicy.h"
#include "fission/types/TypeManager.h"
#include "fission/types/GdtBinaryParser.h"
#include "fission/types/GuidParser.h"
#include "fission/analysis/FidDatabase.h"
#include "fission/processing/PostProcessors.h"
#include "fission/processing/StringScanner.h"
#include "fission/utils/file_utils.h"
#include "libdecomp.hh"

#include <iostream>
#include <regex>
#include <set>

using namespace fission::ffi;
using namespace fission::core;
using namespace fission::types;
using namespace fission::processing;
using namespace fission::analysis;
using namespace fission::utils;

// Global GUID map (loaded once)
static std::map<std::string, std::string> g_guid_map;
static bool g_guid_map_loaded = false;

// ============================================================================
// Helper Functions
// ============================================================================

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
            std::cerr << "[DecompilerCore] Loading GDT from: " << path << std::endl;
            GdtBinaryParser gdt;
            if (gdt.load(path)) {
                TypeManager::load_types_from_gdt(arch->types, &gdt, ArchPolicy::getPointerSize(arch));
            }
            break;
        }
    }
}

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
        std::cerr << "[DecompilerCore] Loaded " << guid_map.size() << " GUIDs/IIDs" << std::endl;
    }
    
    return guid_map;
}

// ============================================================================
// Public API
// ============================================================================

void fission::ffi::ensure_architecture(DecompContext* ctx) {
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
        std::cerr << "[DecompilerCore] Injecting " << ctx->symbols.size() << " symbols" << std::endl;
        ctx->arch->injectIatSymbols(ctx->symbols);
        
        // Register all symbols as functions in global scope
        ghidra::Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
        if (global_scope) {
            ghidra::AddrSpace* code_space = ctx->arch->getDefaultCodeSpace();
            std::cerr << "[DecompilerCore] Using code space for registration: " << code_space->getName() << std::endl;
            
            int func_count = 0;
            int existing_count = 0;
            int failed_count = 0;
            for (const auto& [addr, name] : ctx->symbols) {
                try {
                    ghidra::Address func_addr(ctx->arch->getDefaultCodeSpace(), addr);
                    
                    // Check if function already exists
                    ghidra::Funcdata* existing = global_scope->findFunction(func_addr);
                    if (!existing) {
                        // Create new function
                        ghidra::FunctionSymbol* sym = global_scope->addFunction(func_addr, name);
                        if (sym) {
                            func_count++;
                        } else {
                            failed_count++;
                            std::cerr << "[DecompilerCore] Failed to add function at 0x" << std::hex << addr << std::dec << ": " << name << std::endl;
                        }
                    } else {
                        existing_count++;
                    }
                } catch (const std::exception& e) {
                    failed_count++;
                    std::cerr << "[DecompilerCore] Exception adding function at 0x" << std::hex << addr << std::dec << ": " << e.what() << std::endl;
                } catch (...) {
                    failed_count++;
                }
            }
            std::cerr << "[DecompilerCore] Function registration: " << func_count << " added, " 
                      << existing_count << " already exist, " << failed_count << " failed" << std::endl;
            std::cerr << "[DecompilerCore] Global scope: " << (void*)global_scope << std::endl;
        }
    }
    
    // Register memory blocks (sections) if any
    if (!ctx->memory_blocks.empty()) {
        std::cerr << "[DecompilerCore] Registering " << ctx->memory_blocks.size() << " memory blocks" << std::endl;
        
        // SectionAwareLoadImage handles the VA-to-file-offset mapping
        // The sections have already been registered via add_memory_block
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
    
    std::cerr << "[DecompilerCore] Architecture initialized: " << sleigh_id << std::endl;
}

std::string fission::ffi::run_decompilation(DecompContext* ctx, uint64_t addr) {
    if (!ctx->memory_image) {
        throw std::runtime_error("No binary loaded");
    }
    
    ensure_architecture(ctx);
    
    std::cerr << "[DecompilerCore] Starting decompilation at 0x" << std::hex << addr << std::dec << std::endl;
    
    // Validate architecture components
    if (!ctx->arch) {
        throw std::runtime_error("Architecture not initialized");
    }
    if (!ctx->arch->symboltab) {
        throw std::runtime_error("Symbol table not initialized");
    }
    
    // Get global scope
    ghidra::Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
    if (!global_scope) {
        throw std::runtime_error("Global scope not initialized");
    }
    
    std::cerr << "[DecompilerCore] Global scope in decompilation: " << (void*)global_scope << std::endl;
    
    // Create function address
    ghidra::AddrSpace* code_space = ctx->arch->getDefaultCodeSpace();
    if (!code_space) {
        throw std::runtime_error("Code space not initialized");
    }
    ghidra::Address start_addr(code_space, addr);
    
    std::cerr << "[DecompilerCore] Looking up function at code space=" 
              << code_space->getName() << ", addr=0x" << std::hex << addr << std::dec << std::endl;
    
    // Check if function exists at address
    ghidra::Funcdata* fd = global_scope->findFunction(start_addr);
    if (!fd) {
        // Check if we have a registered name for this address
        std::string func_name;
        auto it = ctx->symbols.find(addr);
        if (it != ctx->symbols.end()) {
            func_name = it->second;
            std::cerr << "[DecompilerCore] Found registered name for 0x" << std::hex << addr << std::dec << ": " << func_name << std::endl;
        } else {
            // Generate name
            std::ostringstream name_ss;
            name_ss << "sub_" << std::hex << addr;
            func_name = name_ss.str();
            std::cerr << "[DecompilerCore] No registered name, using: " << func_name << std::endl;
        }
        
        ghidra::FunctionSymbol* sym = global_scope->addFunction(start_addr, func_name);
        if (!sym) {
            throw std::runtime_error("Failed to add function");
        }
        fd = sym->getFunction();
        std::cerr << "[DecompilerCore] Created new function at 0x" << std::hex << addr << std::dec << " with name: " << func_name << std::endl;
    } else {
        std::cerr << "[DecompilerCore] Found existing function at 0x" << std::hex << addr << std::dec << ": " << fd->getName() << std::endl;
    }
    
    if (!fd) {
        throw std::runtime_error("Failed to get function data");
    }
    
    // Clear only this function's data for fresh analysis
    fd->clear();
    
    std::cerr << "[DecompilerCore] Following control flow..." << std::endl;
    
    // Debug: Check if we can read memory at this address
    uint8_t test_byte;
    try {
        ctx->memory_image->loadFill(&test_byte, 1, start_addr);
        std::cerr << "[DecompilerCore] Successfully read first byte at 0x" << std::hex << addr << ": 0x" << (int)test_byte << std::dec << std::endl;
    } catch (const std::exception& e) {
        std::cerr << "[DecompilerCore] ERROR: Cannot read memory at 0x" << std::hex << addr << std::dec << ": " << e.what() << std::endl;
    }
    
    // CRITICAL: Follow control flow to discover instructions
    ghidra::Address end_addr = start_addr + 0x1000;
    try {
        fd->followFlow(start_addr, end_addr);
        std::cerr << "[DecompilerCore] Control flow analysis complete" << std::endl;
    } catch (const std::exception& e) {
        std::cerr << "[DecompilerCore] ERROR in followFlow: " << e.what() << std::endl;
    } catch (...) {
        std::cerr << "[DecompilerCore] ERROR: Unknown exception in followFlow" << std::endl;
    }
    
    // Check action group
    ghidra::Action* current_action = ctx->arch->allacts.getCurrent();
    if (!current_action) {
        throw std::runtime_error("No current action group");
    }
    
    // CRITICAL: Reset action state for this function AFTER clear and followFlow
    std::cerr << "[DecompilerCore] Resetting action state..." << std::endl;
    current_action->reset(*fd);
    
    std::cerr << "[DecompilerCore] Performing decompilation..." << std::endl;
    
    // Perform decompilation
    current_action->perform(*fd);
    
    std::cerr << "[DecompilerCore] Generating output..." << std::endl;
    
    // Check print language
    if (!ctx->arch->print) {
        throw std::runtime_error("Print language not initialized");
    }
    
    // Print decompiled output to string
    std::ostringstream ss;
    ctx->arch->print->setOutputStream(&ss);
    ctx->arch->print->docFunction(fd);
    
    std::string result = ss.str();
    
    std::cerr << "[DecompilerCore] Raw output: " << result.size() << " bytes, post-processing..." << std::endl;
    
    // ========================================================================
    // Full Post-Processing Chain
    // ========================================================================
    
    // Step 1: IAT symbol replacement
    result = post_process_iat_calls(result, ctx->symbols);
    
    // Step 2: Smart constant replacement
    result = smart_constant_replace(result);
    
    // Step 2.5: String inlining
    std::map<uint64_t, std::string> string_table;
    for (const auto& block : ctx->memory_blocks) {
        if (block.name == ".rdata" && block.file_size > 0) {
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
    
    // Step 3: Constant replacement
    std::map<uint64_t, std::string> enum_values;
    result = post_process_constants(result, enum_values);
    
    // Step 4: GUID substitution
    if (!g_guid_map.empty()) {
        result = substitute_guids(result, g_guid_map);
    }
    
    // Step 5: Unicode string recovery
    result = recover_unicode_strings(result);
    
    // Step 6: Interlocked pattern replacement
    result = replace_interlocked_patterns(result);
    
    // Step 7: xunknown/undefined type replacement
    result = replace_xunknown_types(result);
    
    // Step 8: SEH boilerplate cleanup
    result = cleanup_seh_boilerplate(result);
    
    // Step 9: Internal function naming improvement
    result = improve_internal_function_names(result);
    
    // Step 9.5: Structure offset annotation
    result = annotate_structure_offsets(result);
    
    // Step 10: Apply FID-resolved function names
    if (!ctx->fid_databases.empty() && ctx->matcher) {
        std::map<uint64_t, std::string> fid_names;
        
        // Extract all function addresses from decompiled output
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
            try {
                std::vector<uint8_t> code_bytes(64);
                ghidra::Address read_addr(ctx->arch->getDefaultCodeSpace(), func_addr);
                ctx->memory_image->loadFill(code_bytes.data(), 64, read_addr);
                
                uint64_t hash = FidHasher::calculate_full_hash(code_bytes.data(), code_bytes.size());
                
                bool found_match = false;
                for (size_t db_idx = 0; db_idx < ctx->fid_databases.size() && !found_match; ++db_idx) {
                    std::vector<std::string> names = ctx->fid_databases[db_idx]->lookup_by_hash(hash);
                    if (!names.empty()) {
                        fid_names[func_addr] = names[0];
                        fid_matches++;
                        found_match = true;
                    }
                }
            } catch (...) {
                // Ignore errors
            }
        }
        
        if (fid_matches > 0) {
            result = apply_fid_names(result, fid_names);
        }
    }
    
    std::cerr << "[DecompilerCore] Decompilation complete, " << result.size() << " bytes after post-processing" << std::endl;
    
    return result;
}

void fission::ffi::set_gdt_path(DecompContext* ctx, const char* gdt_path) {
    if (!ctx) return;
    
    std::lock_guard<std::mutex> lock(ctx->mutex);
    ctx->gdt_path = gdt_path ? gdt_path : "";
}

void fission::ffi::set_feature(DecompContext* ctx, const char* feature, bool enabled) {
    if (!ctx || !feature) return;
    
    std::lock_guard<std::mutex> lock(ctx->mutex);
    
    std::string feat(feature);
    
    if (feat == "infer_pointers") {
        ctx->infer_pointers = enabled;
    } else if (feat == "analyze_loops") {
        ctx->analyze_loops = enabled;
    } else if (feat == "readonly_propagate") {
        ctx->readonly_propagate = enabled;
    }
}
