/**
 * Fission Decompiler Core Implementation
 */

#include "fission/ffi/DecompilerCore.h"
#include "fission/loader/SectionAwareLoadImage.h"
#include "fission/core/CliArchitecture.h"
#include "fission/core/ArchPolicy.h"
#include "fission/core/SymbolProvider.h"
#include "fission/types/TypeManager.h"
#include "fission/types/PrototypeEnforcer.h"
#include "fission/types/GdtBinaryParser.h"
#include "fission/types/GuidParser.h"
#include "fission/analysis/FidDatabase.h"
#include "fission/analysis/CallingConvDetector.h"
#include "fission/processing/PostProcessors.h"
#include "fission/processing/StringScanner.h"
#include "fission/utils/file_utils.h"
#include "libdecomp.hh"
#include "address.hh"
#include "funcdata.hh"
#include "op.hh"
#include "varnode.hh"

#include <iostream>
#include <regex>
#include <set>
#include <cctype>

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

static std::string normalize_symbol_name(const std::string& name) {
    std::string norm = name;
    while (!norm.empty() && norm[0] == '_') {
        norm.erase(norm.begin());
    }
    for (char& ch : norm) {
        ch = static_cast<char>(std::tolower(static_cast<unsigned char>(ch)));
    }
    return norm;
}

static bool is_allocator_name(const std::string& name) {
    std::string norm = normalize_symbol_name(name);
    return norm == "malloc" || norm == "calloc" || norm == "realloc";
}

static bool is_address_in_executable(const fission::ffi::DecompContext* ctx, uint64_t addr) {
    for (const auto& block : ctx->memory_blocks) {
        if (!block.is_executable) {
            continue;
        }
        uint64_t size = block.va_size > 0 ? block.va_size : block.file_size;
        if (size == 0) {
            continue;
        }
        uint64_t start = block.va_addr;
        uint64_t end = start + size;
        if (addr >= start && addr < end) {
            return true;
        }
    }
    return false;
}

static bool same_high_var(ghidra::Varnode* lhs, ghidra::Varnode* rhs) {
    if (!lhs || !rhs) {
        return false;
    }
    ghidra::HighVariable* high_lhs = lhs->getHigh();
    ghidra::HighVariable* high_rhs = rhs->getHigh();
    if (high_lhs && high_rhs) {
        return high_lhs == high_rhs;
    }
    return lhs == rhs;
}

static bool flows_from_allocator(
    ghidra::Varnode* vn,
    const std::vector<ghidra::Varnode*>& alloc_returns,
    int depth
) {
    if (!vn || depth > 6) {
        return false;
    }
    for (auto* alloc : alloc_returns) {
        if (same_high_var(vn, alloc)) {
            return true;
        }
    }
    if (!vn->isWritten()) {
        return false;
    }
    ghidra::PcodeOp* def = vn->getDef();
    if (!def || def->isDead()) {
        return false;
    }
    switch (def->code()) {
        case ghidra::CPUI_COPY:
        case ghidra::CPUI_CAST:
        case ghidra::CPUI_PTRSUB:
        case ghidra::CPUI_PTRADD:
        case ghidra::CPUI_INT_ZEXT:
        case ghidra::CPUI_INT_SEXT:
            for (int slot = 0; slot < def->numInput(); ++slot) {
                if (flows_from_allocator(def->getIn(slot), alloc_returns, depth + 1)) {
                    return true;
                }
            }
            break;
        default:
            break;
    }
    return false;
}

