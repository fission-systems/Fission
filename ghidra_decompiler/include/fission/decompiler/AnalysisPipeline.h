/**
 * Fission Decompiler Analysis Pipeline
 *
 * Runs structure/type/global/stack analysis passes after initial decompilation.
 *
 * Two entry points share the same implementation:
 *  - run_analysis_passes(ffi::DecompContext*, ...)  — FFI path
 *  - run_analysis_passes(BatchAnalysisContext&, ...) — batch path
 */
#ifndef FISSION_DECOMPILER_ANALYSIS_PIPELINE_H
#define FISSION_DECOMPILER_ANALYSIS_PIPELINE_H

#include <cstdint>
#include <map>
#include <set>
#include <string>
#include <vector>

namespace ghidra {
class Action;
class Architecture;
class Funcdata;
class TypeStruct;
}

namespace fission {
namespace ffi {
struct DecompContext;
}
namespace analysis {
class CallGraphAnalyzer;
}
namespace types {
struct GlobalTypeRegistry;
}

namespace decompiler {

struct AnalysisArtifacts {
    std::string inferred_struct_definitions;
    std::map<unsigned long long, ghidra::TypeStruct*> captured_structs;
};

// ---------------------------------------------------------------------------
// Batch analysis context — mirrors the FFI DecompContext fields that
// run_analysis_passes actually uses, but sourced from core::DecompilerContext.
// ---------------------------------------------------------------------------
struct BatchAnalysisContext {
    ghidra::Architecture*                               arch          = nullptr;
    fission::types::GlobalTypeRegistry*                 type_registry = nullptr;
    std::map<uint64_t, std::string>*                    symbols       = nullptr;  // iat_symbols
    std::map<uint64_t, std::map<int, std::string>>*     struct_registry = nullptr;
    std::vector<std::pair<uint64_t,uint64_t>>           executable_ranges;  // [start, end)
    uint64_t                                            data_start    = 0;
    uint64_t                                            data_end      = 0;
};

// ---------------------------------------------------------------------------
// FFI path (existing)
// ---------------------------------------------------------------------------
AnalysisArtifacts run_analysis_passes(
    fission::ffi::DecompContext* ctx,
    ghidra::Funcdata* fd,
    ghidra::Action* action,
    size_t max_function_size
);

// ---------------------------------------------------------------------------
// Batch path — same passes, different context source
// ---------------------------------------------------------------------------
AnalysisArtifacts run_analysis_passes(
    BatchAnalysisContext& ctx,
    ghidra::Funcdata* fd,
    ghidra::Action* action,
    size_t max_function_size
);

} // namespace decompiler
} // namespace fission

#endif // FISSION_DECOMPILER_ANALYSIS_PIPELINE_H
