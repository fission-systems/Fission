/**
 * Fission Decompiler Core
 *
 * Core decompilation pipeline (no FFI entrypoints).
 */
#ifndef FISSION_DECOMPILER_DECOMPILATION_CORE_H
#define FISSION_DECOMPILER_DECOMPILATION_CORE_H

#include <cstdint>
#include "fission/ffi/DecompContext.h"
#include "fission/decompiler/AnalysisPipeline.h"
#include <string>

namespace fission {
namespace decompiler {

void ensure_architecture(fission::ffi::DecompContext* ctx);

/**
 * Run decompilation. Optionally returns analysis artifacts when out_artifacts is non-null.
 */
std::string run_decompilation(
    fission::ffi::DecompContext* ctx,
    uint64_t addr,
    AnalysisArtifacts* out_artifacts = nullptr);

/**
 * Run decompilation and return JSON with both code and inferred type metadata.
 * Format: {"code":"...","inferred_types":[{...}]}
 * Enables Rust replace_field_offsets to use StructureAnalyzer results.
 */
std::string run_decompilation_with_metadata(fission::ffi::DecompContext* ctx, uint64_t addr);

std::string run_decompilation_pcode(fission::ffi::DecompContext* ctx, uint64_t addr);

} // namespace decompiler
} // namespace fission

#endif // FISSION_DECOMPILER_DECOMPILATION_CORE_H