static bool returns_allocator_result(
    ghidra::Funcdata* fd,
    const std::map<uint64_t, std::string>& symbols,
    ghidra::Architecture* arch
) {
    if (!fd) {
        return false;
    }

    std::vector<ghidra::Varnode*> alloc_returns;
    for (auto iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        ghidra::PcodeOp* op = *iter;
        if (!op || (op->code() != ghidra::CPUI_CALL && op->code() != ghidra::CPUI_CALLIND)) {
            continue;
        }
        std::string target_name;
        uint64_t target_addr = 0;
        if (ghidra::FuncCallSpecs* fc = fd->getCallSpecs(op)) {
            target_name = fc->getName();
            target_addr = fc->getEntryAddress().getOffset();
        }
        if (target_name.empty()) {
            ghidra::Varnode* target = op->getIn(0);
            if (target && target->isConstant()) {
                target_addr = target->getOffset();
            }
        }
        if (!target_name.empty()) {
            // keep name
        } else if (target_addr != 0) {
            auto name_it = symbols.find(target_addr);
            if (name_it != symbols.end()) {
                target_name = name_it->second;
            } else if (arch && arch->symboltab) {
                ghidra::Scope* scope = arch->symboltab->getGlobalScope();
                if (scope) {
                    ghidra::Funcdata* target_fd =
                        scope->findFunction(ghidra::Address(arch->getDefaultCodeSpace(), target_addr));
                    if (target_fd) {
                        target_name = target_fd->getName();
                    }
                }
            }
        }
        if (target_name.empty() || !is_allocator_name(target_name)) {
            continue;
        }
        ghidra::Varnode* out = op->getOut();
        if (out) {
            alloc_returns.push_back(out);
        }
    }

    if (alloc_returns.empty()) {
        return false;
    }

    for (auto iter = fd->beginOpAlive(); iter != fd->endOpAlive(); ++iter) {
        ghidra::PcodeOp* op = *iter;
        if (!op || op->code() != ghidra::CPUI_RETURN) {
            continue;
        }
        for (int slot = 0; slot < op->numInput(); ++slot) {
            ghidra::Varnode* ret = op->getIn(slot);
            if (flows_from_allocator(ret, alloc_returns, 0)) {
                return true;
            }
        }
    }

    return false;
}

static bool apply_pointer_return_prototype(ghidra::Architecture* arch, ghidra::Funcdata* fd) {
    if (!arch || !fd) {
        return false;
    }
    ghidra::FuncProto& proto = fd->getFuncProto();
    if (proto.isOutputLocked()) {
        return false;
    }
    ghidra::Datatype* outtype = proto.getOutputType();
    if (outtype && outtype->getMetatype() == ghidra::TYPE_PTR) {
        return false;
    }

    ghidra::TypeFactory* factory = arch->types;
    if (!factory) {
        return false;
    }

    ghidra::Datatype* void_type = factory->getTypeVoid();
    if (!void_type) {
        return false;
    }
    int4 ptr_size = factory->getSizeOfPointer();
    ghidra::Datatype* void_ptr = factory->getTypePointer(ptr_size, void_type, 0);
    if (!void_ptr) {
        return false;
    }

    ghidra::PrototypePieces pieces;
    proto.getPieces(pieces);
    pieces.outtype = void_ptr;
    proto.setPieces(pieces);
    proto.setInputLock(false);
    return true;
}

