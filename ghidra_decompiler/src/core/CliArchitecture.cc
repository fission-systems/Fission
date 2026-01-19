#include "fission/core/CliArchitecture.h"
#include "fission/core/ScopeFission.h"
#include "fission/utils/logger.h"
#include "database.hh"
#include "flow.hh"

namespace fission {
namespace core {

// Constants
static const int MAX_INSTRUCTIONS = 200000;

CliArchitecture::CliArchitecture(const std::string& sleigh_id, ghidra::LoadImage* ldr, std::ostream* err)
    : ghidra::SleighArchitecture("", sleigh_id, err), custom_loader(ldr) {}

void CliArchitecture::buildLoader(ghidra::DocumentStorage& store) {
    loader = custom_loader;
}

ghidra::Scope* CliArchitecture::buildDatabase(ghidra::DocumentStorage& store) {
    (void)store;
    symboltab = new ghidra::Database(this, true);
    ghidra::Scope* global_scope = new ScopeFission(this, symbol_provider);
    symboltab->attachScope(global_scope, nullptr);
    return global_scope;
}

void CliArchitecture::injectIatSymbols(const std::map<uint64_t, std::string>& symbols) {
    if (symbols.empty()) return;
    
    ghidra::Scope* global_scope = symboltab->getGlobalScope();
    if (!global_scope) return;
    
    int injected = 0;
    std::vector<uint64_t> injected_addrs;
    for (const auto& [addr, name] : symbols) {
        try {
            ghidra::Address sym_addr(getDefaultCodeSpace(), addr);
            // Get or create function symbol
            ghidra::Funcdata* existing = global_scope->findFunction(sym_addr);
            if (existing == nullptr) {
                // Create external/import symbol as function
                global_scope->addFunction(sym_addr, name);
                injected++;
                injected_addrs.push_back(addr);
            }
        } catch (...) {
            // Ignore symbol injection errors
        }
    }
    
    if (injected > 0) {
        fission::utils::log_stream() << "[fission_core] Injected " << injected << " IAT symbols" << std::endl;
        fission::utils::log_stream() << "[fission_core] First few injected: ";
        for (size_t i = 0; i < std::min(size_t(5), injected_addrs.size()); i++) {
            fission::utils::log_stream() << "0x" << std::hex << injected_addrs[i] << std::dec << " ";
        }
        fission::utils::log_stream() << std::endl;
    }
}

void CliArchitecture::setSymbolProvider(const SymbolProvider* provider) {
    symbol_provider = provider;
}

void CliArchitecture::refreshReadOnly() {
    fillinReadOnlyFromLoader();
}

void configure_arch(CliArchitecture* arch) {
    arch->max_instructions = 500000; // Increased for Jump Table analysis (Phase 6)
    arch->flowoptions &= ~ghidra::FlowInfo::error_toomanyinstructions;
    arch->max_jumptable_size = 2048;
    arch->flowoptions |= ghidra::FlowInfo::record_jumploads;
    
    // === Analysis Improvements ===
    arch->infer_pointers = true;      // Infers pointers from constants (e.g. 0x401000 -> func_401000)
    arch->analyze_for_loops = true;   // Recovers for-loop structures
    arch->readonlypropagate = true;   // Propagates read-only memory as constants
    
    // === Advanced Ghidra Decompiler Options ===
    
    // 5. Output formatting options via PrintLanguage base class
    if (arch->print) {
        // Configure output options through base PrintLanguage class
        arch->print->setFlat(false);          // Use indentation
        arch->print->setIndentIncrement(2);   // 2 spaces per indent level
    }
}

} // namespace core
} // namespace fission
