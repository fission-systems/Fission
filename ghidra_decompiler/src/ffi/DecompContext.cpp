/**
 * Fission Decompiler Context Implementation
 */

#include "fission/ffi/DecompContext.h"
#include "fission/core/CliArchitecture.h"
#include "fission/analysis/FunctionMatcher.h"
#include "fission/analysis/FidDatabase.h"
#include "libdecomp.hh"
#include "sleigh_arch.hh"

#include <iostream>
#include "fission/utils/logger.h"

using namespace fission::ffi;
using namespace fission::core;
using namespace fission::analysis;

// Static initialization state
static bool ghidra_library_initialized = false;
static std::mutex init_mutex;

// ============================================================================
// DecompContext Implementation
// ============================================================================

DecompContext::DecompContext(const char* sla) 
    : sla_dir(sla ? sla : "")
{
    matcher = std::make_unique<FunctionMatcher>();
    symbol_provider_callbacks = DecompSymbolProvider{};
}

DecompContext::~DecompContext() {
    if (symbol_provider_enabled && symbol_provider_callbacks.drop) {
        symbol_provider_callbacks.drop(symbol_provider_callbacks.userdata);
    }
    // Ghidra's Architecture destructor chain has known crash issues when
    // cleaning up after full decompilation (dangling pointers in global
    // scope / symbol table / type system).  The try/catch only catches
    // C++ exceptions, not signal-based crashes (SIGSEGV).
    //
    // Unconditionally *release* instead of destroying:
    //  - For one-shot CLI: process exit reclaims all memory anyway.
    //  - For long-running GUI: each binary typically gets its own context,
    //    and the leaked Architecture is a few hundred KB at most.
    //
    // Proper fix would require auditing Ghidra's internal destructor
    // chain, which is out of scope for now.
    if (arch) {
        arch.release();
    }
}

// ============================================================================
// Lifecycle Functions
// ============================================================================

bool fission::ffi::initialize_ghidra_library(const std::string& sla_dir) {
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
        fission::utils::log_stream() << "[DecompContext] Ghidra library initialized with specpath: " << langDir << std::endl;
        return true;
    } catch (const ghidra::LowlevelError& e) {
        fission::utils::log_stream() << "[DecompContext] Failed to init Ghidra: " << e.explain << std::endl;
        return false;
    } catch (...) {
        fission::utils::log_stream() << "[DecompContext] Unknown error during Ghidra init" << std::endl;
        return false;
    }
}

DecompContext* fission::ffi::create_context(const char* sla_dir) {
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

void fission::ffi::destroy_context(DecompContext* ctx) {
    delete ctx;
}
