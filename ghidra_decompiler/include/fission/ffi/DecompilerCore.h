/**
 * Fission Decompiler Core
 * 
 * Core decompilation logic including architecture initialization,
 * decompilation execution, and post-processing.
 * Separated from libdecomp_ffi.cpp for better modularity.
 */

#ifndef FISSION_FFI_DECOMPILER_CORE_H
#define FISSION_FFI_DECOMPILER_CORE_H

#include <cstdint>
#include "fission/ffi/DecompContext.h"
#include <string>

namespace fission {
namespace ffi {

/**
 * Ensure architecture is initialized for the context
 * Lazy initialization - only creates architecture when first needed
 * @param ctx Decompiler context
 */
void ensure_architecture(DecompContext* ctx);

/**
 * Run decompilation for a function
 * @param ctx Decompiler context
 * @param addr Function address
 * @return Decompiled C code as string
 */
std::string run_decompilation(DecompContext* ctx, uint64_t addr);

/**
 * Run decompilation and return Pcode as JSON
 * @param ctx Decompiler context
 * @param addr Function address
 * @return Pcode JSON string
 */
std::string run_decompilation_pcode(DecompContext* ctx, uint64_t addr);

/**
 * Set GDT (Ghidra Data Type) path
 * @param ctx Decompiler context
 * @param gdt_path Path to GDT file
 */
void set_gdt_path(DecompContext* ctx, const char* gdt_path);

/**
 * Set a feature flag
 * @param ctx Decompiler context
 * @param feature Feature name
 * @param enabled Enable or disable
 */
void set_feature(DecompContext* ctx, const char* feature, bool enabled);

} // namespace ffi
} // namespace fission

#endif // FISSION_FFI_DECOMPILER_CORE_H
