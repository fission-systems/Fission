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

    // === Core analysis flags ===
    arch->infer_pointers = true;      // Infers pointers from constants (e.g. 0x401000 -> func_401000)
    arch->analyze_for_loops = true;   // Recovers for-loop structures
    arch->readonlypropagate = true;   // Propagates read-only memory as constants

    // === Type / cast quality ===
    // Aggressively trim sign-extended inputs: removes spurious (int)(char)x casts
    // that appear when Ghidra conservatively preserves sign-extension ops.
    arch->aggressive_ext_trim = true;

    // Suppress NaN guard expressions around float comparisons.
    // Real C code virtually never has explicit NaN checks; these are artifacts
    // introduced by the float semantics model.  Ignoring them produces cleaner output.
    arch->nan_ignore_all = true;

    // === Alias / pointer analysis ===
    // alias_block_level: 0=none, 1=struct (default), 2=array, 3=all
    // Level 2 prevents the alias analysis from treating array elements as
    // potentially aliasing each other, which catches more dead-store /
    // copy-propagation opportunities inside loops.
    arch->alias_block_level = 2;

    // === Data-type splitting ===
    // split_datatype_config bits: 0=structs, 1=arrays, 2=pointers
    // Bit 0: allow struct arguments to be split into individual fields at call sites.
    // This dramatically improves readability for functions that accept small structs
    // by value (common in x64 Windows ABI, e.g. POINT, RECT, LARGE_INTEGER).
    arch->split_datatype_config = 0x1;

    // === Parameter trimming depth ===
    // How deeply the decompiler recurses while trimming unnecessary parameter wrappers.
    // Default is 4; raising to 7 helps with deeply-nested helper wrappers.
    arch->trim_recurse_max = 7;

    // === Output formatting ===
    if (arch->print) {
        arch->print->setFlat(false);          // Use indentation
        arch->print->setIndentIncrement(2);   // 2 spaces per indent level
        arch->print->setMaxLineSize(120);     // Wider lines before wrapping (default ~80)
        // NO_NAMESPACES: suppress all namespace qualifiers from the output.
        // For binary decompilation we have no source-level namespace context,
        // so any emitted namespace tokens are noise (e.g. "ghidra::").
        arch->print->setNamespaceStrategy(ghidra::PrintLanguage::NO_NAMESPACES);
    }
}

} // namespace core
} // namespace fission
