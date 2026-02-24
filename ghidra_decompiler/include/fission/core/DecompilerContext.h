#ifndef FISSION_CORE_DECOMPILER_CONTEXT_H
#define FISSION_CORE_DECOMPILER_CONTEXT_H

#include <string>
#include <map>
#include <vector>
#include <cstdint>
#include "fission/loader/MemoryImage.h"
#include "fission/core/CliArchitecture.h"
#include "fission/types/GlobalTypeRegistry.h"
#include "fission/analysis/FidDatabase.h"

namespace fission {
namespace core {

class DecompilerContext {
public:
    bool initialized = false;
    std::string sla_dir;
    
    // Cached architecture objects
    fission::loader::MemoryLoadImage* loader_64bit = nullptr;
    fission::loader::MemoryLoadImage* loader_32bit = nullptr;
    CliArchitecture* arch_64bit = nullptr;
    CliArchitecture* arch_32bit = nullptr;
    
    bool arch_64bit_ready = false;
    bool arch_32bit_ready = false;
    
    // Store IAT symbols for post-processing
    std::map<uint64_t, std::string> iat_symbols;
    
    // Store enum/constant values for constant name substitution (value -> name)
    std::map<uint64_t, std::string> enum_values;

    // Store known GUIDs for substitution (UUID -> Name)
    std::map<std::string, std::string> guid_map;
    
    // FID-resolved function names (address -> name)
    std::map<uint64_t, std::string> fid_function_names;

    // VTable virtual call display names
    // key: vtable address, value: map(slot_offset -> display_name)
    std::map<uint64_t, std::map<int, std::string>> vtable_virtual_names;

    // Slot-only fallback hints derived from scanned vtables
    // key: slot_offset, value: display_name
    std::map<int, std::string> vcall_slot_name_hints;

    // Slot-only fallback call targets derived from scanned vtables
    // key: slot_offset, value: function address
    std::map<int, uint64_t> vcall_slot_target_hints;
    
    // FISSION IMPROVEMENT: Cached data section symbols (address -> (name, type_size, type_meta))
    struct DataSymbolInfo {
        std::string name;
        int size;          // 4 for float, 8 for double
        int type_meta;     // TYPE_FLOAT, etc.
    };
    std::map<uint64_t, DataSymbolInfo> data_section_symbols;
    bool data_symbols_scanned = false;

    // Data section range for GlobalDataAnalyzer (per-function analysis)
    uint64_t data_section_start = 0;
    uint64_t data_section_end = 0;

    // Executable section ranges populated from BinaryDetector::sections.
    // Used by BatchAnalysisContext to guard callgraph pending-reanalysis.
    std::vector<std::pair<uint64_t, uint64_t>> executable_ranges;

    // Cross-function type registry for CallGraphAnalyzer
    fission::types::GlobalTypeRegistry type_registry;

    // Struct field registry: address -> (field_offset -> field_name)
    // Previously global_struct_registry in DecompilationPipeline.cc
    std::map<uint64_t, std::map<int, std::string>> struct_registry;

    // Per-context FID databases for batch prologue scan (replaces static statics)
    std::vector<fission::analysis::FidDatabase> batch_fid_dbs;
    bool batch_fid_dbs_loaded = false;
    bool batch_fid_dbs_is64bit = false;

    DecompilerContext();
    ~DecompilerContext();
    
    // Initialize Ghidra library (only once)
    bool initialize(const std::string& sleigh_directory);

    // Setup architecture for a specific mode
    // sleigh_id: if non-empty, overrides the default arch_id (e.g. "AARCH64:LE:64:v8A")
    void setup_architecture(bool is_64bit, const std::vector<uint8_t>& bytes,
                            uint64_t image_base, const std::string& compiler_id,
                            const std::string& sleigh_id = "");
};

} // namespace core
} // namespace fission

#endif // FISSION_CORE_DECOMPILER_CONTEXT_H