static bool infer_callee_pointer_returns(
    fission::ffi::DecompContext* ctx,
    ghidra::Funcdata* caller_fd,
    ghidra::Action* action
) {
    if (!ctx || !caller_fd || !action || !ctx->arch) {
        return false;
    }

    std::set<uint64_t> callee_addrs;
    for (auto iter = caller_fd->beginOpAlive(); iter != caller_fd->endOpAlive(); ++iter) {
        ghidra::PcodeOp* op = *iter;
        if (!op || (op->code() != ghidra::CPUI_CALL && op->code() != ghidra::CPUI_CALLIND)) {
            continue;
        }
        uint64_t target_addr = 0;
        if (ghidra::FuncCallSpecs* fc = caller_fd->getCallSpecs(op)) {
            target_addr = fc->getEntryAddress().getOffset();
        }
        if (target_addr == 0) {
            ghidra::Varnode* target = op->getIn(0);
            if (target && target->isConstant()) {
                target_addr = target->getOffset();
            }
        }
        if (target_addr == 0) {
            continue;
        }
        if (!is_address_in_executable(ctx, target_addr)) {
            continue;
        }
        callee_addrs.insert(target_addr);
    }

    if (callee_addrs.empty()) {
        return false;
    }

    bool updated = false;
    ghidra::Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
    if (!global_scope) {
        return false;
    }

    for (uint64_t addr : callee_addrs) {
        ghidra::Address func_addr(ctx->arch->getDefaultCodeSpace(), addr);
        ghidra::Funcdata* callee = global_scope->findFunction(func_addr);
        if (!callee) {
            ghidra::FunctionSymbol* sym = global_scope->addFunction(func_addr, "sub_" + std::to_string(addr));
            if (!sym) {
                continue;
            }
            callee = sym->getFunction();
        }
        if (!callee) {
            continue;
        }

        if (callee->isProcStarted() || callee->getFuncProto().isInline()) {
            continue;
        }

        auto sym_it = ctx->symbols.find(addr);
        if (sym_it != ctx->symbols.end() && is_allocator_name(sym_it->second)) {
            continue;
        }

        callee->clear();
        bool flow_ok = true;
        try {
            ghidra::Address start(func_addr);
            ghidra::Address end = start + 0x1000;
            callee->followFlow(start, end);
        } catch (const ghidra::LowlevelError&) {
            flow_ok = false;
        } catch (...) {
            flow_ok = false;
        }
        if (!flow_ok) {
            continue;
        }

        action->reset(*callee);
        action->perform(*callee);

        if (returns_allocator_result(callee, ctx->symbols, ctx->arch.get())) {
            if (apply_pointer_return_prototype(ctx->arch.get(), callee)) {
                updated = true;
            }
        }
    }

    return updated;
}

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

    if (!ctx->symbol_provider) {
        if (ctx->symbol_provider_enabled) {
            ctx->symbol_provider = std::make_unique<fission::core::CallbackSymbolProvider>(
                &ctx->symbol_provider_callbacks
            );
        } else {
            ctx->symbol_provider = std::make_unique<fission::core::MapSymbolProvider>(
                &ctx->symbols,
                &ctx->global_symbols
            );
        }
    }

    ctx->arch->setSymbolProvider(ctx->symbol_provider.get());
    
    // CRITICAL: Initialize Sleigh engine and register print languages
    ghidra::DocumentStorage store;
    ctx->arch->init(store);

    bool readonly_props_set = false;
    if (ctx->memory_image) {
        ghidra::AddrSpace* data_space = ctx->arch->getDefaultDataSpace();
        if (data_space) {
            ctx->memory_image->setDefaultSpace(data_space);
            ctx->arch->refreshReadOnly();
            readonly_props_set = true;
        }
    }
    
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
        if (!readonly_props_set && ctx->arch->symboltab) {
            ghidra::AddrSpace* data_space = ctx->arch->getDefaultDataSpace();
            if (data_space) {
                for (const auto& block : ctx->memory_blocks) {
                    uint64_t size = block.va_size > 0 ? block.va_size : block.file_size;
                    if (size == 0) {
                        continue;
                    }

                    ghidra::uintb start = block.va_addr;
                    ghidra::uintb last = start + static_cast<ghidra::uintb>(size - 1);
                    if (last < start) {
                        last = start;
                    }

                    uint4 flags = 0;
                    if (!block.is_writable) {
                        flags |= ghidra::Varnode::readonly;
                    }

                    if (flags != 0) {
                        ctx->arch->symboltab->setPropertyRange(
                            flags,
                            ghidra::Range(data_space, start, last)
                        );
                    }
                }
            }
        }

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
    
    std::cerr << "[DecompilerCore] Requesting decompilation for addr: 0x" << std::hex << addr << std::dec << std::endl;
    std::cerr << "[DecompilerCore] Created Address object: " << start_addr.getShortcut() << std::endl;
    start_addr.printRaw(std::cerr); std::cerr << std::endl;

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
    
    // ========================================================================
    // Calling Convention Application (Binary Format-based)
    // ========================================================================
    // For Windows PE binaries, force MS x64 calling convention
    // For Unix/Mac binaries (Mach-O, ELF), it will use System V ABI by default
    try {
        if (ctx->is_64bit) {
            // Assume Windows PE for now - can be extended with binary format detection
            ProtoModel* model = ctx->arch->getModel("__fastcall");  // MS x64
            if (model) {
                FuncProto& proto = fd->getFuncProto();
                proto.setModel(model);
                std::cerr << "[DecompilerCore] Applied MS x64 calling convention (__fastcall)" << std::endl;
            }
        }
    } catch (const std::exception& e) {
        std::cerr << "[DecompilerCore] ERROR applying calling convention: " << e.what() << std::endl;
    }
    
    // Check action group
    ghidra::Action* current_action = ctx->arch->allacts.getCurrent();
    if (!current_action) {
        throw std::runtime_error("No current action group");
    }

    // Enforce GDT-based and built-in prototypes before action reset.
    if (!ctx->symbols.empty()) {
        fission::types::PrototypeEnforcer proto_enforcer;
        proto_enforcer.enforce_iat_prototypes(ctx->arch.get(), ctx->symbols);
    }

    // CRITICAL: Reset action state for this function AFTER prototypes are applied
    std::cerr << "[DecompilerCore] Resetting action state..." << std::endl;
    current_action->reset(*fd);
    
    std::cerr << "[DecompilerCore] Performing decompilation..." << std::endl;
    
    // Perform decompilation
    try {
        current_action->perform(*fd);
    } catch (const ghidra::LowlevelError& e) {
        throw std::runtime_error("Ghidra LowlevelError: " + e.explain);
    } catch (const std::exception& e) {
        throw;
    } catch (...) {
        throw std::runtime_error("Unknown error during decompilation");
    }

    bool updated_self = false;
    try {
        if (returns_allocator_result(fd, ctx->symbols, ctx->arch.get())) {
            updated_self = apply_pointer_return_prototype(ctx->arch.get(), fd);
        }
    } catch (const ghidra::LowlevelError& e) {
        std::cerr << "[DecompilerCore] Pointer inference failed (self): " << e.explain << std::endl;
    } catch (const std::exception& e) {
        std::cerr << "[DecompilerCore] Pointer inference failed (self): " << e.what() << std::endl;
    } catch (...) {
        std::cerr << "[DecompilerCore] Pointer inference failed (self): unknown error" << std::endl;
    }

    bool updated_callee = false;
    try {
        updated_callee = infer_callee_pointer_returns(ctx, fd, current_action);
    } catch (const ghidra::LowlevelError& e) {
        std::cerr << "[DecompilerCore] Pointer inference failed (callee): " << e.explain << std::endl;
    } catch (const std::exception& e) {
        std::cerr << "[DecompilerCore] Pointer inference failed (callee): " << e.what() << std::endl;
    } catch (...) {
        std::cerr << "[DecompilerCore] Pointer inference failed (callee): unknown error" << std::endl;
    }
    if (updated_self || updated_callee) {
        std::cerr << "[DecompilerCore] Updated callee prototypes, re-running..." << std::endl;
        fd->clear();
        current_action->reset(*fd);
        try {
            current_action->perform(*fd);
        } catch (const ghidra::LowlevelError& e) {
            throw std::runtime_error("Ghidra LowlevelError: " + e.explain);
        } catch (const std::exception& e) {
            throw;
        } catch (...) {
            throw std::runtime_error("Unknown error during decompilation");
        }
    }
    
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

    // Step 8.5: Apply global data symbol names (g_/gp_)
    if (!ctx->global_symbols.empty()) {
        result = apply_global_symbols(result, ctx->global_symbols);
    }
    
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

