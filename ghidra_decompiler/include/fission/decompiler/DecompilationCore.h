/**
 * Fission Decompiler Core
 *
 * Core decompilation pipeline (no FFI entrypoints).
 */
#ifndef FISSION_DECOMPILER_DECOMPILATION_CORE_H
#define FISSION_DECOMPILER_DECOMPILATION_CORE_H

#include <cstdint>
#include "fission/ffi/DecompContext.h"
#include <string>

namespace fission {
namespace decompiler {

void ensure_architecture(fission::ffi::DecompContext* ctx);

std::string run_decompilation(fission::ffi::DecompContext* ctx, uint64_t addr);

std::string run_decompilation_pcode(fission::ffi::DecompContext* ctx, uint64_t addr);

} // namespace decompiler
} // namespace fission

#endif // FISSION_DECOMPILER_DECOMPILATION_CORE_H
