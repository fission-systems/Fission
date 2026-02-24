#include "fission/types/GlobalTypeRegistry.h"

#include <iostream>
#include "fission/utils/logger.h"

namespace fission {
namespace types {

GlobalTypeRegistry::GlobalTypeRegistry() {}
GlobalTypeRegistry::~GlobalTypeRegistry() {}

void GlobalTypeRegistry::register_function_types(uint64_t func_addr, const FunctionSignature& sig) {
    signatures[func_addr] = sig;
    signatures[func_addr].analyzed = true;
    
    fission::utils::log_stream() << "[GlobalTypeRegistry] Registered types for 0x" << std::hex << func_addr
              << " with " << std::dec << sig.params.size() << " params" << std::endl;
}

const FunctionSignature* GlobalTypeRegistry::get_function_signature(uint64_t func_addr) const {
    auto it = signatures.find(func_addr);
    if (it != signatures.end()) {
        return &it->second;
    }
    return nullptr;
}

void GlobalTypeRegistry::register_call(uint64_t caller, uint64_t callee, int call_addr) {
    CallSite site;
    site.caller_addr = caller;
    site.callee_addr = callee;
    site.call_instruction_addr = call_addr;
    call_sites.push_back(site);
    
    // Update reverse maps
    callers_map[callee].push_back(caller);
    callees_map[caller].push_back(callee);
}

std::vector<uint64_t> GlobalTypeRegistry::get_callers(uint64_t callee_addr) const {
    auto it = callers_map.find(callee_addr);
    if (it != callers_map.end()) {
        return it->second;
    }
    return {};
}

std::vector<uint64_t> GlobalTypeRegistry::get_callees(uint64_t caller_addr) const {
    auto it = callees_map.find(caller_addr);
    if (it != callees_map.end()) {
        return it->second;
    }
    return {};
}

bool GlobalTypeRegistry::is_analyzed(uint64_t func_addr) const {
    auto it = signatures.find(func_addr);
    return it != signatures.end() && it->second.analyzed;
}

void GlobalTypeRegistry::mark_for_reanalysis(uint64_t func_addr) {
    // O(1) insert — unordered_set ignores duplicates automatically.
    pending_reanalysis.insert(func_addr);

    // Mark as not analyzed
    if (signatures.count(func_addr)) {
        signatures[func_addr].analyzed = false;
    }
}

std::vector<uint64_t> GlobalTypeRegistry::get_pending_reanalysis() const {
    return std::vector<uint64_t>(pending_reanalysis.begin(), pending_reanalysis.end());
}

std::vector<uint64_t> GlobalTypeRegistry::consume_pending_reanalysis() {
    std::vector<uint64_t> result(pending_reanalysis.begin(), pending_reanalysis.end());
    pending_reanalysis.clear();
    return result;
}

void GlobalTypeRegistry::clear() {
    signatures.clear();
    call_sites.clear();
    callers_map.clear();
    callees_map.clear();
    pending_reanalysis.clear();  // unordered_set::clear() is O(n)
}

} // namespace types
} // namespace fission