// Simple AssemblyEmit implementation for capturing disassembly
class SimpleAssemblyEmit : public ghidra::AssemblyEmit {
    std::string mnemonic_;
    std::string body_;
    
public:
    virtual void dump(const ghidra::Address& addr, const std::string& mnem, const std::string& body) override {
        mnemonic_ = mnem;
        body_ = body;
    }
    
    const std::string& getMnemonic() const { return mnemonic_; }
    const std::string& getBody() const { return body_; }
};

std::string fission::ffi::run_decompilation_pcode(DecompContext* ctx, uint64_t addr) {
    if (!ctx) return "{}";
    
    ensure_architecture(ctx);
    
    if (!ctx->arch->symboltab) throw std::runtime_error("Symbol table not initialized");
    ghidra::Scope* global_scope = ctx->arch->symboltab->getGlobalScope();
    if (!global_scope) throw std::runtime_error("Global scope not initialized");
    
    ghidra::AddrSpace* code_space = ctx->arch->getDefaultCodeSpace();
    if (!code_space) throw std::runtime_error("Code space not initialized");
    ghidra::Address start_addr(code_space, addr);
    
    ghidra::Funcdata* fd = global_scope->findFunction(start_addr);
    if (!fd) {
        std::string func_name = "sub_" + std::to_string(addr);
        ghidra::FunctionSymbol* sym = global_scope->addFunction(start_addr, func_name);
        if (!sym) throw std::runtime_error("Failed to add function");
        fd = sym->getFunction();
    }
    
    if (!fd) throw std::runtime_error("Failed to get function data");
    
    fd->clear();
    
    ghidra::Address end_addr = start_addr + 0x10000;
    try {
        fd->followFlow(start_addr, end_addr);
    } catch (...) {}
    
    ghidra::Action* current_action = ctx->arch->allacts.getCurrent();
    if (!current_action) throw std::runtime_error("No current action group");
    
    try {
        // Clear only this function's data for fresh analysis
        fd->clear();
        
        // Follow control flow to discover instructions
        // We use a reasonable limit for the end address
        ghidra::Address end_addr = start_addr + 0x10000; 
        fd->followFlow(start_addr, end_addr);
        
        current_action->reset(*fd);
        current_action->perform(*fd);
        
        std::ostringstream json;
        json << "{";
        json << "\"blocks\": [";
        
        const ghidra::BlockGraph& basic_blocks = fd->getBasicBlocks();
        for (int i = 0; i < basic_blocks.getSize(); ++i) {
            ghidra::FlowBlock* block = basic_blocks.getBlock(i);
            ghidra::BlockBasic* bb = static_cast<ghidra::BlockBasic*>(block);
            
            if (i > 0) json << ",";
            
            json << "{";
            json << "\"index\": " << block->getIndex() << ",";
            json << "\"start_addr\": \"0x" << std::hex << block->getStart().getOffset() << "\",";
            json << "\"ops\": [";
            
            bool first_op = true;
            auto iter = bb->beginOp();
            auto end_iter = bb->endOp();
            
            for (; iter != end_iter; ++iter) {
                ghidra::PcodeOp* op = *iter;
                if (!op) continue;
                
                if (!first_op) json << ",";
                first_op = false;
                
                json << "{";
                json << "\"seq\": " << op->getSeqNum().getTime() << ",";
                json << "\"opcode\": \"" << op->getOpcode()->getName() << "\",";
                json << "\"addr\": \"0x" << std::hex << op->getAddr().getOffset() << "\",";
                
                // Try to get assembly mnemonic
                try {
                    ghidra::Address asm_addr = op->getAddr();
                    SimpleAssemblyEmit asm_emit;
                    ctx->arch->translate->printAssembly(asm_emit, asm_addr);
                    std::string mnemonic = asm_emit.getMnemonic();
                    std::string body = asm_emit.getBody();
                    if (!mnemonic.empty()) {
                        if (!body.empty()) {
                            json << "\"asm\": \"" << mnemonic << " " << body << "\",";
                        } else {
                            json << "\"asm\": \"" << mnemonic << "\",";
                        }
                    } else {
                        json << "\"asm\": null,";
                    }
                } catch (...) {
                    json << "\"asm\": null,";
                }
                
                ghidra::Varnode* out = op->getOut();
                if (out) {
                    json << "\"output\": {";
                    json << "\"offset\": \"0x" << std::hex << out->getOffset() << "\",";
                    json << "\"size\": " << std::dec << out->getSize() << ",";
                    json << "\"space\": " << out->getSpace()->getType() << ","; // Use type ID for space
                    json << "\"const_val\": " << (out->isConstant() ? std::to_string(out->getOffset()) : "null");
                    json << "},";
                } else {
                    json << "\"output\": null,";
                }
                
                json << "\"inputs\": [";
                for (int j = 0; j < op->numInput(); ++j) {
                    ghidra::Varnode* in = op->getIn(j);
                    if (j > 0) json << ",";
                    json << "{";
                    json << "\"offset\": \"0x" << std::hex << in->getOffset() << "\",";
                    json << "\"size\": " << std::dec << in->getSize() << ",";
                    json << "\"space\": " << in->getSpace()->getType() << ",";
                    json << "\"const_val\": " << (in->isConstant() ? std::to_string(in->getOffset()) : "null");
                    json << "}";
                }
                json << "]";
                json << "}";
            }
            json << "]";
            json << "}";
        }
        
        json << "]";
        json << "}";
        
        return json.str();
    } catch (const LowlevelError& e) {
        throw std::runtime_error("Ghidra LowlevelError: " + e.explain);
    } catch (const std::exception& e) {
        throw std::runtime_error("Decompilation error: " + std::string(e.what()));
    } catch (...) {
        throw std::runtime_error("Unknown decompilation error in run_decompilation_pcode");
    }
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
