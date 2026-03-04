/**
 * Fission Decompiler Analysis Pipeline
 *
 * Runs structure/type/global/stack analysis passes after initial decompilation.
 *
 * A single unified implementation is driven by the AnalysisContext interface.
 * Two concrete adapters (FfiAnalysisContext, BatchAnalysisAdapter) are provided
 * internally; the legacy overloads forward to the unified path.
 *
 *  - run_analysis_passes(ffi::DecompContext*, ...)   — FFI convenience wrapper
 *  - run_analysis_passes(BatchAnalysisContext&, ...) — batch convenience wrapper
 *  - run_analysis_passes(AnalysisContext&, ...)      — unified implementation
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
    std::string inferred_union_definitions;   ///< Phase 2: union type declarations
    std::map<unsigned long long, ghidra::TypeStruct*> captured_structs;
    // Dynamic field-offset map: offset-key → "struct_name.field_name"
    // Used by annotate_structure_offsets() for field annotation
    std::map<std::string, std::string> type_replacements;
    // GAP-4: addresses discovered from resolved jump/switch tables.
    // These should be enqueued for decompilation on the Rust side.
    std::vector<uint64_t> jump_table_targets;
};

// ===========================================================================
// AnalysisContext — abstract interface consumed by the unified pipeline.
// Concrete implementations (FfiAnalysisContext, BatchAnalysisAdapter) live in
// AnalysisPipeline.cpp and adapt the two existing context types.
// ===========================================================================
class AnalysisContext {
public:
    virtual ~AnalysisContext() = default;

    /// Underlying Ghidra Architecture object.
    virtual ghidra::Architecture* get_arch() = 0;

    /// IAT / imported symbol map (address → name).
    virtual const std::map<uint64_t, std::string>& get_symbols() = 0;

    /// Per-function struct field registry (nullable).
    virtual std::map<uint64_t, std::map<int, std::string>>* get_struct_registry() = 0;

    /// Cross-function type registry for call-graph analysis (nullable).
    virtual fission::types::GlobalTypeRegistry* get_type_registry() = 0;

    /// Populate [out_start, out_end) with the data-section VA range.
    /// Returns false if unavailable.
    virtual bool get_data_section_range(uint64_t& out_start, uint64_t& out_end) = 0;

    /// Return true if `addr` falls inside an executable section.
    virtual bool is_address_executable(uint64_t addr) = 0;

    /// Whether pointer-return prototype inference is available (FFI only).
    virtual bool has_pointer_return_inference() const = 0;

    /// Run pointer-return inference on the current function and its callees.
    /// Returns true if any prototypes were updated (triggers Stage-1 rerun).
    /// Default: no-op (batch path).
    virtual bool try_infer_pointer_returns(
        ghidra::Funcdata* /*fd*/, ghidra::Action* /*action*/) { return false; }

    /// Register the function's signature in the type registry for call-graph
    /// propagation.
    virtual void register_function_signature(ghidra::Funcdata* fd) = 0;
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
// Unified entry point (new — takes the abstract AnalysisContext interface)
// ---------------------------------------------------------------------------
AnalysisArtifacts run_analysis_passes(
    AnalysisContext& ctx,
    ghidra::Funcdata* fd,
    ghidra::Action* action,
    size_t max_function_size
);

// ---------------------------------------------------------------------------
// FFI convenience wrapper (existing API — delegates to unified path)
// ---------------------------------------------------------------------------
AnalysisArtifacts run_analysis_passes(
    fission::ffi::DecompContext* ctx,
    ghidra::Funcdata* fd,
    ghidra::Action* action,
    size_t max_function_size
);

// ---------------------------------------------------------------------------
// Batch convenience wrapper (existing API — delegates to unified path)
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
